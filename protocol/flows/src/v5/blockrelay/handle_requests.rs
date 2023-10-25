use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_core::debug;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{kaspad_message::Payload, InvRelayBlockMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;

pub struct HandleRelayBlockRequests {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for HandleRelayBlockRequests {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleRelayBlockRequests {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        // We begin by sending the current sink to the new peer. This is to help nodes to exchange
        // state even if no new blocks arrive for some reason.
        // Note: in go-kaspad this was done via a dedicated one-time flow.
        self.send_sink().await?;
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestRelayBlocks)?;
            let hashes: Vec<_> = msg.try_into()?;

            let session = self.ctx.consensus().unguarded_session();

            for hash in hashes {
                let block = session.async_get_block(hash).await?;
                self.router.enqueue(make_message!(Payload::Block, (&block).into())).await?;
                debug!("relayed block with hash {} to peer {}", hash, self.router);
            }
        }
    }

    async fn send_sink(&mut self) -> Result<(), ProtocolError> {
        let sink = self.ctx.consensus().unguarded_session().async_get_sink().await;
        if sink == self.ctx.config.genesis.hash {
            return Ok(());
        }
        self.router.enqueue(make_message!(Payload::InvRelayBlock, InvRelayBlockMessage { hash: Some(sink.into()) })).await?;
        Ok(())
    }
}
