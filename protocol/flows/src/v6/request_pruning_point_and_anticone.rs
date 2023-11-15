//!
//! In v6 of the P2P protocol we dropped the filling of DAA and GHOSTDAG indices for each trusted entry
//! since the syncee no longer uses them in the rusty-kaspa design where the full sub-DAG is sent
//!

use itertools::Itertools;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_request_id, make_response,
    pb::{
        self, kaspad_message::Payload, BlockWithTrustedDataV4Message, DoneBlocksWithTrustedDataMessage, PruningPointsMessage,
        TrustedDataMessage,
    },
    IncomingRoute, Router,
};
use log::debug;
use std::sync::Arc;

use crate::{flow_context::FlowContext, flow_trait::Flow, v5::ibd::IBD_BATCH_SIZE};

pub struct PruningPointAndItsAnticoneRequestsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for PruningPointAndItsAnticoneRequestsFlow {
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
            let (_, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestPruningPointAndItsAnticone)?;
            debug!("Got request for pruning point and its anticone");

            let consensus = self.ctx.consensus();
            let mut session = consensus.session().await;

            let pp_headers = session.async_pruning_point_headers().await;
            self.router
                .enqueue(make_response!(
                    Payload::PruningPoints,
                    PruningPointsMessage { headers: pp_headers.into_iter().map(|header| <pb::BlockHeader>::from(&*header)).collect() },
                    request_id
                ))
                .await?;

            let trusted_data = session.async_get_pruning_point_anticone_and_trusted_data().await?;
            self.router
                .enqueue(make_response!(
                    Payload::TrustedData,
                    TrustedDataMessage {
                        daa_window: trusted_data.daa_window_blocks.iter().map(|daa_block| daa_block.into()).collect_vec(),
                        ghostdag_data: trusted_data.ghostdag_blocks.iter().map(|gd| gd.into()).collect_vec()
                    },
                    request_id
                ))
                .await?;

            for hashes in trusted_data.anticone.chunks(IBD_BATCH_SIZE) {
                for &hash in hashes {
                    let block = session.async_get_block(hash).await?;
                    self.router
                        .enqueue(make_response!(
                            Payload::BlockWithTrustedDataV4,
                            // No need to send window indices in v6
                            BlockWithTrustedDataV4Message { block: Some((&block).into()), ..Default::default() },
                            request_id
                        ))
                        .await?;
                }

                if hashes.len() == IBD_BATCH_SIZE {
                    // No timeout here, as we don't care if the syncee takes its time computing,
                    // since it only blocks this dedicated flow
                    drop(session); // Avoid holding the session through dequeue calls
                    dequeue!(self.incoming_route, Payload::RequestNextPruningPointAndItsAnticoneBlocks)?;
                    session = consensus.session().await;
                }
            }

            self.router
                .enqueue(make_response!(Payload::DoneBlocksWithTrustedData, DoneBlocksWithTrustedDataMessage {}, request_id))
                .await?;
            debug!("Finished sending pruning point anticone")
        }
    }
}
