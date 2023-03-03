use crate::{
    flow_context::FlowContext,
    v5::{
        ibd::{HeadersChunkStream, TrustedEntryStream},
        Flow,
    },
};
use consensus_core::{
    api::{BlockValidationFuture, DynConsensus},
    block::Block,
    pruning::{PruningPointProof, PruningPointsList},
};
use futures::future::join_all;
use hashes::Hash;
use kaspa_core::{debug, info};
use muhash::MuHash;
use p2p_lib::{
    common::ProtocolError,
    convert::model::trusted::TrustedDataPackage,
    dequeue_with_timeout, make_message,
    pb::{
        kaspad_message::Payload, RequestHeadersMessage, RequestIbdBlocksMessage, RequestPruningPointAndItsAnticoneMessage,
        RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage,
    },
    IncomingRoute, Router,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc::Receiver;

use super::{PruningPointUtxosetChunkStream, IBD_BATCH_SIZE};

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    pub(super) ctx: FlowContext,
    pub(super) router: Arc<Router>,
    pub(super) incoming_route: IncomingRoute,

    // Receives relay blocks from relay flow which are out of orphan resolution range and hence trigger IBD
    relay_receiver: Receiver<Block>,
}

#[async_trait::async_trait]
impl Flow for IbdFlow {
    fn name(&self) -> &'static str {
        "IBD"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

pub enum IbdType {
    #[allow(dead_code)]
    None,
    Sync(Hash),
    DownloadHeadersProof,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, relay_receiver: Receiver<Block>) -> Self {
        Self { ctx, router, incoming_route, relay_receiver }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        while let Some(relay_block) = self.relay_receiver.recv().await {
            if let Some(_guard) = self.ctx.try_set_ibd_running() {
                info!("IBD started with peer {}", self.router);

                let consensus = self.ctx.consensus();
                let negotiation_output = self.negotiate_missing_syncer_chain_segment(&consensus).await?;
                match self.decide_ibd_type(&consensus, &relay_block, negotiation_output.highest_known_syncer_chain_hash)? {
                    IbdType::None => continue,
                    IbdType::Sync(highest_known_syncer_chain_hash) => {
                        // TODO: check config conditions
                        self.sync_pruning_point_future_headers(
                            &consensus,
                            negotiation_output.syncer_header_selected_tip,
                            highest_known_syncer_chain_hash,
                        )
                        .await?;
                    }
                    IbdType::DownloadHeadersProof => {
                        self.perform_ibd_with_headers_proof(&consensus, negotiation_output.syncer_header_selected_tip).await?;
                    }
                }
                self.sync_missing_block_bodies(&consensus, negotiation_output.syncer_header_selected_tip).await?;

                // TODO: make sure a message is printed also on errors
                info!("IBD with peer {} finished", self.router);
            }
        }

        Ok(())
    }

    fn decide_ibd_type(
        &self,
        consensus: &DynConsensus,
        _relay_block: &Block,
        highest_known_syncer_chain_hash: Option<Hash>,
    ) -> Result<IbdType, ProtocolError> {
        let Some(pp) = consensus.pruning_point() else {
            // TODO: fix when applying staging consensus
            return Ok(IbdType::DownloadHeadersProof);
        };

        Ok(if let Some(highest_known_syncer_chain_hash) = highest_known_syncer_chain_hash {
            if consensus.is_chain_ancestor_of(pp, highest_known_syncer_chain_hash)? {
                IbdType::Sync(highest_known_syncer_chain_hash)
            } else {
                IbdType::DownloadHeadersProof
            }
        } else {
            IbdType::DownloadHeadersProof
        })

        // TODO: full imp with blue work check
    }

