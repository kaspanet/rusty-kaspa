use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    ibd::{negotiate::ChainNegotiationOutput, HeadersChunkStream, TrustedEntryStream},
};
use futures::future::{join_all, select, try_join_all, Either};
use itertools::Itertools;
use kaspa_consensus_core::{
    api::BlockValidationFuture,
    block::Block,
    header::Header,
    pruning::{PruningPointProof, PruningPointsList, PruningProofMetadata},
    trusted::TrustedBlock,
    tx::Transaction,
    BlockHashSet,
};
use kaspa_consensusmanager::{spawn_blocking, ConsensusProxy, StagingConsensus};
use kaspa_core::{debug, info, time::unix_now, warn};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    convert::model::trusted::TrustedDataPackage,
    dequeue_with_timeout, make_message, make_request,
    pb::{
        kaspad_message::Payload, RequestAntipastMessage, RequestBlockBodiesMessage, RequestHeadersMessage, RequestIbdBlocksMessage,
        RequestPruningPointAndItsAnticoneMessage, RequestPruningPointProofMessage, RequestPruningPointUtxoSetMessage,
    },
    IncomingRoute, Router,
};
use kaspa_utils::channel::JobReceiver;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;

use super::{progress::ProgressReporter, HeadersChunk, PruningPointUtxosetChunkStream, IBD_BATCH_SIZE};
type BlockBody = Vec<Transaction>;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    pub(super) ctx: FlowContext,
    pub(super) router: Arc<Router>,
    pub(super) incoming_route: IncomingRoute,
    pub(super) body_only_ibd_permitted: bool,

    // Receives relay blocks from relay flow which are out of orphan resolution range and hence trigger IBD
    relay_receiver: JobReceiver<Block>,
}

#[async_trait::async_trait]
impl Flow for IbdFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

pub enum IbdType {
    Sync,
    DownloadHeadersProof,
    PruningCatchUp,
}

struct QueueChunkOutput {
    jobs: Vec<BlockValidationFuture>,
    daa_score: u64,
    timestamp: u64,
}
// TODO: define a peer banning strategy

