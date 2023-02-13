use consensus_core::api::DynConsensus;
use kaspa_core::info;
use p2p_lib::{common::FlowError, IncomingRoute, Router};
use std::sync::Arc;

use crate::ctx::FlowContext;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    _incoming_route: IncomingRoute,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, _incoming_route: incoming_route }
    }

    pub async fn start(&mut self) -> Result<(), FlowError> {
        self.start_ibd_with_headers_proof().await?;
        Ok(())
    }

    async fn start_ibd_with_headers_proof(&mut self) -> Result<(), FlowError> {
        info!("Starting IBD with headers proof");
        let consensus = self.ctx.consensus();
        self.sync_and_validate_pruning_proof(&consensus).await?;
        Ok(())
    }

    async fn sync_and_validate_pruning_proof(&mut self, _consensus: &DynConsensus) -> Result<(), FlowError> {
        Ok(())
    }
}