    async fn perform_ibd_with_headers_proof(
        &mut self,
        consensus: &DynConsensus,
        syncer_header_selected_tip: Hash,
    ) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof");
        let pruning_point = self.sync_and_validate_pruning_proof(consensus).await?;
        self.sync_pruning_point_future_headers(consensus, syncer_header_selected_tip, pruning_point).await?;
        self.sync_pruning_point_utxoset(consensus, pruning_point).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, consensus: &DynConsensus) -> Result<Hash, ProtocolError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = msg.try_into()?;
        debug!("received proof with overall {} headers", proof.iter().map(|l| l.len()).sum::<usize>());

        // TODO: call validate_pruning_proof when implemented
        // consensus.clone().validate_pruning_proof(&proof);

        let proof_pruning_point = proof[0].last().expect("was just ensured by validation").hash;

        // TODO: verify the proof pruning point is different than current consensus pruning point

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let pruning_points: PruningPointsList = msg.try_into()?;

        if pruning_points.is_empty() || pruning_points.last().unwrap().hash != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the last pruning point in the list"));
        }

        // TODO: validate pruning points before importing

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = msg.try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route);
        let Some(pruning_point_entry) = entry_stream.next().await? else { return Err(ProtocolError::Other("got `done` message before receiving the pruning point")); };

        // TODO: verify trusted pruning point matches proof pruning point

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }

        // TODO: logs
        let trusted_set = pkg.build_trusted_subdag(entries)?;
        consensus.clone().apply_pruning_proof(proof, &trusted_set);
        consensus.clone().import_pruning_points(pruning_points);

        info!("Starting to process {} semi-trusted blocks", trusted_set.len());
        let mut last_time = std::time::SystemTime::now();
        let mut last_index: usize = 0;
        for (i, tb) in trusted_set.into_iter().enumerate() {
            let now = std::time::SystemTime::now();
            let passed = now.duration_since(last_time).unwrap();
            if passed > Duration::from_secs(1) {
                info!("Processed {} semi-trusted blocks in the last {} seconds (total {})", i - last_index, passed.as_secs(), i);
                last_time = now;
                last_index = i;
            }
            // TODO: queue all and join
            consensus.clone().validate_and_insert_trusted_block(tb).await?;
        }
        info!("Done processing semi-trusted blocks");

        // TODO: make sure that the proof pruning point is not genesis

        Ok(proof_pruning_point)
    }

    async fn sync_pruning_point_future_headers(
        &mut self,
        consensus: &DynConsensus,
        syncer_header_selected_tip: Hash,
        highest_known_syncer_chain_hash: Hash,
    ) -> Result<(), ProtocolError> {
        // TODO: sync missing relay block past \cap anticone(syncer tip)

        self.router
            .enqueue(make_message!(
                Payload::RequestHeaders,
                RequestHeadersMessage {
                    low_hash: Some(highest_known_syncer_chain_hash.into()),
                    high_hash: Some(syncer_header_selected_tip.into())
                }
            ))
            .await?;
        let mut chunk_stream = HeadersChunkStream::new(&self.router, &mut self.incoming_route);

        let Some(chunk) = chunk_stream.next().await? else { return Ok(()); };
        let mut prev_joins: Vec<BlockValidationFuture> =
            chunk.into_iter().map(|h| consensus.clone().validate_and_insert_block(Block::from_header_arc(h), false)).collect();

        // TODO: logs
        while let Some(chunk) = chunk_stream.next().await? {
            let current_joins =
                chunk.into_iter().map(|h| consensus.clone().validate_and_insert_block(Block::from_header_arc(h), false)).collect();
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            join_all(prev_joins).await.into_iter().try_for_each(|x| x.map(drop))?;
            prev_joins = current_joins;
        }

        join_all(prev_joins).await.into_iter().try_for_each(|x| x.map(drop))?;

        Ok(())
    }

    async fn sync_pruning_point_utxoset(&mut self, consensus: &DynConsensus, pruning_point: Hash) -> Result<(), ProtocolError> {
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointUtxoSet,
                RequestPruningPointUtxoSetMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let mut chunk_stream = PruningPointUtxosetChunkStream::new(&self.router, &mut self.incoming_route);
        let mut multiset = MuHash::new();
        while let Some(chunk) = chunk_stream.next().await? {
            consensus.append_imported_pruning_point_utxos(&chunk, &mut multiset);
        }
        consensus.import_pruning_point_utxo_set(pruning_point, &mut multiset)?;
        Ok(())
    }

    async fn sync_missing_block_bodies(&mut self, consensus: &DynConsensus, high: Hash) -> Result<(), ProtocolError> {
        // TODO: query consensus in batches
        let hashes = consensus.get_missing_block_body_hashes(high)?;
        if hashes.is_empty() {
            return Ok(());
        }
        // TODO: progress reporter using DAA score from below headers
        let _low_header = consensus.get_header(*hashes.first().unwrap())?;
        let _high_header = consensus.get_header(*hashes.last().unwrap())?;

        let mut iter = hashes.chunks(IBD_BATCH_SIZE);
        let mut prev_jobs = self.queue_block_processing_chunk(consensus, iter.next().expect("hashes was non empty")).await?;

        // TODO: logs
        for (i, chunk) in iter.enumerate() {
            let current_jobs = self.queue_block_processing_chunk(consensus, chunk).await?;
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            join_all(prev_jobs).await.into_iter().try_for_each(|x| x.map(drop))?;
            prev_jobs = current_jobs;

            if i % 5 == 0 {
                info!("Processed {} block bodies", (i + 1) * IBD_BATCH_SIZE);
            }
        }

        join_all(prev_jobs).await.into_iter().try_for_each(|x| x.map(drop))?;

        // TODO: raise new block template event

        Ok(())
    }

    async fn queue_block_processing_chunk(
        &mut self,
        consensus: &DynConsensus,
        chunk: &[Hash],
    ) -> Result<Vec<BlockValidationFuture>, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdBlocks,
                RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
            let block: Block = msg.try_into()?;
            if block.hash() != expected_hash {
                return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", expected_hash, block.hash())));
            }
            if block.is_header_only() {
                return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
            }
            // TODO: decide if we resolve virtual separately on long IBD
            jobs.push(consensus.validate_and_insert_block(block, true));
        }

        Ok(jobs)
    }
}
