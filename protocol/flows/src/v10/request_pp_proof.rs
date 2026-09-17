use std::sync::Arc;

use kaspa_consensus_core::pruning::PruningPointProof;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue_with_request_id, make_response,
    pb::{PruningPointProofChunkMessage, PruningPointProofChunksEndMessage, PruningPointProofMessage, kaspad_message::Payload},
};
use log::debug;

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestPruningPointProofFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
    use_pruning_proof_chunks: bool,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointProofFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointProofFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, use_pruning_proof_chunks: bool) -> Self {
        Self { ctx, router, incoming_route, use_pruning_proof_chunks }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (_, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestPruningPointProof)?;
            debug!("Got pruning point proof request");
            let proof = self.ctx.consensus().unguarded_session().async_get_pruning_point_proof().await;
            if self.use_pruning_proof_chunks {
                for chunk in proof_chunks(&proof) {
                    self.router.enqueue(make_response!(Payload::PruningPointProofChunk, chunk, request_id)).await?;
                }
                self.router
                    .enqueue(make_response!(Payload::PruningPointProofChunksEnd, PruningPointProofChunksEndMessage {}, request_id))
                    .await?;
            } else {
                self.router
                    .enqueue(make_response!(
                        Payload::PruningPointProof,
                        PruningPointProofMessage { headers: proof.iter().map(|headers| headers.into()).collect() },
                        request_id
                    ))
                    .await?;
            }
            debug!("Sent pruning point proof");
        }
    }
}

const PRUNING_POINT_PROOF_CHUNK_SIZE: usize = 100;

/// Send each level in order, using one empty chunk to preserve an empty level.
fn proof_chunks(proof: &PruningPointProof) -> impl Iterator<Item = PruningPointProofChunkMessage> + '_ {
    proof.iter().enumerate().flat_map(|(level, headers)| {
        headers.chunks(PRUNING_POINT_PROOF_CHUNK_SIZE).chain(headers.is_empty().then_some(&[][..])).map(move |headers| {
            PruningPointProofChunkMessage { chunk: headers.iter().map(|header| header.as_ref().into()).collect(), level: level as u32 }
        })
    })
}
