use crate::{
    ctx::FlowContext,
    v5::ibd::{HeadersChunkStream, TrustedEntryStream},
};
use consensus_core::{
    api::DynConsensus,
    block::Block,
    blockstatus::BlockStatus,
    errors::block::{BlockProcessResult, RuleError},
    pruning::{PruningPointProof, PruningPointsList},
};
use futures::future::{join_all, BoxFuture};
use hashes::Hash;
use kaspa_core::{debug, info};
use p2p_lib::{
    common::FlowError,
    convert::model::trusted::TrustedDataPackage,
    dequeue_with_timeout, make_message,
    pb::{
        kaspad_message::Payload, RequestHeadersMessage, RequestIbdChainBlockLocatorMessage, RequestPruningPointAndItsAnticoneMessage,
        RequestPruningPointProofMessage,
    },
    IncomingRoute, Router,
};
use std::{sync::Arc, time::Duration};

use super::HeadersChunk;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    incoming_route: IncomingRoute,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
        // TEMP
        // TODO: start flows only after ready flow runs
        tokio::time::sleep(Duration::from_secs(1)).await;

        // None hashes indicate that the full chain is queried.
        let block_locator = self.get_syncer_chain_block_locator(None, None).await?;
        if block_locator.is_empty() {
            return Err(FlowError::ProtocolError("Expecting initial syncer chain block locator to contain at least one element"));
        }
        let syncer_header_selected_tip = *block_locator.first().expect("verified locator is not empty");
        self.start_ibd_with_headers_proof(syncer_header_selected_tip).await?;
        Ok(())
    }

    async fn get_syncer_chain_block_locator(
        &mut self,
        low_hash: Option<Hash>,
        high_hash: Option<Hash>,
    ) -> Result<Vec<Hash>, FlowError> {
        // TODO: use low and high hashes when zooming in
        self.router
            .enqueue(make_message!(
                Payload::RequestIbdChainBlockLocator,
                RequestIbdChainBlockLocatorMessage { low_hash: low_hash.map(|h| h.into()), high_hash: high_hash.map(|h| h.into()) }
            ))
            .await?;
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::IbdChainBlockLocator)?;
        if msg.block_locator_hashes.len() > 64 {
            return Err(FlowError::ProtocolError(
                "Got block locator of size > 64 while expecting
 locator to have size which is logarithmic in DAG size (which should never exceed 2^64)",
            ));
        }
        Ok(msg.try_into()?)
    }

    async fn start_ibd_with_headers_proof(&mut self, syncer_header_selected_tip: Hash) -> Result<(), FlowError> {
        info!("Starting IBD with headers proof");
        let consensus = self.ctx.consensus();
        let pruning_point = self.sync_and_validate_pruning_proof(&consensus).await?;
        self.sync_pruning_point_future_headers(&consensus, syncer_header_selected_tip, pruning_point).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, consensus: &DynConsensus) -> Result<Hash, FlowError> {
        self.router.enqueue(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;

        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        let proof: PruningPointProof = msg.try_into()?;
        debug!("received proof with overall {} headers", proof.iter().map(|l| l.len()).sum::<usize>());

        // TODO: call validate_pruning_proof when implemented
        // consensus.clone().validate_pruning_proof(&proof);

        let proof_pruning_point = proof[0].last().expect("was just insured by validation").hash;

        // TODO: verify the proof pruning point is different than current consensus pruning point

        self.router
            .enqueue(make_message!(Payload::RequestPruningPointAndItsAnticone, RequestPruningPointAndItsAnticoneMessage {}))
            .await?;

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPoints)?;
        let pruning_points: PruningPointsList = msg.try_into()?;

        // TODO: verify last pruning point header hashes to proof_pruning_point
        // TODO: import pruning points into consensus

        let msg = dequeue_with_timeout!(self.incoming_route, Payload::TrustedData)?;
        let pkg: TrustedDataPackage = msg.try_into()?;
        debug!("received trusted data with {} daa entries and {} ghostdag entries", pkg.daa_window.len(), pkg.ghostdag_window.len());

        let mut entry_stream = TrustedEntryStream::new(&self.router, &mut self.incoming_route);
        let Some(pruning_point_entry) = entry_stream.next().await? else { return Err(FlowError::ProtocolError("got `done` message before receiving the pruning point")); };

        // TODO: verify trusted pruning point matches proof pruning point

        let mut entries = vec![pruning_point_entry];
        while let Some(entry) = entry_stream.next().await? {
            entries.push(entry);
        }

        let trusted_set = pkg.build_trusted_set(entries)?;
        consensus.clone().apply_pruning_proof(proof, &trusted_set);
        consensus.clone().import_pruning_points(pruning_points);

        let mut last_time = std::time::SystemTime::now();
        let mut last_index: usize = 0;
        for (i, tb) in trusted_set.into_iter().enumerate() {
            let now = std::time::SystemTime::now();
            let passed = now.duration_since(last_time).unwrap();
            if passed > Duration::new(1, 0) {
                info!("Processed {} trusted blocks in the last {} seconds (total {})", i - last_index, passed.as_secs(), i);
                last_time = now;
                last_index = i;
            }
            consensus.clone().validate_and_insert_trusted_block(tb).await?;
        }
        info!("Done processing trusted blocks");

        Ok(proof_pruning_point)
    }

    async fn sync_pruning_point_future_headers(
        &mut self,
        consensus: &DynConsensus,
        syncer_header_selected_tip: Hash,
        highest_known_syncer_chain_hash: Hash,
    ) -> Result<(), FlowError> {
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
        let mut prev_joins = submit_chunk(consensus, chunk);

        while let Some(chunk) = chunk_stream.next().await? {
            let current_joins = submit_chunk(consensus, chunk);
            let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>()?;
            assert!(statuses.iter().all(|s| s.is_valid())); // TODO
            prev_joins = current_joins;
        }

        let statuses = join_all(prev_joins).await.into_iter().collect::<Result<Vec<BlockStatus>, RuleError>>()?;
        assert!(statuses.iter().all(|s| s.is_valid())); // TODO

        Ok(())
    }
}

fn submit_chunk(consensus: &DynConsensus, chunk: HeadersChunk) -> Vec<BoxFuture<'static, BlockProcessResult<BlockStatus>>> {
    let mut futures = Vec::new();
    for header in chunk {
        let f = consensus.clone().validate_and_insert_block(Block::from_header_arc(header), false);
        futures.push(f);
    }
    futures
}
