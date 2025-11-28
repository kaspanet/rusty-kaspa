use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_core::debug;
use kaspa_p2p_lib::{
    common::ProtocolError, dequeue_with_request_id, make_response, pb::kaspad_message::Payload, IncomingRoute, Router,
};
use std::sync::Arc;

pub struct HandleBlockBodyRequests {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for HandleBlockBodyRequests {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleBlockBodyRequests {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestBlockBodies)?;
            let hashes: Vec<_> = msg.try_into()?;
            debug!("got request for {} blocks bodies", hashes.len());
            let session = self.ctx.consensus().unguarded_session();

            for hash in hashes {
                let body = session.async_get_block_body(hash).await?;
                self.router.enqueue(make_response!(Payload::BlockBody, body.as_ref().into(), request_id)).await?;
            }
        }
    }
}
