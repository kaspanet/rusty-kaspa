use p2p_lib::{common::FlowError, IncomingRoute, Router};
use std::sync::Arc;

use crate::ctx::FlowContext;

/// Flow for managing IBD - Initial Block Download
pub struct IbdFlow {
    ctx: FlowContext,
    pub router: Arc<Router>, // TODO: remove pub
    incoming_route: IncomingRoute,
}

impl IbdFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    pub async fn start(&self) -> Result<(), FlowError> {
        todo!()
    }
}
