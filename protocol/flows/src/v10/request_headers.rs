use std::{cmp::max, mem::size_of, sync::Arc};

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

pub(super) fn estimated_header_size(header: &Header, max_block_level: BlockLevel) -> usize {
    const TAG_SIZE: usize = 1; // All BlockHeader and nested-message field numbers are below 16.
    const MAX_U32_VARINT_SIZE: usize = 5;
    const MAX_U64_VARINT_SIZE: usize = 10;
    const MAX_NESTED_MESSAGE_LENGTH_SIZE: usize = 4; // Enough for the maximum accepted expanded-parent set.
    const HASH_MESSAGE_SIZE: usize = TAG_SIZE + 1 + size_of::<Hash>(); // bytes field: tag, length and value.
    const HASH_FIELD_SIZE: usize = TAG_SIZE + 1 + HASH_MESSAGE_SIZE; // Hash field: tag, message length and message.

    let parent_count = header.parents_by_level.expanded_iter().map(|parents| parents.len()).sum::<usize>();
    // BlockLevelParents envelope plus the cumulativeLevel field.
    let block_level_parents_overhead = TAG_SIZE + MAX_NESTED_MESSAGE_LENGTH_SIZE + TAG_SIZE + MAX_U32_VARINT_SIZE;
    let parent_levels_size = (usize::from(max_block_level) + 1) * block_level_parents_overhead;
    let parents_size = parent_count * HASH_FIELD_SIZE + parent_levels_size;

    // Estimate each transmitted field; the cached header hash is not sent. Scalar values use their
    // maximum protobuf varint size, while hashes and blue work include their tags and length prefixes.
    TAG_SIZE + MAX_U32_VARINT_SIZE // version
        + parents_size
        + 4 * HASH_FIELD_SIZE // hashMerkleRoot, acceptedIdMerkleRoot, utxoCommitment and pruningPoint
        + TAG_SIZE + MAX_U64_VARINT_SIZE // timestamp
        + TAG_SIZE + MAX_U32_VARINT_SIZE // bits
        + TAG_SIZE + MAX_U64_VARINT_SIZE // nonce
        + TAG_SIZE + MAX_U64_VARINT_SIZE // daaScore
        + TAG_SIZE + 1 + header.blue_work.to_be_bytes_var().len() // blueWork
        + TAG_SIZE + MAX_U64_VARINT_SIZE // blueScore
}

pub(crate) fn header_chunks<T: AsRef<Header>>(
    headers: impl Iterator<Item = T>,
    max_chunk_size: usize,
    max_block_level: BlockLevel,
) -> impl Iterator<Item = Vec<pb::BlockHeader>> {
    let mut headers = headers.map(move |header| {
        // Account for the repeated BlockHeader field's tag and length prefix in the containing chunk message.
        let header_size = 1 + 4 + estimated_header_size(header.as_ref(), max_block_level);
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlueWorkType;
    use prost::Message;

    #[test]
    fn estimated_header_size_covers_protobuf_size() {
        let headers = [
            (
                Header::new_finalized(
                    u16::MAX,
                    vec![vec![Hash::from(1u64)]].try_into().unwrap(),
                    Hash::from(2u64),
                    Hash::from(3u64),
                    Hash::from(4u64),
                    i64::MAX as u64,
                    u32::MAX,
                    u64::MAX,
                    u64::MAX,
                    BlueWorkType::from_be_bytes_var(&[u8::MAX; 24]).unwrap(),
                    u64::MAX,
                    Hash::from(5u64),
                ),
                0,
            ),
            (
                Header::new_finalized(
                    u16::MAX,
                    vec![(100, vec![Hash::from(1u64); 3_000])].try_into().unwrap(),
                    Hash::from(2u64),
                    Hash::from(3u64),
                    Hash::from(4u64),
                    i64::MAX as u64,
                    u32::MAX,
                    u64::MAX,
                    u64::MAX,
                    BlueWorkType::from_be_bytes_var(&[u8::MAX; 24]).unwrap(),
                    u64::MAX,
                    Hash::from(5u64),
                ),
                99,
            ),
        ];

        for (header, max_block_level) in &headers {
            let encoded_header = pb::BlockHeader::from(header);
            assert!(encoded_header.encoded_len() <= estimated_header_size(header, *max_block_level));
        }

        let (chunk, estimated_size) = header_chunks(std::iter::once(&headers[0].0), usize::MAX, 0).next().unwrap();
        assert!(BlockHeadersMessage { block_headers: chunk }.encoded_len() <= estimated_size);
    }
}