impl IbdFlow {
    pub fn new(
        ctx: FlowContext,
        router: Arc<Router>,
        incoming_route: IncomingRoute,
        relay_receiver: JobReceiver<Block>,
        body_only_ibd_permitted: bool,
    ) -> Self {
        Self { ctx, router, incoming_route, relay_receiver, body_only_ibd_permitted }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        while let Ok(relay_block) = self.relay_receiver.recv().await {
            if let Some(_guard) = self.ctx.try_set_ibd_running(self.router.key(), relay_block.header.daa_score) {
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
        let mut session = self.ctx.consensus().session().await;

        let negotiation_output = self.negotiate_missing_syncer_chain_segment(&session).await?;
        let ibd_type = self
            .determine_ibd_type(
                &session,
                &relay_block.header,
                negotiation_output.highest_known_syncer_chain_hash,
                negotiation_output.syncer_pruning_point,
            )
            .await?;
        match ibd_type {
            IbdType::Sync => {
                let pruning_point = session.async_pruning_point().await;

                info!("syncing ahead from current pruning point");
                // Following IBD catchup a new pruning point is designated and finalized in consensus. Blocks from its anticone (including itself)
                // have undergone normal header verification, but contain no body yet. Processing of new blocks in the pruning point's future cannot proceed
                // since these blocks' parents are missing block data.
                // Hence we explicitly process bodies of the currently body missing anticone blocks as trusted blocks
                // Notice that this is degenerate following sync_with_headers_proof
                // but not necessarily so after sync_headers -
                // as it might sync following a previous pruning_catch_up that crashed before this stage concluded
                if !session.async_is_pruning_point_anticone_fully_synced().await {
                    self.sync_missing_trusted_bodies(&session).await?;
                }
                if !session.async_is_pruning_utxoset_stable().await
                // Utxo might not be available even if the pruning point block data is.
                // Utxo must be synced before all so the node could function
                {
                    info!(
                        "utxoset corresponding to the current pruning point is incomplete, attempting to download it from {}",
                        self.router
                    );

                    self.sync_new_utxo_set(&session, pruning_point).await?;
                }
                // Once utxo is valid, simply sync missing headers
                self.sync_headers(
                    &session,
                    negotiation_output.syncer_virtual_selected_parent,
                    negotiation_output.highest_known_syncer_chain_hash.unwrap(),
                    &relay_block,
                )
                .await?;
            }
            IbdType::DownloadHeadersProof => {
                drop(session); // Avoid holding the previous consensus throughout the staging IBD
                let staging = self.ctx.consensus_manager.new_staging_consensus();
                match self.ibd_with_headers_proof(&staging, negotiation_output.syncer_virtual_selected_parent, &relay_block).await {
                    Ok(()) => {
                        spawn_blocking(|| staging.commit()).await.unwrap();
                        info!(
                            "Header download stage of IBD with headers proof completed successfully from {}. Committed staging consensus.",
                                    self.router
                                );

                        // This will reobtain the freshly committed staging consensus
                        session = self.ctx.consensus().session().await;
                        // Next, sync a utxoset corresponding to the new pruning point from the syncer.
                        // Note that the new pruning point's anticone need not be downloaded separately as in other IBD types
                        // as it was just downloaded as part of the headers proof.
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;
                    }
                    Err(e) => {
                        warn!("IBD with headers proof from {} was unsuccessful ({})", self.router, e);
                        staging.cancel();
                        return Err(e);
                    }
                }
            }
            IbdType::PruningCatchUp => {
                info!("catching up to new pruning point {} ", negotiation_output.syncer_pruning_point);
                match self.pruning_point_catchup(&session, &negotiation_output, &relay_block).await {
                    Ok(()) => {
                        info!("header stage of pruning catchup from peer {} completed", self.router);
                        self.sync_missing_trusted_bodies(&session).await?;
                        self.sync_new_utxo_set(&session, negotiation_output.syncer_pruning_point).await?;
                        // Note that pruning of old data will only occur once virtual has caught up sufficiently far
                    }

                    Err(e) => {
                        warn!("IBD catchup from peer {} was unsuccessful ({})", self.router, e);
                        return Err(e);
                    }
                }
            }
        }

        // Sync missing bodies in the past of syncer sink (virtual selected parent)
        self.sync_missing_block_bodies(&session, negotiation_output.syncer_virtual_selected_parent).await?;

        // Relay block might be in the antipast of syncer sink, thus
        // check its past for missing bodies as well.
        self.sync_missing_block_bodies(&session, relay_block.hash()).await?;

        // Following IBD we revalidate orphans since many of them might have been processed during the IBD
        // or are now processable
        let (queued_hashes, virtual_processing_tasks) = self.ctx.revalidate_orphans(&session).await;
        let mut unorphaned_hashes = Vec::with_capacity(queued_hashes.len());
        let results = join_all(virtual_processing_tasks).await;
        for (hash, result) in queued_hashes.into_iter().zip(results) {
            match result {
                Ok(_) => unorphaned_hashes.push(hash),
                // We do not return the error and disconnect here since we don't know
                // that this peer was the origin of the orphan block
                Err(e) => warn!("Validation failed for orphan block {}: {}", hash, e),
            }
        }
        match unorphaned_hashes.len() {
            0 => {}
            n => info!("IBD post processing: unorphaned {} blocks ...{}", n, unorphaned_hashes.last().unwrap()),
        }

        Ok(())
    }

    async fn determine_ibd_type(
        &self,
        consensus: &ConsensusProxy,
        relay_header: &Header,
        highest_known_syncer_chain_hash: Option<Hash>,
        syncer_pruning_point: Hash,
    ) -> Result<IbdType, ProtocolError> {
        if let Some(highest_known_syncer_chain_hash) = highest_known_syncer_chain_hash {
            let pruning_point = consensus.async_pruning_point().await;
            let sink = consensus.async_get_sink().await;
            info!("current sink is:{}", sink);
            info!("current pruning point is:{}", pruning_point);
            if consensus.async_is_chain_ancestor_of(pruning_point, highest_known_syncer_chain_hash).await? {
                if syncer_pruning_point == pruning_point {
                    // The node is only missing a segment in the future of its current pruning point, and the chains
                    // agree as well, so we perform a simple sync IBD and only download the missing data
                    return Ok(IbdType::Sync);
                } else {
                    consensus.async_verify_is_pruning_sample(syncer_pruning_point).await?;
                    // The node is missing a segment in the near future of its current pruning point, but the syncer is ahead
                    // and already pruned the current pruning point.

                    if consensus.async_get_block_status(syncer_pruning_point).await.is_some_and(|b| b.has_block_body())
                        && !consensus.async_is_consensus_in_transitional_ibd_state().await
                    {
                        // The data pruned by the syncer is already available from within the node (from relay or past ibd attempts)
                        // and the consensus is not in a transitional state requiring data on the previous pruning point,
                        // hence we can carry on syncing as normal.
                        return Ok(IbdType::Sync);
                    } else {
                        // Two options:
                        // 1: syncer_pruning_point is in the future, and there is a need to partially resync from syncer_pruning_point
                        // 2: syncer_pruning_point is in the past of current pruning point, or is unknown on which case the syncing node is flawed,
                        // and IBD should be stopped

                        if consensus
                            .async_is_chain_ancestor_of(pruning_point, syncer_pruning_point)
                            .await
                            .map_err(|_| ProtocolError::Other("syncer pruning point is corrupted"))?
                        {
                            return Ok(IbdType::PruningCatchUp);
                        } else {
                            return Err(ProtocolError::Other("syncer pruning point is outdated"));
                        }
                    }
                }
            }

            // If the pruning point is not in the chain of `highest_known_syncer_chain_hash`, it
            // means it's in its antichain (because if `highest_known_syncer_chain_hash` was in
            // the pruning point's past the pruning point itself would be
            // `highest_known_syncer_chain_hash`). So it means there's a finality conflict.
            //
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition
            // to disconnecting from the peer (e.g., banning, rpc notification)
            return Err(ProtocolError::Other("peer is in a finality conflict with the local pruning point"));
        }

        let hst_header = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
        let pruning_depth = self.ctx.config.pruning_depth();
        if relay_header.blue_score >= hst_header.blue_score + pruning_depth && relay_header.blue_work > hst_header.blue_work {
            let finality_duration_in_milliseconds = self.ctx.config.finality_duration_in_milliseconds();
            if unix_now() > consensus.async_creation_timestamp().await + finality_duration_in_milliseconds {
                let fp = consensus.async_finality_point().await;
                let fp_ts = consensus.async_get_header(fp).await?.timestamp;
                if unix_now() < fp_ts + finality_duration_in_milliseconds * 3 / 2 {
                    // We reject the headers proof if the node has a relatively up-to-date finality point and current
                    // consensus has matured for long enough (and not recently synced). This is mostly a spam-protector
                    // since subsequent checks identify these violations as well
                    // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
                    return Err(ProtocolError::Other(
                        "peer has no known block but local consensus appears to be up to date, this is most likely a spam attempt",
                    ));
                }
            }

            // The relayed block has sufficient blue score and blue work over the current header selected tip
            Ok(IbdType::DownloadHeadersProof)
        } else {
            Err(ProtocolError::Other("peer has no known block but conditions for requesting headers proof are not met"))
        }
    }

    /// This function is triggered when the syncer's pruning point is higher
    /// than ours and we already processed its header before.
    /// so we only need to sync more headers and set it to our new pruning point before proceeding with IBD
    async fn pruning_point_catchup(
        &mut self,
        consensus: &ConsensusProxy,
        negotiation_output: &ChainNegotiationOutput,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        // Before attempting to update to the syncers pruning point, sync to the latest headers of the syncer,
        // to ensure that  we will locally have sufficient headers on top of  the syncer's pruning point
        let syncer_pp = negotiation_output.syncer_pruning_point;
        let syncer_sink = negotiation_output.syncer_virtual_selected_parent;
        self.sync_headers(consensus, syncer_sink, negotiation_output.highest_known_syncer_chain_hash.unwrap(), relay_block).await?;

        // This function's main effect is to confirm the syncer's pruning point can be finalized into the consensus, and to update
        // all the relevant stores
        consensus.async_intrusive_pruning_point_update(syncer_pp, syncer_sink).await?;

        // A sanity check to confirm that following the intrusive addition of new pruning points,
        // the latest pruning point still correctly agrees with the DAG data,
        // and is the head of a pruning points "chain" leading all the way down to genesis
        // TODO(relaxed): once the catchup functionality has sufficiently matured, consider only doing this test if sanity checks are enabled
        info!("validating pruning points consistency");
        consensus.async_validate_pruning_points(syncer_sink).await.unwrap();
        info!("pruning points consistency validated");
        Ok(())
    }

    async fn ibd_with_headers_proof(
        &mut self,
        staging: &StagingConsensus,
        syncer_virtual_selected_parent: Hash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        info!("Starting IBD with headers proof with peer {}", self.router);

        let staging_session = staging.session().await;

        let pruning_point = self.sync_and_validate_pruning_proof(&staging_session, relay_block).await?;
        self.sync_headers(&staging_session, syncer_virtual_selected_parent, pruning_point, relay_block).await?;
        staging_session.async_validate_pruning_points(syncer_virtual_selected_parent).await?;
        self.validate_staging_timestamps(&self.ctx.consensus().session().await, &staging_session).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, staging: &ConsensusProxy, relay_block: &Block) -> Result<Hash, ProtocolError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = msg.try_into()?;
        info!(
            "Received headers proof with overall {} headers ({} unique)",
            proof.iter().map(|l| l.len()).sum::<usize>(),
            proof.iter().flatten().unique_by(|h| h.hash).count()
        );

        let proof_metadata = PruningProofMetadata::new(relay_block.header.blue_work);

        // Get a new session for current consensus (non staging)
        let consensus = self.ctx.consensus().session().await;

        // The proof is validated in the context of current consensus
        let proof =
            consensus.clone().spawn_blocking(move |c| c.validate_pruning_proof(&proof, &proof_metadata).map(|()| proof)).await?;

        let proof_pruning_point = proof[0].last().expect("was just ensured by validation").hash;

        if proof_pruning_point == self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the proof pruning point is the genesis block"));
        }

        if proof_pruning_point == consensus.async_pruning_point().await {
            return Err(ProtocolError::Other("the proof pruning point is the same as the current pruning point"));
        }
        drop(consensus);

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;
        // First, all pruning points up to the last  are sent
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let pruning_points: PruningPointsList = msg.try_into()?;

        if pruning_points.is_empty() || pruning_points.last().unwrap().hash != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the last pruning point in the list"));
        }

