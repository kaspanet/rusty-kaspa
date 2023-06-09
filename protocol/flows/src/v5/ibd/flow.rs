use crate::{
    flow_context::FlowContext,
    v5::{
        ibd::{HeadersChunkStream, TrustedEntryStream},
        Flow,
    },
};
use futures::future::try_join_all;
use kaspa_consensus_core::{
    api::{BlockValidationFuture, ConsensusApi},
    block::Block,
    blockhash::BlockHashExtensions,
    header::Header,
    pruning::{PruningPointProof, PruningPointsList},
    BlockHashSet,
};
use kaspa_consensusmanager::StagingConsensus;
use kaspa_core::{debug, info};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    convert::model::trusted::TrustedDataPackage,
    dequeue_with_timeout, make_message,
    pb::{
        kaspad_message::Payload, RequestAnticoneMessage, RequestHeadersMessage, RequestIbdBlocksMessage,
        RequestPruningPointAndItsAnticoneMessage, RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage,
    },
    IncomingRoute, Router,
};
use std::{
    ops::Deref,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::mpsc::Receiver;

use super::{progress::ProgressReporter, HeadersChunk, PruningPointUtxosetChunkStream, IBD_BATCH_SIZE};

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
    None,
    Sync(Hash),
    DownloadHeadersProof,
}

