use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_consensus_core::api::ImportLane;
use kaspa_consensus_core::errors::consensus::ConsensusError;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, make_message,
    pb::{DoneSmtChunksMessage, SmtLaneEntryMessage, SmtMetadataMessage, UnexpectedPruningPointMessage, kaspad_message::Payload},
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

        // Send metadata: lanes_root || payload_and_ctx_digest || parent_seq_commit (96 bytes)
        let mut md_bytes = Vec::with_capacity(96);
        md_bytes.extend_from_slice(&metadata.lanes_root.as_bytes());
        md_bytes.extend_from_slice(&metadata.payload_and_ctx_digest.as_bytes());
        md_bytes.extend_from_slice(&metadata.parent_seq_commit.as_bytes());
        self.router
            .enqueue(make_message!(
                Payload::SmtMetadata,
                SmtMetadataMessage { data: md_bytes, active_lanes_count: metadata.active_lanes_count }
            ))
            .await?;

        // Stream lanes: blocking DB iteration pushes into channel, async loop sends to peer
        let (tx, mut rx) = tokio::sync::mpsc::channel::<ImportLane>(64);
        let session_clone = session.clone();
        tokio::task::spawn_blocking(move || {
            session_clone.blocking_iter_smt_lanes(expected_pp, move |lane| tx.blocking_send(lane).is_ok());
        });

        let mut lane_count = 0usize;
        while let Some(lane) = rx.recv().await {
            let mut data = Vec::with_capacity(64);
            data.extend_from_slice(&lane.lane_key.as_bytes());
            data.extend_from_slice(&lane.lane_tip.as_bytes());
            self.router
                .enqueue(make_message!(
                    Payload::SmtLaneEntry,
                    SmtLaneEntryMessage { data, blue_score: lane.blue_score, proof: Vec::new() }
                ))
                .await?;
            lane_count += 1;
        }

        debug!("Finished sending SMT state for pruning point {}: {} lanes", expected_pp, lane_count);
        self.router.enqueue(make_message!(Payload::DoneSmtChunks, DoneSmtChunksMessage {})).await?;
        Ok(())
    }

    async fn send_unexpected_pruning_point(&mut self) -> Result<(), ProtocolError> {
        self.router.enqueue(make_message!(Payload::UnexpectedPruningPoint, UnexpectedPruningPointMessage {})).await?;
        Ok(())
    }
}
