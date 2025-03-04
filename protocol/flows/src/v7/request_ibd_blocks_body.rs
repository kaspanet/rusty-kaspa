use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_core::debug;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue_with_request_id, make_response,
    pb::{kaspad_message::Payload, BlockBodyMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;

pub struct HandleIbdBlockBodyRequests {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for HandleIbdBlockBodyRequests {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleIbdBlockBodyRequests {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestIbdBlocksBodies)?;
            let hashes: Vec<_> = msg.try_into()?;

            debug!("got request for {} IBD blocks bodies", hashes.len());
            let session = self.ctx.consensus().unguarded_session();

            for hash in hashes {
                let block_body = session.async_get_block(hash).await?.transactions;
                let block_body_messages = BlockBodyMessage { transactions: block_body.iter().map(|tx| tx.into()).collect() };
                self.router.enqueue(make_response!(Payload::IbdBlockBody, block_body_messages, request_id)).await?;
            }
        }
    }
}
