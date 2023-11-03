use std::sync::Arc;

use kaspa_consensus_core::errors::{consensus::ConsensusError, sync::SyncManagerError};
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue_with_request_id, make_response,
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
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestIbdChainBlockLocator)?;
            let (low, high) = msg.try_into()?;

            let locator =
                match (self.ctx.consensus().session().await).async_create_virtual_selected_chain_block_locator(low, high).await {
                    Ok(locator) => Ok(locator),
                    Err(e) => {
                        let orig = e.clone();
                        if let ConsensusError::SyncManagerError(SyncManagerError::BlockNotInSelectedParentChain(_)) = e {
                            // This signals a reset to the locator zoom-in process. The syncee is expected to restart the search
                            Ok(vec![])
                        } else {
                            Err(orig)
                        }
                    }
                }?;

            self.router
                .enqueue(make_response!(
                    Payload::IbdChainBlockLocator,
                    IbdChainBlockLocatorMessage { block_locator_hashes: locator.into_iter().map(|hash| hash.into()).collect() },
                    request_id
                ))
                .await?;
        }
    }
}
