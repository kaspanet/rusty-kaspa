use std::sync::Arc;

use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, make_message,
    pb::{self, kaspad_message::Payload, BlockHeadersMessage, DoneHeadersMessage},
    IncomingRoute, Router,
};
use log::debug;

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub struct RequestHeadersFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
}

#[async_trait::async_trait]
impl Flow for RequestHeadersFlow {
    fn name(&self) -> &'static str {
        "REQUEST_HEADERS"
    }

    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestHeadersFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute) -> Self {
        Self { ctx, router, incoming_route }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let msg = dequeue!(self.incoming_route, Payload::RequestHeaders)?;
            let (high, mut low) = msg.try_into()?;

            let consensus = self.ctx.consensus();
            let mut session = consensus.session().await;

            match session.is_chain_ancestor_of(low, high) {
                Ok(is_ancestor) => {
                    if !is_ancestor {
                        return Err(ProtocolError::OtherOwned(format!(
                            "get_hashes_between's low hash {} is not a chain ancestor of {}",
                            low, high
                        )));
                    }
                }
                Err(e) => return Err(e.into()),
            };
            debug!("Received RequestHeaders: high {}, low {}", high, low);

            // max_blocks MUST be > merge_set_size_limit
            while low != high {
                const MAX_BLOCKS: usize = 1 << 10;
                debug!("Getting block headers between {} and {}", high, low);
                let (hashes, _) = match session.get_hashes_between(low, high, MAX_BLOCKS) {
                    Ok(hashes) => hashes,
                    Err(e) => return Err(e.into()),
                };

                debug!("Got {} header hashes above {}", hashes.len(), low);
                low = *hashes.last().unwrap();
                let mut block_headers = Vec::with_capacity(hashes.len());
                for hash in hashes {
                    block_headers.push(<pb::BlockHeader>::from(&*session.get_header(hash)?));
                }

                self.router.enqueue(make_message!(Payload::BlockHeaders, BlockHeadersMessage { block_headers })).await?;

                drop(session); // Avoid holding the session through dequeue calls
                dequeue!(self.incoming_route, Payload::RequestNextHeaders)?;
                session = consensus.session().await;
            }

            self.router.enqueue(make_message!(Payload::DoneHeaders, DoneHeadersMessage {})).await?;
        }
    }
}
