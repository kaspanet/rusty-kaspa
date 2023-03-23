use std::sync::Arc;

use kaspa_consensus_core::errors::{consensus::ConsensusError, sync::SyncManagerError};

use p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{kaspad_message::Payload, IbdChainBlockLocatorMessage},
    IncomingRoute, Router,
};

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestIbdChainBlockLocatorFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestIbdChainBlockLocatorFlow {
    fn name(&self) -> &'static str {
        "IBD_CHAIN_BLOCK_LOCATOR"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestIbdChainBlockLocatorFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestIbdChainBlockLocator)?;
            let (low, high) = msg.try_into()?;

            let locator = match self.ctx.consensus().create_headers_selected_chain_block_locator(low, high) {
                Ok(locator) => Ok(locator),
                Err(e) => {
                    let orig = e.clone();
                    if let ConsensusError::SyncManagerError(SyncManagerError::BlockNotInSelectedParentChain(_)) = e {
                        Ok(vec![])
                    } else {
                        Err(orig)
                    }
                }
            }?;

            self.router
                .enqueue(make_message!(
                    Payload::IbdChainBlockLocator,
                    IbdChainBlockLocatorMessage { block_locator_hashes: locator.into_iter().map(|hash| hash.into()).collect() }
                ))
                .await?;
        }
    }
}
