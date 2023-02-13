use kaspa_core::info;
use p2p_lib::{common::FlowError, IncomingRoute, Router};
use std::sync::Arc;

use crate::ctx::FlowContext;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    _ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    _incoming_route: IncomingRoute,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { _ctx: ctx, router, _incoming_route: incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
        self.start_ibd_with_headers_proof().await?;
        Ok(())
    }

    async fn start_ibd_with_headers_proof(&mut self) -> Result<(), FlowError> {
        info!("Starting IBD with headers proof");
        Ok(())
    }
}