        if pruning_points.first().unwrap().hash != self.ctx.config.genesis.hash {
            return Err(ProtocolError::Other("the first pruning point in the list is expected to be genesis"));
        }

        // Check if past pruning points violate finality of current consensus
        if self.ctx.consensus().session().await.async_are_pruning_points_violating_finality(pruning_points.clone()).await {
            // TODO (relaxed): consider performing additional actions on finality conflicts in addition to disconnecting from the peer (e.g., banning, rpc notification)
            return Err(ProtocolError::Other("pruning points are violating finality"));
        }

        // Trusted data is sent in two stages:
        // The first, TrustedDataPackage, contains meta data about daa_window
        // blocks headers, and ghostdag data, which are required to verify the pruning
        // point and its anticone.
        // The latter, the trusted data entries, each represent a block (with daa) from the anticone of the pruning point
        // (including the PP itself), alongside indexing denoting the respective metadata headers or ghostdag data
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = msg.try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route);
        // The first entry of the trusted data is the pruning point itself.
        let Some(pruning_point_entry) = entry_stream.next().await? else {
            return Err(ProtocolError::Other("got `done` message before receiving the pruning point"));
        };

        if pruning_point_entry.block.hash() != proof_pruning_point {
            return Err(ProtocolError::Other("the proof pruning point is not equal to the expected trusted entry"));
        }

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }
        // Create a topologically ordered vector of  trusted blocks - the pruning point and its anticone,
        // and their daa windows headers
        let mut trusted_set = pkg.build_trusted_subdag(entries)?;

        if self.ctx.config.enable_sanity_checks {
            let con = self.ctx.consensus().unguarded_session_blocking();
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    let ref_proof = proof.clone();
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;

                    info!("Building the proof which was just applied (sanity test)");
                    let built_proof = c.get_pruning_point_proof();
                    let mut mismatch_detected = false;
                    for (i, (ref_level, built_level)) in ref_proof.iter().zip(built_proof.iter()).enumerate() {
                        if ref_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                            != built_level.iter().map(|h| h.hash).collect::<BlockHashSet>()
                        {
                            mismatch_detected = true;
                            warn!("Locally built proof for level {} does not match the applied one", i);
                        }
                    }
                    if mismatch_detected {
                        info!("Validating the locally built proof (sanity test fallback #2)");
                        // Note: the proof is validated in the context of *current* consensus
                        if let Err(err) = con.validate_pruning_proof(&built_proof, &proof_metadata) {
                            panic!("Locally built proof failed validation: {}", err);
                        }
                        info!("Locally built proof was validated successfully");
                    } else {
                        info!("Proof was locally built successfully");
                    }
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        } else {
            trusted_set = staging
                .clone()
                .spawn_blocking(move |c| {
                    c.apply_pruning_proof(proof, &trusted_set)?;
                    c.import_pruning_points(pruning_points)?;
                    Result::<_, ProtocolError>::Ok(trusted_set)
                })
                .await?;
        }

        // TODO (relaxed): add logs to staging commit process

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
            // TODO (relaxed): queue and join in batches
            staging.validate_and_insert_trusted_block(tb).virtual_state_task.await?;
        }
        staging.async_clear_body_missing_anticone_set().await;
        info!("Done processing trusted blocks");
        Ok(proof_pruning_point)
    }

    async fn sync_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: Hash,
        highest_known_syncer_chain_hash: Hash,
        relay_block: &Block,
    ) -> Result<(), ProtocolError> {
        let highest_shared_header_score = consensus.async_get_header(highest_known_syncer_chain_hash).await?.daa_score;
        let mut progress_reporter = ProgressReporter::new(highest_shared_header_score, relay_block.header.daa_score, "block headers");

        self.router
            .enqueue(make_message!(
                Payload::RequestHeaders,
                RequestHeadersMessage {
                    low_hash: Some(highest_known_syncer_chain_hash.into()),
                    high_hash: Some(syncer_virtual_selected_parent.into())
                }
            ))
            .await?;
        let mut chunk_stream = HeadersChunkStream::new(&self.router, &mut self.incoming_route);

        if let Some(chunk) = chunk_stream.next().await? {
            let (mut prev_daa_score, mut prev_timestamp) = {
                let last_header = chunk.last().expect("chunk is never empty");
                (last_header.daa_score, last_header.timestamp)
            };
            let mut prev_jobs: Vec<BlockValidationFuture> =
                chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();

            while let Some(chunk) = chunk_stream.next().await? {
                let (current_daa_score, current_timestamp) = {
                    let last_header = chunk.last().expect("chunk is never empty");
                    (last_header.daa_score, last_header.timestamp)
                };
                let current_jobs = chunk
                    .into_iter()
                    .map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task)
                    .collect();
                let prev_chunk_len = prev_jobs.len();
                // Join the previous chunk so that we always concurrently process a chunk and receive another
                try_join_all(prev_jobs).await?;
                // Log the progress
                progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
                prev_daa_score = current_daa_score;
                prev_timestamp = current_timestamp;
                prev_jobs = current_jobs;
            }

            let prev_chunk_len = prev_jobs.len();
            try_join_all(prev_jobs).await?;
            progress_reporter.report_completion(prev_chunk_len);
        }

        if consensus.async_get_block_status(syncer_virtual_selected_parent).await.is_none() {
            // If the syncer's claimed sink header has still not been received, the peer is misbehaving
            return Err(ProtocolError::OtherOwned(format!(
                "did not receive syncer's virtual selected parent {} from peer {} during header download",
                syncer_virtual_selected_parent, self.router
            )));
        }

        self.sync_missing_relay_past_headers(consensus, syncer_virtual_selected_parent, relay_block.hash()).await?;

        Ok(())
    }

    async fn sync_new_utxo_set(&mut self, consensus: &ConsensusProxy, pruning_point: Hash) -> Result<(), ProtocolError> {
        // A  better solution could be to create a copy of the old utxo state for some sort of fallback rather than delete it.
        consensus.async_clear_pruning_utxo_set().await; // this deletes the old pruning utxoset and also sets the pruning utxo as invalidated
        self.sync_pruning_point_utxoset(consensus, pruning_point).await?;
        consensus.async_set_pruning_utxoset_stable().await; //  only if the function has reached here, will the utxo be considered "final"
        self.ctx.on_pruning_point_utxoset_override();
        Ok(())
    }

    async fn sync_missing_relay_past_headers(
        &mut self,
        consensus: &ConsensusProxy,
        syncer_virtual_selected_parent: Hash,
        relay_block_hash: Hash,
    ) -> Result<(), ProtocolError> {
        // Finished downloading syncer selected tip blocks,
        // check if we already have the triggering relay block
        if consensus.async_get_block_status(relay_block_hash).await.is_some() {
            return Ok(());
        }

        // Send a special header request for the sink antipast. This is expected to
        // be a relatively small set since virtual and relay blocks should be close topologically.
        // See server-side handling of `RequestAnticone` for further details.
        self.router
            .enqueue(make_message!(
                Payload::RequestAntipast,
                RequestAntipastMessage {
                    block_hash: Some(syncer_virtual_selected_parent.into()),
                    context_hash: Some(relay_block_hash.into())
                }
            ))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockHeaders)?;
        let chunk: HeadersChunk = msg.try_into()?;
        let jobs: Vec<BlockValidationFuture> =
            chunk.into_iter().map(|h| consensus.validate_and_insert_block(Block::from_header_arc(h)).virtual_state_task).collect();
        try_join_all(jobs).await?;
        dequeue_with_timeout!(self.incoming_route, Payload::DoneHeaders)?;

        if consensus.async_get_block_status(relay_block_hash).await.is_none() {
            // If the relay block has still not been received, the peer is misbehaving
            Err(ProtocolError::OtherOwned(format!(
                "did not receive relay block {} from peer {} during header download",
                relay_block_hash, self.router
            )))
        } else {
            Ok(())
        }
    }

    async fn validate_staging_timestamps(
        &self,
        consensus: &ConsensusProxy,
        staging_consensus: &ConsensusProxy,
    ) -> Result<(), ProtocolError> {
        // The purpose of this check is to prevent the potential abuse explained here:
        // https://github.com/kaspanet/research/issues/3#issuecomment-895243792
        let staging_hst = staging_consensus.async_get_header(staging_consensus.async_get_headers_selected_tip().await).await.unwrap();
        let current_hst = consensus.async_get_header(consensus.async_get_headers_selected_tip().await).await.unwrap();
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

    async fn sync_pruning_point_utxoset(&mut self, consensus: &ConsensusProxy, pruning_point: Hash) -> Result<(), ProtocolError> {
        info!("downloading the pruning point utxoset, this can take a little while.");
        self.router
            .enqueue(make_message!(
                Payload::RequestPruningPointUtxoSet,
                RequestPruningPointUtxoSetMessage { pruning_point_hash: Some(pruning_point.into()) }
            ))
            .await?;
        let mut chunk_stream = PruningPointUtxosetChunkStream::new(&self.router, &mut self.incoming_route);
        let mut multiset = MuHash::new();
        while let Some(chunk) = chunk_stream.next().await? {
            multiset = consensus
                .clone()
                .spawn_blocking(move |c| {
                    c.append_imported_pruning_point_utxos(&chunk, &mut multiset);
                    multiset
                })
                .await;
        }
        consensus.clone().spawn_blocking(move |c| c.import_pruning_point_utxo_set(pruning_point, multiset)).await?;
        Ok(())
    }
    async fn sync_missing_trusted_bodies(&mut self, consensus: &ConsensusProxy) -> Result<(), ProtocolError> {
        info!("downloading pruning point anticone missing block data");
        let diesembodied_hashes = consensus.async_get_body_missing_anticone().await;
        if self.body_only_ibd_permitted {
            self.sync_missing_trusted_bodies_no_headers(consensus, diesembodied_hashes).await?
        } else {
            self.sync_missing_trusted_bodies_full_blocks(consensus, diesembodied_hashes).await?;
        }
        consensus.async_clear_body_missing_anticone_set().await;
        Ok(())
    }
    async fn sync_missing_trusted_bodies_no_headers(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<Hash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestBlockBodies,
                    RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
                let blk_body: BlockBody = msg.try_into()?;
                // TODO (relaxed): make header queries in a batch.
                let blk_header = consensus.async_get_header(hash).await.map_err(|err| {
                    // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                    // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                    ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", hash, err))
                })?;
                if blk_body.is_empty() {
                    return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", hash)));
                }
                let block = Block { header: blk_header, transactions: blk_body.into() };
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_trusted_bodies_full_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        diesembodied_hashes: Vec<Hash>,
    ) -> Result<(), ProtocolError> {
        let iter = diesembodied_hashes.chunks(IBD_BATCH_SIZE);
        for chunk in iter {
            self.router
                .enqueue(make_message!(
                    Payload::RequestIbdBlocks,
                    RequestIbdBlocksMessage { hashes: chunk.iter().map(|h| h.into()).collect() }
                ))
                .await?;
            let mut jobs = Vec::with_capacity(chunk.len());

            for &hash in chunk.iter() {
                // TODO: change to BodyOnly requests when incorporated
                let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdBlock)?;
                let block: Block = msg.try_into()?;
                if block.hash() != hash {
                    return Err(ProtocolError::OtherOwned(format!("expected block {} but got {}", hash, block.hash())));
                }
                if block.is_header_only() {
                    return Err(ProtocolError::OtherOwned(format!("sent header of {} where expected block with body", block.hash())));
                }
                // TODO (relaxed): sending ghostdag data may be redundant, especially when the headers were already verified.
                // Consider sending empty ghostdag data, simplifying a great deal. The result should be the same -
                // a trusted task is sent, however the header is already verified, and hence only the block body will be verified.
                jobs.push(
                    consensus
                        .validate_and_insert_trusted_block(TrustedBlock::new(block, consensus.async_get_ghostdag_data(hash).await?))
                        .virtual_state_task,
                );
            }
            try_join_all(jobs).await?; // TODO (relaxed): be more efficient with batching as done with block bodies in general
        }
        Ok(())
    }
    async fn sync_missing_block_bodies(&mut self, consensus: &ConsensusProxy, high: Hash) -> Result<(), ProtocolError> {
        // TODO (relaxed): query consensus in batches
        let sleep_task = sleep(Duration::from_secs(2));
        let hashes_task = consensus.async_get_missing_block_body_hashes(high);
        tokio::pin!(sleep_task);
        tokio::pin!(hashes_task);
        let hashes = match select(sleep_task, hashes_task).await {
            Either::Left((_, hashes_task)) => {
                // We select between the tasks in order to inform the user if this operation is taking too long. On full IBD
                // this operation requires traversing the full DAG which indeed might take several seconds or even minutes.
                info!(
                    "IBD: searching for missing block bodies to request from peer {}. This operation might take several seconds.",
                    self.router
                );
                // Now re-await the original task
                hashes_task.await
            }
            Either::Right((hashes_result, _)) => hashes_result,
        }?;
        if hashes.is_empty() {
            return Ok(());
        }

        let low_header = consensus.async_get_header(*hashes.first().expect("hashes was non empty")).await?;
        let high_header = consensus.async_get_header(*hashes.last().expect("hashes was non empty")).await?;
        let mut progress_reporter = ProgressReporter::new(low_header.daa_score, high_header.daa_score, "blocks");

        let mut iter = hashes.chunks(IBD_BATCH_SIZE);
        let QueueChunkOutput { jobs: mut prev_jobs, daa_score: mut prev_daa_score, timestamp: mut prev_timestamp } =
            self.queue_block_processing_chunk(consensus, iter.next().expect("hashes was non empty")).await?;

        for chunk in iter {
            let QueueChunkOutput { jobs: current_jobs, daa_score: current_daa_score, timestamp: current_timestamp } =
                self.queue_block_processing_chunk(consensus, chunk).await?;
            let prev_chunk_len = prev_jobs.len();
            // Join the previous chunk so that we always concurrently process a chunk and receive another
            try_join_all(prev_jobs).await?;
            // Log the progress
            progress_reporter.report(prev_chunk_len, prev_daa_score, prev_timestamp);
            prev_daa_score = current_daa_score;
            prev_timestamp = current_timestamp;
            prev_jobs = current_jobs;
        }

        let prev_chunk_len = prev_jobs.len();
        try_join_all(prev_jobs).await?;
        progress_reporter.report_completion(prev_chunk_len);

        Ok(())
    }

    async fn queue_block_processing_chunk(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[Hash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        if self.body_only_ibd_permitted {
            self.queue_block_processing_chunk_body_only(consensus, chunk).await
        } else {
            self.queue_block_processing_chunk_full_block(consensus, chunk).await
        }
    }

    async fn queue_block_processing_chunk_full_block(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[Hash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
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
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }

    async fn queue_block_processing_chunk_body_only(
        &mut self,
        consensus: &ConsensusProxy,
        chunk: &[Hash],
    ) -> Result<QueueChunkOutput, ProtocolError> {
        let mut jobs = Vec::with_capacity(chunk.len());
        let mut current_daa_score = 0;
        let mut current_timestamp = 0;
        self.router
            .enqueue(make_request!(
                Payload::RequestBlockBodies,
                RequestBlockBodiesMessage { hashes: chunk.iter().map(|h| h.into()).collect() },
                self.incoming_route.id()
            ))
            .await?;
        for &expected_hash in chunk {
            let msg = dequeue_with_timeout!(self.incoming_route, Payload::BlockBody)?;
            // TODO (relaxed): make header queries in a batch.
            let blk_header = consensus.async_get_header(expected_hash).await.map_err(|err| {
                // Conceptually this indicates local inconsistency, since we received the expected hashes via a local
                // get_missing_block_body_hashes call. However for now we fail gracefully and only disconnect from this peer.
                ProtocolError::OtherOwned(format!("syncee inconsistency: missing block header for {}, err: {}", expected_hash, err))
            })?;
            let blk_body: BlockBody = msg.try_into()?;
            if blk_body.is_empty() {
                return Err(ProtocolError::OtherOwned(format!("sent empty block body for block {}", expected_hash)));
            }
            let block = Block { header: blk_header, transactions: blk_body.into() };
            current_daa_score = block.header.daa_score;
            current_timestamp = block.header.timestamp;
            jobs.push(consensus.validate_and_insert_block(block).virtual_state_task);
        }
        Ok(QueueChunkOutput { jobs, daa_score: current_daa_score, timestamp: current_timestamp })
    }
}
