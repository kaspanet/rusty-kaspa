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
        kaspad_message::Payload, RequestHeadersMessage, RequestIbdBlocksMessage, RequestIbdChainBlockLocatorMessage,
        RequestPruningPointAndItsAnticoneMessage, RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage,
    },
    IncomingRoute, Router,
};
use std::{sync::Arc, time::Duration};

use super::{PruningPointUtxosetChunkStream, IBD_BATCH_SIZE};

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
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

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        info!("Started IBD");
        // None hashes indicate that the full chain is queried.
        let block_locator = self.get_syncer_chain_block_locator(None, None).await?;
        if block_locator.is_empty() {
            info!("Can't IBD from this peer");
            return Ok(()); // TODO: Consider uncommenting once IBD flow is finalized
                           // return Err(ProtocolError::Other("Expecting initial syncer chain block locator to contain at least one element"));
        }
        let syncer_header_selected_tip = *block_locator.first().expect("verified locator is not empty");
        self.start_ibd_with_headers_proof(syncer_header_selected_tip).await?;
        info!("Finished IBD");
        Ok(())
    }

    async fn get_syncer_chain_block_locator(
        &mut self,
        low_hash: Option<Hash>,
        high_hash: Option<Hash>,
    ) -> Result<Vec<Hash>, ProtocolError> {
        // TODO: use low and high hashes when zooming in
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdChainBlockLocator,
                RequestIbdChainBlockLocatorMessage { low_hash: low_hash.map(|h| h.into()), high_hash: high_hash.map(|h| h.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdChainBlockLocator)?;
        if msg.block_locator_hashes.len() > 64 {
            return Err(ProtocolError::Other(
                "Got block locator of size > 64 while expecting
 locator to have size which is logarithmic in DAG size (which should never exceed 2^64)",
            ));
        }
        Ok(msg.try_into()?)
    }

    async fn start_ibd_with_headers_proof(&mut self, syncer_header_selected_tip: Hash) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof");
        let consensus = self.ctx.consensus();
        let pruning_point = self.sync_and_validate_pruning_proof(&consensus).await?;
        self.sync_pruning_point_future_headers(&consensus, syncer_header_selected_tip, pruning_point).await?;
        self.sync_pruning_point_utxoset(&consensus, pruning_point).await?;
        self.sync_missing_block_bodies(&consensus, syncer_header_selected_tip).await?;
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
        let hashes = consensus.get_missing_block_body_hashes(high)?;
        if hashes.is_empty() {
            return Ok(());
        }
        // TODO: progress reporter using DAA score from below headers
        let _low_header = consensus.get_header(*hashes.first().unwrap())?;
        let _high_header = consensus.get_header(*hashes.last().unwrap())?;
        for chunk in hashes.chunks(IBD_BATCH_SIZE) {
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
                consensus.validate_and_insert_block(block, true).await?;

                // TODO: raise new block event or make sure consensus does
            }
        }

        // TODO: raise new block template event

        Ok(())
    }
}
