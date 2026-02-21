use std::sync::Arc;

use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    convert::header::HeaderFormat,
    dequeue_with_request_id, make_response,
    pb::{PruningPointProofMessage, kaspad_message::Payload},
};
use log::debug;

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestPruningPointProofFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
    header_format: HeaderFormat,
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
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, header_format: HeaderFormat) -> Self {
        Self { ctx, router, incoming_route, header_format }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (_, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestPruningPointProof)?;
            debug!("Got pruning point proof request");
            let proof = self.ctx.consensus().unguarded_session().async_get_pruning_point_proof().await;
            self.router
                .enqueue(make_response!(
                    Payload::PruningPointProof,
                    PruningPointProofMessage { headers: proof.iter().map(|headers| (self.header_format, headers).into()).collect() },
                    request_id
                ))
                .await?;
            debug!("Sent pruning point proof");
        }
    }
}