// TODO: define a peer banning strategy

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, relay_receiver: Receiver<Block>) -> Self {
        Self { ctx, router, incoming_route, relay_receiver }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        while let Some(relay_block) = self.relay_receiver.recv().await {
            if let Some(_guard) = self.ctx.try_set_ibd_running(self.router.key()) {
                info!("IBD started with peer {}", self.router);

                match self.ibd(relay_block).await {
                    Ok(_) => info!("IBD with peer {} completed successfully", self.router),
                    Err(e) => {
                        info!("IBD with peer {} completed with error: {}", self.router, e);
                        return Err(e);
                    }
                }
            }
        }

        Ok(())
    }

    async fn ibd(&mut self, relay_block: Block) -> Result<(), ProtocolError> {
        let mut session = self.ctx.consensus().session_owned().await;

        let negotiation_output = self.negotiate_missing_syncer_chain_segment(session.deref()).await?;
        let ibd_type =
            self.determine_ibd_type(session.deref(), &relay_block.header, negotiation_output.highest_known_syncer_chain_hash)?;
        match ibd_type {
            IbdType::None => return Ok(()),
            IbdType::Sync(highest_known_syncer_chain_hash) => {
                self.sync_headers(
                    session.deref(),
                    negotiation_output.syncer_header_selected_tip,
                    highest_known_syncer_chain_hash,
                    &relay_block,
                )
                .await?;
            }
            IbdType::DownloadHeadersProof => {
                drop(session); // Avoid holding the previous consensus throughout the staging IBD
                let staging = self.ctx.consensus_manager.new_staging_consensus();
                match self.ibd_with_headers_proof(&staging, negotiation_output.syncer_header_selected_tip, &relay_block).await {
                    Ok(()) => {
                        staging.commit();
                        self.ctx.on_pruning_point_utxoset_override();
                        // This will reobtain the freshly committed staging consensus
                        session = self.ctx.consensus().session_owned().await;
                    }
                    Err(e) => {
                        staging.cancel();
                        return Err(e);
                    }
                }
            }
        }

        // Sync missing bodies in the past of syncer selected tip
        self.sync_missing_block_bodies(session.deref(), negotiation_output.syncer_header_selected_tip).await?;

        // Relay block might be in the anticone of syncer selected tip, thus
        // check its chain for missing bodies as well.
        self.sync_missing_block_bodies(session.deref(), relay_block.hash()).await
    }

    fn determine_ibd_type(
        &self,
        consensus: &dyn ConsensusApi,
        relay_header: &Header,
        highest_known_syncer_chain_hash: Option<Hash>,
    ) -> Result<IbdType, ProtocolError> {
        let Some(pruning_point) = consensus.pruning_point() else {
            // TODO: fix when applying staging consensus
            return Ok(IbdType::DownloadHeadersProof);
        };

        if let Some(highest_known_syncer_chain_hash) = highest_known_syncer_chain_hash {
            if consensus.is_chain_ancestor_of(pruning_point, highest_known_syncer_chain_hash)? {
                // The node is only missing a segment in the future of its current pruning point, and the chains
                // agree as well, so we perform a simple sync IBD and only download the missing data
                return Ok(IbdType::Sync(highest_known_syncer_chain_hash));
            }

            // TODO: in this case we know a syncer chain block, but it violates our current finality. In some cases
            // this info should possibly be used to reject the IBD despite having more blue work etc.
        }

        let hst_hash = consensus.get_headers_selected_tip();
        // TODO: remove when applying staging consensus
        if hst_hash.is_origin() {
            return Ok(IbdType::DownloadHeadersProof);
        }
        let hst_header = consensus.get_header(hst_hash).unwrap();
        if relay_header.blue_score >= hst_header.blue_score + self.ctx.config.pruning_depth
            && relay_header.blue_work > hst_header.blue_work
        {
            // The relayed block has sufficient blue score and blue work over the current header selected tip
            Ok(IbdType::DownloadHeadersProof)
        } else {
            Ok(IbdType::None)
        }
    }

    async fn ibd_with_headers_proof(
        &mut self,
        staging: &StagingConsensus,
        syncer_header_selected_tip: Hash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof with peer {}", self.router);

        let session = staging.session().await;
        let consensus = session.deref();

        let pruning_point = self.sync_and_validate_pruning_proof(consensus).await?;
        self.sync_headers(consensus, syncer_header_selected_tip, pruning_point, relay_block).await?;
        self.validate_staging_timestamps(self.ctx.consensus().session().await.deref(), consensus)?;
        self.sync_pruning_point_utxoset(consensus, pruning_point).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, consensus: &dyn ConsensusApi) -> Result<Hash, ProtocolError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = msg.try_into()?;
        debug!("received proof with overall {} headers", proof.iter().map(|l| l.len()).sum::<usize>());

        consensus.validate_pruning_proof(&proof)?;

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

        if pruning_points.first().unwrap().hash != self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the first pruning point in the list is expected to be genesis"));
        }

        // TODO: validate pruning points before importing

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = msg.try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route);
        let Some(pruning_point_entry) = entry_stream.next().await? else { return Err(ProtocolError::Other("got `done` message before receiving the pruning point")); };

        if pruning_point_entry.block.hash() != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the expected trusted entry"));
        }

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }

        let trusted_set = pkg.build_trusted_subdag(entries)?;

        if self.ctx.config.enable_sanity_checks {
            let ref_proof = proof.clone();
            consensus.apply_pruning_proof(proof, &trusted_set);
            consensus.import_pruning_points(pruning_points);

            info!("Building the proof which was just applied (sanity test)");
            let built_proof = consensus.get_pruning_point_proof();
            for (i, (ref_level, built_level)) in ref_proof.iter().zip(built_proof.iter()).enumerate() {
                assert_eq!(
                    ref_level.iter().map(|h| h.hash).collect::<BlockHashSet>(),
                    built_level.iter().map(|h| h.hash).collect::<BlockHashSet>(),
                    "Locally built proof for level {} does not match the applied one",
                    i
                );
            }
            info!("Proof was locally built successfully");
        } else {
            consensus.apply_pruning_proof(proof, &trusted_set);
            consensus.import_pruning_points(pruning_points);
        }

        info!("Starting to process {} trusted blocks", trusted_set.len());
        let mut last_time = Instant::now();
        let mut last_index: usize = 0;
        for (i, tb) in trusted_set.into_iter().enumerate() {
            let now = Instant::now();
            let passed = now.duration_since(last_time);
            if passed > Duration::from_secs(1) {
                info!("Processed {} trusted blocks in the last {:.2}s (total {})", i - last_index, passed.as_secs_f64(), i);
                last_time = now;
                last_index = i;
            }
            // TODO: queue and join in batches
            consensus.validate_and_insert_trusted_block(tb).await?;
        }
        info!("Done processing trusted blocks");

        // TODO: make sure that the proof pruning point is not genesis

        Ok(proof_pruning_point)
    }

    async fn sync_headers(
        &mut self,
        consensus: &dyn ConsensusApi,
        syncer_header_selected_tip: Hash,
        highest_known_syncer_chain_hash: Hash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        let highest_shared_header_score = consensus.get_header(highest_known_syncer_chain_hash)?.daa_score;
        let mut progress_reporter = ProgressReporter::new(highest_shared_header_score, relay_block.header.daa_score, "block headers");

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

        if let Some(chunk) = chunk_stream.next().await? {
            let mut prev_daa_score = chunk.last().expect("chunk is never empty").daa_score;
            let mut prev_jobs: Vec<BlockValidationFuture> =
                chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h))).collect();

            while let Some(chunk) = chunk_stream.next().await? {
                let current_daa_score = chunk.last().expect("chunk is never empty").daa_score;
                let current_jobs = chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h))).collect();
                let prev_chunk_len = prev_jobs.len();
                // Join the previous chunk so that we always concurrently process a chunk and receive another
                try_join_all(prev_jobs).await?;
                // Log the progress
                progress_reporter.report(prev_chunk_len, prev_daa_score);
                prev_daa_score = current_daa_score;
                prev_jobs = current_jobs;
            }

            let prev_chunk_len = prev_jobs.len();
            try_join_all(prev_jobs).await?;
            progress_reporter.report_completion(prev_chunk_len);
        }

        self.sync_missing_relay_past_headers(consensus, syncer_header_selected_tip, relay_block.hash()).await?;

        Ok(())
    }

    async fn sync_missing_relay_past_headers(
        &mut self,
        consensus: &dyn ConsensusApi,
        syncer_header_selected_tip: Hash,
        relay_block_hash: Hash,
    ) -> Result<(), ProtocolError> {
        // Finished downloading syncer selected tip blocks,
        // check if we already have the triggering relay block
        if consensus.get_block_status(relay_block_hash).is_some() {
            return Ok(());
        }

        // Send a special header request for the selected tip anticone. This is expected to
        // be a small set, as it is bounded to the size of virtual's mergeset.
        self.router
            .enqueue(make_message!(
                Payload::RequestAnticone,
                RequestAnticoneMessage {
                    block_hash: Some(syncer_header_selected_tip.into()),
                    context_hash: Some(relay_block_hash.into())
                }
            ))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockHeaders)?;
        let chunk: HeadersChunk = msg.try_into()?;
        let jobs: Vec<BlockValidationFuture> =
            chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h))).collect();
        try_join_all(jobs).await?;
        dequeue_with_timeout!(self.incoming_route, Payload::DoneHeaders)?;

        if consensus.get_block_status(relay_block_hash).is_none() {
            // If the relay block has still not been received, the peer is misbehaving
            Err(ProtocolError::OtherOwned(format!(
                "did not receive relay block {} from peer {} during block download",
                relay_block_hash, self.router
            )))
        } else {
            Ok(())
        }
    }

    fn validate_staging_timestamps(
        &self,
        consensus: &dyn ConsensusApi,
        staging_consensus: &dyn ConsensusApi,
    ) -> Result<(), ProtocolError> {
        let staging_hst = staging_consensus.get_header(staging_consensus.get_headers_selected_tip()).unwrap();
        let current_hst = consensus.get_header(consensus.get_headers_selected_tip()).unwrap();
        // If staging is behind current or within 10 minutes ahead of it, then something is wrong and we reject the IBD
        if staging_hst.timestamp < current_hst.timestamp || staging_hst.timestamp - current_hst.timestamp < 600_000 {
            Err(ProtocolError::OtherOwned(format!(
                "The difference between the timestamp of the current selected tip ({}) and the 
staging selected tip ({}) is too small or negative. Aborting IBD...",
                current_hst.timestamp, staging_hst.timestamp
            )))
        } else {
            Ok(())
        }
    }

    async fn sync_pruning_point_utxoset(&mut self, consensus: &dyn ConsensusApi, pruning_point: Hash) -> Result<(), ProtocolError> {
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

    async fn sync_missing_block_bodies(&mut self, consensus: &dyn ConsensusApi, high: Hash) -> Result<(), ProtocolError> {
        // TODO: query consensus in batches
        let hashes = consensus.get_missing_block_body_hashes(high)?;
        if hashes.is_empty() {
            return Ok(());
        }

        let low_header = consensus.get_header(*hashes.first().expect("hashes was non empty"))?;
        let high_header = consensus.get_header(*hashes.last().expect("hashes was non empty"))?;
        let mut progress_reporter = ProgressReporter::new(low_header.daa_score, high_header.daa_score, "blocks");

        let mut iter = hashes.chunks(IBD_BATCH_SIZE);
        let (mut prev_jobs, mut prev_daa_score) =
            self.queue_block_processing_chunk(consensus, iter.next().expect("hashes was non empty")).await?;

        for chunk in iter {
            let (current_jobs, current_daa_score) = self.queue_block_processing_chunk(consensus, chunk).await?;
            let prev_chunk_len = prev_jobs.len();
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            try_join_all(prev_jobs).await?;
            // Log the progress
            progress_reporter.report(prev_chunk_len, prev_daa_score);
            prev_daa_score = current_daa_score;
            prev_jobs = current_jobs;
        }

        let prev_chunk_len = prev_jobs.len();
        try_join_all(prev_jobs).await?;
        progress_reporter.report_completion(prev_chunk_len);

        self.ctx.on_new_block_template().await?;

        Ok(())
    }

    async fn queue_block_processing_chunk(
        &mut self,
        consensus: &dyn ConsensusApi,
        chunk: &[Hash],
    ) -> Result<(Vec<BlockValidationFuture>, u64), ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
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
            current_daa_score = block.header.daa_score;
            jobs.push(consensus.validate_and_insert_block(block));
        }

        Ok((jobs, current_daa_score))
    }
}
