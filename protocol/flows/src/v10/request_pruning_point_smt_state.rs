use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    ibd::{SMT_CHUNK_SIZE, SMT_FLOW_CONTROL_WINDOW},
};
use kaspa_consensus_core::{api::ImportLane, errors::consensus::ConsensusError};
use kaspa_core::{debug, info};
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
        drop(session);

        let expected_count = metadata.active_lanes_count;

        let mut md_bytes = Vec::with_capacity(96);
        md_bytes.extend_from_slice(&metadata.lanes_root.as_bytes());
        md_bytes.extend_from_slice(&metadata.payload_and_ctx_digest.as_bytes());
        md_bytes.extend_from_slice(&metadata.parent_seq_commit.as_bytes());
        self.router
            .enqueue(make_message!(Payload::SmtMetadata, SmtMetadataMessage { data: md_bytes, active_lanes_count: expected_count }))
            .await?;

        if expected_count == 0 {
            debug!("Finished sending SMT state for pruning point {}: 0 lanes", expected_pp);
            return Ok(());
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<ImportLane>>(256);

        let session_for_reader = consensus.unguarded_session();
        let stream = match session_for_reader.open_pruning_point_smt_lane_stream(expected_pp) {
            Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point().await,
            res => res,
        }?;
        drop(session_for_reader);
        let reader_handle = tokio::task::spawn_blocking(move || -> Result<u64, ConsensusError> {
            let mut count: u64 = 0;
            let mut batch: Vec<ImportLane> = Vec::with_capacity(SMT_CHUNK_SIZE);
            for item in stream {
                batch.push(item?);
                if batch.len() == SMT_CHUNK_SIZE {
                    count += batch.len() as u64;
                    if tx.blocking_send(std::mem::take(&mut batch)).is_err() {
                        // Receiver went away — let the async side surface the error.
                        return Ok(count);
                    }
                    batch.reserve(SMT_CHUNK_SIZE);
                }
            }
            if !batch.is_empty() {
                count += batch.len() as u64;
                let _ = tx.blocking_send(batch);
            }
            Ok(count)
        });

        let mut lanes_sent: u64 = 0;
        let mut chunks_sent: usize = 0;

        while let Some(batch) = rx.recv().await {
            let entries: Vec<SmtLaneEntry> = batch
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

            // Flow-control round-trip. Skip it on the last window so the peer
            // never has to send a trailing RequestNext just to unblock us.
            if lanes_sent < expected_count && chunks_sent.is_multiple_of(SMT_FLOW_CONTROL_WINDOW) {
                dequeue!(self.incoming_route, Payload::RequestNextPruningPointSmtChunk)?;
            }
        }

        let reader_count = match reader_handle.await {
            Ok(Ok(count)) => count,
            Ok(Err(ConsensusError::UnexpectedPruningPoint)) => return self.send_unexpected_pruning_point().await,
            Ok(Err(e)) => return Err(ProtocolError::OtherOwned(format!("SMT lane stream error: {e}"))),
            Err(e) => return Err(ProtocolError::OtherOwned(format!("SMT lane reader task panicked: {e}"))),
        };

        assert!(lanes_sent == reader_count && lanes_sent == expected_count);

        info!("Finished sending SMT state for pruning point {}: {} lanes in {} chunks", expected_pp, lanes_sent, chunks_sent);
        Ok(())
    }

    async fn send_unexpected_pruning_point(&mut self) -> Result<(), ProtocolError> {
        self.router.enqueue(make_message!(Payload::UnexpectedPruningPoint, UnexpectedPruningPointMessage {})).await?;
        Ok(())
    }
}
