use crate::{flow_context::FlowContext, flow_trait::Flow, v5::ibd::IBD_BATCH_SIZE};
use itertools::Itertools;
use kaspa_consensus_core::errors::consensus::ConsensusError;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{
        kaspad_message::Payload, DonePruningPointUtxoSetChunksMessage, PruningPointUtxoSetChunkMessage, UnexpectedPruningPointMessage,
    },
    IncomingRoute, Router,
};
use std::sync::Arc;

pub struct RequestPruningPointUtxoSetFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointUtxoSetFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointUtxoSetFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let expected_pp = dequeue!(self.incoming_route, Payload::RequestPruningPointUtxoSet)?.try_into()?;
            self.handle_request(expected_pp).await?
        }
    }

    async fn handle_request(&mut self, expected_pp: Hash) -> Result<(), ProtocolError> {
        const CHUNK_SIZE: usize = 1000;
        let mut from_outpoint = None;
        let mut chunks_sent = 0;

        let consensus = self.ctx.consensus();
        let mut session = consensus.session().await;
        let mut is_start = true;
        let to_process = session.async_get_pruning_point_utxoset_size().await;
        let mut processed = 0;
        loop {
            // We avoid keeping the consensus session across the limitless dequeue call below
            let pruning_point_utxos =
                match session.async_get_pruning_point_utxos(expected_pp, from_outpoint, CHUNK_SIZE, chunks_sent != 0).await {
                    Err(ConsensusError::UnexpectedPruningPoint) => return self.send_unexpected_pruning_point_message().await,
                    res => res,
                }?;

            // Update the total processed count
            processed += pruning_point_utxos.len() as u64;

            debug!(
                "Retrieved {0} + {1} / {2} UTXOs from pp {3} ({4:.0}%)",
                processed,
                pruning_point_utxos.len(),
                to_process,
                expected_pp,
                (processed as f64 / to_process as f64) * 100.0
            );

            // Send the chunk
            self.router
                .enqueue(make_message!(
                    Payload::PruningPointUtxoSetChunk,
                    PruningPointUtxoSetChunkMessage {
                        outpoint_and_utxo_entry_pairs: pruning_point_utxos
                            .iter()
                            .map(|(outpoint, entry)| { (outpoint, entry).into() })
                            .collect_vec(),
                        utxoset_size: if is_start { to_process } else { 0u64 }, // Only send the size in the first chunk, see comment in p2p.proto for more information.
                    }
                ))
                .await?;

            chunks_sent += 1;
            if chunks_sent % IBD_BATCH_SIZE == 0 {
                drop(session); // Avoid holding the session through dequeue calls
                dequeue!(self.incoming_route, Payload::RequestNextPruningPointUtxoSetChunk)?;
                session = consensus.session().await;
            }

            // This indicates that there are no more entries to query
            if to_process == processed {
                return self.send_done_message(expected_pp).await;
            }

            // Mark the beginning of the next chunk
            from_outpoint = Some(pruning_point_utxos.last().expect("not empty by prev condition").0);

            is_start = false;
        }
    }

    async fn send_unexpected_pruning_point_message(&mut self) -> Result<(), ProtocolError> {
        self.router.enqueue(make_message!(Payload::UnexpectedPruningPoint, UnexpectedPruningPointMessage {})).await?;
        Ok(())
    }

    async fn send_done_message(&mut self, expected_pp: Hash) -> Result<(), ProtocolError> {
        debug!("Finished sending UTXOs for pruning point {}", expected_pp);
        self.router.enqueue(make_message!(Payload::DonePruningPointUtxoSetChunks, DonePruningPointUtxoSetChunksMessage {})).await?;
        Ok(())
    }
}
