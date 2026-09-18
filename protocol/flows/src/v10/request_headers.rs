use std::{
    cmp::max,
    mem::{size_of, size_of_val},
    sync::Arc,
};

use kaspa_consensus_core::{BlockLevel, api::ConsensusApi, header::Header};
use kaspa_core::warn;
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, dequeue_with_request_id, make_response,
    pb::{self, BlockHeadersMessage, DoneHeadersMessage, kaspad_message::Payload},
};
use log::debug;

use crate::{flow_context::FlowContext, flow_trait::Flow};

pub(crate) const HEADERS_CHUNK_SIZE: usize = 20 * 1024 * 1024;

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
        let max_blocks = max(MAX_BLOCKS, self.ctx.config.mergeset_size_limit() as usize + 1);
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
                for block_headers in header_chunks(block_headers.into_iter(), HEADERS_CHUNK_SIZE, self.ctx.config.max_block_level) {
                    self.router
                        .enqueue(make_response!(Payload::BlockHeaders, BlockHeadersMessage { block_headers }, request_id))
                        .await?;

                    dequeue!(self.incoming_route, Payload::RequestNextHeaders)?;
                }
                session = consensus.session().await;
            }

            self.router.enqueue(make_response!(Payload::DoneHeaders, DoneHeadersMessage {}, request_id)).await?;
        }
    }

    /// Helper function to get a bunch of headers between `low` and `high`.
    /// Returns the hash of the highest block obtained, to be used as `low` for the next call
    fn get_headers_between(
        consensus: &dyn ConsensusApi,
        low: Hash,
        high: Hash,
        max_blocks: usize,
    ) -> Result<(Vec<Arc<Header>>, Hash), ProtocolError> {
        let hashes = consensus.get_hashes_between(low, high, max_blocks)?.0;
        let last = *hashes.last().expect("caller ensured that high and low are valid and different");
        debug!("obtained {} header hashes above {}", hashes.len(), low);
        let mut block_headers = Vec::with_capacity(hashes.len());
        for hash in hashes {
            block_headers.push(consensus.get_header(hash)?);
        }
        Ok((block_headers, last))
    }
}

fn estimated_header_size(header: &Header, max_block_level: BlockLevel) -> usize {
    let parent_count = header.parents_by_level.expanded_iter().map(|parents| parents.len()).sum::<usize>();
    let parents_size = parent_count * size_of::<Hash>()
        + usize::from(max_block_level)
            * (
                3 // Length varint upper bound for roughly 0.5M parents.
            + size_of::<u32>()
                // cumulativeLevel field
            );

    // Estimate each transmitted field; the cached header hash is not sent.
    size_of::<u32>() // Version is encoded as uint32 in protobuf.
        + parents_size
        + size_of_val(&header.hash_merkle_root)
        + size_of_val(&header.accepted_id_merkle_root)
        + size_of_val(&header.utxo_commitment)
        + size_of_val(&header.timestamp)
        + size_of_val(&header.bits)
        + size_of_val(&header.nonce)
        + size_of_val(&header.daa_score)
        + size_of_val(&header.blue_work)
        + size_of_val(&header.blue_score)
        + size_of_val(&header.pruning_point)
}

pub(crate) fn header_chunks<T: AsRef<Header>>(
    headers: impl Iterator<Item = T>,
    max_chunk_size: usize,
    max_block_level: BlockLevel,
) -> impl Iterator<Item = Vec<pb::BlockHeader>> {
    let mut headers = headers.map(move |header| {
        let header_size = estimated_header_size(header.as_ref(), max_block_level);
        (header, header_size)
    });
    let mut next_header = headers.next();

    std::iter::from_fn(move || {
        next_header.as_ref()?;
        let mut chunk = Vec::new();
        let mut chunk_size = 0;

        while let Some((header, header_size)) = next_header.take() {
            // We allow a single header to exceed the chunk size budget. The rationale is that the receiver can
            // change its message size policy to accommodate the large header, so we let the receiver decide
            // whether to accept it or not.
            if !chunk.is_empty() && chunk_size + header_size > max_chunk_size {
                next_header = Some((header, header_size));
                break;
            }
            if header_size > max_chunk_size {
                warn!(
                    "Header with hash {} is larger than the chunk size budget ({} > {})",
                    header.as_ref().hash,
                    header_size,
                    max_chunk_size
                );
            }
            chunk_size += header_size;
            chunk.push(header.as_ref().into());
            next_header = headers.next();
        }

        Some(chunk)
    })
}
