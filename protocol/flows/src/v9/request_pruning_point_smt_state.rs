use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    ibd::{SMT_CHUNK_SIZE, SMT_FLOW_CONTROL_WINDOW},
};
use kaspa_consensus_core::errors::consensus::ConsensusError;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{SmtLaneChunkMessage, SmtLaneEntry, SmtMetadataMessage, UnexpectedPruningPointMessage, kaspad_message::Payload},
};
use std::sync::Arc;

pub struct RequestPruningPointSmtStateFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointSmtStateFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointSmtStateFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let expected_pp = dequeue!(self.incoming_route, Payload::RequestPruningPointSmtState)?.try_into()?;
            self.handle_request(expected_pp).await?
        }
    }

    async fn handle_request(&mut self, expected_pp: Hash) -> Result<(), ProtocolError> {
        let consensus = self.ctx.consensus();
        let session = consensus.session().await;

        // Get metadata synchronously
        let metadata = match session.async_get_pruning_point_smt_metadata(expected_pp).await {
            Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point().await,
            res => res,
        }?;

        let expected_count = metadata.active_lanes_count;

        // Send metadata: lanes_root || payload_and_ctx_digest || parent_seq_commit (96 bytes)
        let mut md_bytes = Vec::with_capacity(96);
        md_bytes.extend_from_slice(&metadata.lanes_root.as_bytes());
        md_bytes.extend_from_slice(&metadata.payload_and_ctx_digest.as_bytes());
        md_bytes.extend_from_slice(&metadata.parent_seq_commit.as_bytes());
        self.router
            .enqueue(make_message!(Payload::SmtMetadata, SmtMetadataMessage { data: md_bytes, active_lanes_count: expected_count }))
            .await?;

        drop(session);

        if expected_count == 0 {
            debug!("Finished sending SMT state for pruning point {}: 0 lanes", expected_pp);
            return Ok(());
        }

        // Chunk the DB read with a cursor. Each `async_get_pruning_point_smt_lanes_chunk`
        // opens and releases its own RocksDB iterator / snapshot, so the pruning lock is
        // never held across the full IBD stream — only for one bounded chunk at a time.
        let mut cursor: Option<Hash> = None;
        let mut lanes_sent: u64 = 0;
        let mut chunks_sent: usize = 0;

        'outer: while lanes_sent < expected_count {
            let remaining = (expected_count - lanes_sent) as usize;
            let limit = remaining.min(SMT_CHUNK_SIZE);

            let lanes = match consensus
                .session()
                .await
                .async_get_pruning_point_smt_lanes_chunk(expected_pp, cursor, limit, lanes_sent)
                .await
            {
                Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point().await,
                res => res,
            }?;

            let [.., last] = lanes.as_slice() else {
                // DB exhausted before reaching expected_count — the peer's view of the
                // pruning point likely changed. Abort rather than send a truncated stream.
                return self.send_unexpected_pruning_point().await;
            };
            // Advance cursor to the last lane of this chunk before we move `lanes`.
            cursor = Some(last.lane_key);

            let entries: Vec<SmtLaneEntry> = lanes
                .into_iter()
                .map(|lane| {
                    let mut data = Vec::with_capacity(64);
                    data.extend_from_slice(&lane.lane_key.as_bytes());
                    data.extend_from_slice(&lane.lane_tip.as_bytes());
                    let proof_bytes = lane.proof.as_ref().map(|p| p.to_bytes()).unwrap_or_default();
                    SmtLaneEntry { data, blue_score: lane.blue_score, proof: proof_bytes }
                })
                .collect();

            let chunk_len = entries.len() as u64;
            self.router.enqueue(make_message!(Payload::SmtLaneChunk, SmtLaneChunkMessage { entries })).await?;

            lanes_sent += chunk_len;
            chunks_sent += 1;

            // Final chunk — receiver will not send a trailing RequestNext, so don't wait.
            if lanes_sent >= expected_count {
                break 'outer;
            }

            // Flow-control: after every window, wait for the peer's RequestNext. No
            // consensus session is held here, so peer latency has no DB impact.
            if chunks_sent.is_multiple_of(SMT_FLOW_CONTROL_WINDOW) {
                dequeue!(self.incoming_route, Payload::RequestNextPruningPointSmtChunk)?;
            }
        }

        // Sanity: confirm the DB has no lanes past `expected_count` — guards against
        // a silent overcount during iteration.
        let tail = match consensus.session().await.async_get_pruning_point_smt_lanes_chunk(expected_pp, cursor, 1, lanes_sent).await {
            Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point().await,
            res => res,
        }?;
        if !tail.is_empty() {
            return Err(ProtocolError::Other("SMT lane iteration yielded more entries than active_lanes_count"));
        }

        debug!("Finished sending SMT state for pruning point {}: {} lanes in {} chunks", expected_pp, lanes_sent, chunks_sent);
        Ok(())
    }

    async fn send_unexpected_pruning_point(&mut self) -> Result<(), ProtocolError> {
        self.router.enqueue(make_message!(Payload::UnexpectedPruningPoint, UnexpectedPruningPointMessage {})).await?;
        Ok(())
    }
}
