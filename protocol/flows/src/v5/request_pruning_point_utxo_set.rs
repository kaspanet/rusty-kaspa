use std::sync::Arc;

use itertools::Itertools;
use log::debug;
use p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{
        kaspad_message::Payload,
        DonePruningPointUtxoSetChunksMessage, PruningPointUtxoSetChunkMessage,
    },
    IncomingRoute, Router,
};

use crate::{flow_context::FlowContext, flow_trait::Flow, v5::ibd::IBD_BATCH_SIZE};

pub struct RequestPruningPointUtxoSetFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointUtxoSetFlow {
    fn name(&self) -> &'static str {
        "PP_UTXOS"
    }

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
            const CHUNK_SIZE: usize = 1000;
            let mut from_outpoint = None;
            let mut chunks_sent = 0;
            loop {
                let pp_utxos =
                    self.ctx.consensus().get_pruning_point_utxos(expected_pp, from_outpoint, CHUNK_SIZE, chunks_sent != 0)?;
                debug!("Retrieved {} UTXOs for pruning point {}", pp_utxos.len(), expected_pp);

                self.router
                    .enqueue(make_message!(
                        Payload::PruningPointUtxoSetChunk,
                        PruningPointUtxoSetChunkMessage {
                            outpoint_and_utxo_entry_pairs: pp_utxos
                                .iter()
                                .map(|(outpoint, entry)| { ((outpoint, entry)).into() })
                                .collect_vec()
                        }
                    ))
                    .await?;

                let finished = pp_utxos.len() < CHUNK_SIZE;
                if finished && chunks_sent % IBD_BATCH_SIZE != 0 {
                    debug!("Finished sending UTXOs for pruning point {}", expected_pp);
                    self.router
                        .enqueue(make_message!(Payload::DonePruningPointUtxoSetChunks, DonePruningPointUtxoSetChunksMessage {}))
                        .await?;
                }

                from_outpoint = pp_utxos.last().map(|(outpoint, _)| *outpoint);
                chunks_sent += 1;

                if chunks_sent % IBD_BATCH_SIZE == 0 {
                    dequeue!(self.incoming_route, Payload::RequestPruningPointUtxoSet)?;
                }
            }
        }
    }
}
