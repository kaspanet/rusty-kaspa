use std::{cmp::max, sync::Arc};

use kaspa_consensus_core::api::ConsensusApi;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    common::ProtocolError,
    dequeue, dequeue_with_request_id, make_response,
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
        const MAX_BLOCKS: usize = 1 << 10;
        // Internal consensus logic requires that `max_blocks > mergeset_size_limit`
        let max_blocks = max(MAX_BLOCKS, self.ctx.config.mergeset_size_limit().after() as usize + 1);

        loop {
            let (msg, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestHeaders)?;
            let (high, mut low) = msg.try_into()?;

            let consensus = self.ctx.consensus();
            let mut session = consensus.session().await;

            match session.async_is_chain_ancestor_of(low, high).await {
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
                debug!("Getting block headers between {} and {}", high, low);

                // We spawn the I/O-intensive operation of reading a bunch of headers as a tokio blocking task
                let (block_headers, last) =
                    session.spawn_blocking(move |c| Self::get_headers_between(c, low, high, max_blocks)).await?;
                debug!("Got {} header hashes above {}", block_headers.len(), low);
                low = last;
                self.router.enqueue(make_response!(Payload::BlockHeaders, BlockHeadersMessage { block_headers }, request_id)).await?;

                dequeue!(self.incoming_route, Payload::RequestNextHeaders)?;
                session = consensus.session().await;
            }

            self.router.enqueue(make_response!(Payload::DoneHeaders, DoneHeadersMessage {}, request_id)).await?;
        }
    }

    /// Helper function to get a bunch of headers between `low` and `high` and to parse them into pb structs.
    /// Returns the hash of the highest block obtained, to be used as `low` for the next call
    fn get_headers_between(
        consensus: &dyn ConsensusApi,
        low: Hash,
        high: Hash,
        max_blocks: usize,
    ) -> Result<(Vec<pb::BlockHeader>, Hash), ProtocolError> {
        let hashes = consensus.get_hashes_between(low, high, max_blocks)?.0;
        let last = *hashes.last().expect("caller ensured that high and low are valid and different");
        debug!("obtained {} header hashes above {}", hashes.len(), low);
        let mut block_headers = Vec::with_capacity(hashes.len());
        for hash in hashes {
            block_headers.push(<pb::BlockHeader>::from(&*consensus.get_header(hash)?));
        }
        Ok((block_headers, last))
    }
}
