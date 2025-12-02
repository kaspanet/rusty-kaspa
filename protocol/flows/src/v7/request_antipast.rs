use crate::{flow_context::FlowContext, flow_trait::Flow};
use kaspa_consensus_core::errors::consensus::ConsensusError;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue_with_request_id, make_response,
    pb::{kaspad_message::Payload, BlockHeadersMessage, DoneHeadersMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;

pub struct HandleAntipastRequests {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for HandleAntipastRequests {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl HandleAntipastRequests {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestAntipast)?;
            let (block, context): (Hash, Hash) = msg.try_into()?;

            debug!("received anticone request with block hash: {}, context hash: {} for peer {}", block, context, self.router);

            let consensus = self.ctx.consensus();
            let session = consensus.session().await;

            // `RequestAntipast` is expected to be called by the syncee for getting the antipast of `sink`
            // intersected by past of the relayed block. We do not expect the relay block to be too much after
            // the sink (in fact usually it should be in its past or anticone), hence we bound the expected traversal to be
            // in the order of `mergeset_size_limit`.
            let hashes =
                session.async_get_antipast_from_pov(block, context, Some(self.ctx.config.mergeset_size_limit().after() * 4)).await?;
            let mut headers = session
                .spawn_blocking(|c| hashes.into_iter().map(|h| c.get_header(h)).collect::<Result<Vec<_>, ConsensusError>>())
                .await?;
            debug!("got {} headers in anticone({}) cap past({}) for peer {}", headers.len(), block, context, self.router);

            // Sort the headers in bottom-up topological order before sending
            headers.sort_by(|a, b| a.blue_work.cmp(&b.blue_work));

            self.router
                .enqueue(make_response!(
                    Payload::BlockHeaders,
                    BlockHeadersMessage { block_headers: headers.into_iter().map(|header| header.as_ref().into()).collect() },
                    request_id
                ))
                .await?;
            self.router.enqueue(make_response!(Payload::DoneHeaders, DoneHeadersMessage {}, request_id)).await?;
        }
    }
}
