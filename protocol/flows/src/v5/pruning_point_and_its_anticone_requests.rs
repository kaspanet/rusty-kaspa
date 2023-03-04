use std::sync::Arc;

use itertools::Itertools;
use log::debug;
use p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{
        self, kaspad_message::Payload, BlockWithTrustedDataV4Message, DoneBlocksWithTrustedDataMessage, PruningPointsMessage,
        TrustedDataMessage,
    },
    IncomingRoute, Router,
};

use crate::{flow_context::FlowContext, flow_trait::Flow, v5::ibd::IBD_BATCH_SIZE};

pub struct PruningPointAndItsAnticoneRequestsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for PruningPointAndItsAnticoneRequestsFlow {
    fn name(&self) -> &'static str {
        "PP_ANTICONE"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl PruningPointAndItsAnticoneRequestsFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            dequeue!(self.incoming_route, Payload::RequestPruningPointAndItsAnticone)?;
            debug!("Got request for pruning point and its anticone");
            let pp_headers = self.ctx.consensus().pruning_point_headers();
            self.router
                .enqueue(make_message!(
                    Payload::PruningPoints,
                    PruningPointsMessage { headers: pp_headers.into_iter().map(|header| <pb::BlockHeader>::from(&*header)).collect() }
                ))
                .await?;

            let trusted_data = self.ctx.consensus().get_pruning_point_anticone_and_trusted_data();
            let pp_anticone = &trusted_data.0;
            let daa_window = &trusted_data.1;
            let ghostdag_data = &trusted_data.2;
            self.router
                .enqueue(make_message!(
                    Payload::TrustedData,
                    TrustedDataMessage {
                        daa_window: daa_window.iter().map(|daa_block| daa_block.into()).collect_vec(),
                        ghostdag_data: ghostdag_data.iter().map(|gd| gd.into()).collect_vec()
                    }
                ))
                .await?;

            for hashes in pp_anticone.chunks(IBD_BATCH_SIZE) {
                for hash in hashes {
                    let block = self.ctx.consensus().get_block(*hash)?;
                    self.router
                        .enqueue(make_message!(
                            Payload::BlockWithTrustedDataV4,
                            BlockWithTrustedDataV4Message {
                                block: Some((&block).into()),
                                daa_window_indices: vec![], // TODO: Fill with real values to be compatible with go-kaspad
                                ghostdag_data_indices: vec![]  // TODO: Fill with real values to be compatible with go-kaspad
                            }
                        ))
                        .await?;
                }

                // No timeout here, as we don't care if the syncee takes its time computing,
                // since it only blocks this dedicated flow
                dequeue!(self.incoming_route, Payload::RequestNextPruningPointAndItsAnticoneBlocks)?;
            }

            self.router.enqueue(make_message!(Payload::DoneBlocksWithTrustedData, DoneBlocksWithTrustedDataMessage {})).await?;
            debug!("Finished sending pruning point anticone")
        }
    }
}
