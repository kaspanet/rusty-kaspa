use std::sync::Arc;

use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue_with_request_id, make_response,
    pb::{kaspad_message::Payload, BlockLocatorMessage},
    IncomingRoute, Router,
};

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestBlockLocatorFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestBlockLocatorFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestBlockLocatorFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestBlockLocator)?;
            let (high, limit) = msg.try_into()?;

            let locator =
                self.ctx.consensus().session().await.async_create_block_locator_from_pruning_point(high, limit as usize).await?;

            self.router
                .enqueue(make_response!(
                    Payload::BlockLocator,
                    BlockLocatorMessage { hashes: locator.into_iter().map(|hash| hash.into()).collect() },
                    request_id
                ))
                .await?;
        }
    }
}
