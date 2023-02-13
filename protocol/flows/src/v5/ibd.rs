use consensus_core::api::DynConsensus;
use kaspa_core::info;
use p2p_lib::{
    common::FlowError,
    dequeue_with_timeout, make_message,
    pb::{kaspad_message::Payload, RequestPruningPointProofMessage},
    IncomingRoute, Router,
};
use std::{sync::Arc, time::Duration};

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
        self.router.route_to_network(make_message!(Payload::RequestPruningPointProof, RequestPruningPointProofMessage {})).await?;
        // Pruning proof generation and communication might take several minutes, so we allow a long 10 minute timeout
        let msg = dequeue_with_timeout!(self.incoming_route, Payload::PruningPointProof, Duration::from_secs(10 * 60))?;
        info!("received proof with overall {} headers", msg.headers.iter().map(|l| l.headers.len()).sum::<usize>());
        Ok(())
    }
}
