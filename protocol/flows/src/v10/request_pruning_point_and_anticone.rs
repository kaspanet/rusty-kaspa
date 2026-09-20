//!
//! In v6 of the P2P protocol we dropped the filling of DAA and GHOSTDAG indices for each trusted entry
//! since the syncee no longer uses them in the rusty-kaspa design where the full sub-DAG is sent
//!

use itertools::Itertools;
use kaspa_consensus_core::{block::Block, trusted::TrustedHeader};
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue, dequeue_with_request_id, make_response,
    pb::{
        BlockWithTrustedDataV4Message, DoneBlocksWithTrustedDataMessage, PruningPointsMessage, TrustedDataChunkMessage,
        TrustedDataChunksEndMessage, TrustedDataMessage, kaspad_message::Payload,
    },
};
use log::{debug, info, warn};
use std::{mem::size_of_val, sync::Arc};

use super::request_headers::estimated_header_size;

const TRUSTED_DATA_CHUNK_SIZE: usize = 20 * 1024 * 1024;

use crate::{flow_context::FlowContext, flow_trait::Flow, ibd::IBD_BATCH_SIZE};

pub struct PruningPointAndItsAnticoneRequestsFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
    use_trusted_data_chunks: bool,
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
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, use_trusted_data_chunks: bool) -> Self {
        Self { ctx, router, incoming_route, use_trusted_data_chunks }
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
                    PruningPointsMessage { headers: pp_headers.into_iter().map(|header| (&*header).into()).collect() },
                    request_id
                ))
                .await?;

            let trusted_data = session.async_get_pruning_point_anticone_and_trusted_data().await?;
            if self.use_trusted_data_chunks {
                for (i, (chunk, chunk_size)) in
                    trusted_data_chunks(&trusted_data.daa_window_blocks, TRUSTED_DATA_CHUNK_SIZE).enumerate()
                {
                    let count = chunk.headers.len();
                    self.router.enqueue(make_response!(Payload::TrustedDataChunk, chunk, request_id)).await?;
                    info!("Sent trusted data chunk #{}: {} DAA blocks, estimated size {} bytes", i + 1, count, chunk_size);
                }
                self.router.enqueue(make_response!(Payload::TrustedDataChunksEnd, TrustedDataChunksEndMessage {}, request_id)).await?;
            } else {
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
            }

            //
            // TODO(relaxed): consider refactoring GRPC to properly include the header-only chain segment and cleanup old fields
            //

            let blocks_iter = trusted_data
                .anticone
                .iter()
                .copied()
                .map(|hash| (hash, false))
                .chain(trusted_data.header_only_chain_segment.iter().copied().map(|hash| (hash, true)));
            for (i, (hash, send_header_only)) in blocks_iter.enumerate() {
                let block = if send_header_only {
                    let header = session.async_get_header(hash).await?;
                    Block::from_header_arc(header)
                } else {
                    session.async_get_block(hash).await?
                };
                self.router
                    .enqueue(make_response!(
                        Payload::BlockWithTrustedDataV4,
                        // No need to send window indices since v6
                        BlockWithTrustedDataV4Message { block: Some((&block).into()), ..Default::default() },
                        request_id
                    ))
                    .await?;
                let sent = i + 1;
                if sent.is_multiple_of(IBD_BATCH_SIZE) {
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

fn estimated_trusted_header_size(header: &TrustedHeader) -> usize {
    // A nested protobuf Hash has a tag and length for both the message and its 32-byte value.
    const HASH_SIZE: usize = 32 + 4;
    // Each anticone-size entry wraps a Hash and a uint32 (up to five varint bytes).
    const ANTICONE_ENTRY_SIZE: usize = 2 + HASH_SIZE + 1 + 5;
    let ghostdag = &header.ghostdag;
    let ghostdag_size = 1 + 10 // blue_score: tag and uint64 varint upper bound
        + 2 + size_of_val(&ghostdag.blue_work)
        + HASH_SIZE // selected_parent
        + (ghostdag.mergeset_blues.len() + ghostdag.mergeset_reds.len()) * HASH_SIZE
        + ghostdag.blues_anticone_sizes.len() * ANTICONE_ENTRY_SIZE;

    // Include the header, ghostdag and DaaBlockV4 message tags and length prefixes.
    estimated_header_size(&header.header) + ghostdag_size + 3 * (1 + 5)
}

fn trusted_data_chunks(
    headers: &[TrustedHeader],
    max_chunk_size: usize,
) -> impl Iterator<Item = (TrustedDataChunkMessage, usize)> + '_ {
    let mut headers = headers.iter().peekable();
    std::iter::from_fn(move || {
        headers.peek()?;
        let mut chunk_headers = Vec::new();
        let mut chunk_size = 0;
        while let Some(&header) = headers.peek() {
            let header_size = estimated_trusted_header_size(header);
            // As with proof headers, let the receiver decide whether to accept a single oversized entry.
            if !chunk_headers.is_empty() && chunk_size + header_size > max_chunk_size {
                break;
            }
            if header_size > max_chunk_size {
                warn!("Trusted header {} exceeds the chunk size budget ({} > {})", header.header.hash, header_size, max_chunk_size);
            }
            chunk_headers.push(header.into());
            chunk_size += header_size;
            headers.next();
        }
        Some((TrustedDataChunkMessage { headers: chunk_headers }, chunk_size))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ibd::trusted_data::receive_trusted_data;
    use kaspa_consensus_core::{header::Header, trusted::ExternalGhostdagData};
    use kaspa_hashes::Hash;
    use kaspa_p2p_lib::{convert::model::trusted::TrustedDataEntry, make_message, pb};
    use prost::Message;
    use tokio::sync::mpsc;

    fn trusted_header(nonce: u64, parent_levels: u8, parents: usize, mergeset_size: usize) -> TrustedHeader {
        let mut header = Header::from_precomputed_hash(Default::default(), vec![]);
        header.nonce = nonce;
        header.parents_by_level = vec![(parent_levels, vec![Hash::from(1u64); parents])].try_into().unwrap();
        header.finalize();
        TrustedHeader::new(
            Arc::new(header),
            ExternalGhostdagData {
                blue_score: u64::MAX,
                blue_work: 123.into(),
                selected_parent: Hash::from(1u64),
                mergeset_blues: vec![Hash::from(2u64); mergeset_size],
                mergeset_reds: vec![Hash::from(3u64); mergeset_size],
                blues_anticone_sizes: [(Hash::from(2u64), 42)].into_iter().collect(),
            },
        )
    }

    #[test]
    fn trusted_chunks_respect_budget_and_include_ghostdag_size() {
        let small = trusted_header(0, 1, 1, 0);
        let large = trusted_header(1, 1, 1, 1000);
        let small_size = estimated_trusted_header_size(&small);
        let large_size = estimated_trusted_header_size(&large);
        assert_eq!(large_size - small_size, 2 * 1000 * 36);

        let headers = [small, large, trusted_header(2, 1, 1, 1000), trusted_header(3, 1, 1, 0)];
        let budget = small_size + large_size;
        let chunks = trusted_data_chunks(&headers, budget).collect::<Vec<_>>();
        assert_eq!(chunks.iter().map(|(chunk, _)| chunk.headers.len()).collect::<Vec<_>>(), [2, 2]);
        for (chunk, size) in &chunks {
            assert_eq!(*size, budget);
            assert!(chunk.encoded_len() <= *size);
        }
        let flattened = chunks.into_iter().flat_map(|(chunk, _)| chunk.headers).collect::<Vec<_>>();
        assert_eq!(flattened, headers.iter().map(pb::DaaBlockV4::from).collect::<Vec<_>>());
    }

    #[test]
    fn trusted_chunks_use_compressed_parent_size_and_twenty_mib_budget() {
        let headers = (0..5).map(|i| trusted_header(i, 1, 250_000, 10)).collect::<Vec<_>>();
        let chunks = trusted_data_chunks(&headers, TRUSTED_DATA_CHUNK_SIZE).collect::<Vec<_>>();
        assert_eq!(chunks.iter().map(|(chunk, _)| chunk.headers.len()).collect::<Vec<_>>(), [2, 2, 1]);
        for (chunk, size) in chunks {
            assert!(size <= TRUSTED_DATA_CHUNK_SIZE);
            assert!(chunk.encoded_len() <= size);
        }
    }

    #[test]
    fn trusted_chunks_isolate_oversized_entries_and_skip_empty_input() {
        assert!(trusted_data_chunks(&[], 1).next().is_none());
        let headers = [trusted_header(0, 1, 1, 0), trusted_header(1, 1, 1, 1000), trusted_header(2, 1, 1, 0)];
        let budget = estimated_trusted_header_size(&headers[0]) * 2;
        let chunks = trusted_data_chunks(&headers, budget).collect::<Vec<_>>();
        assert_eq!(chunks.iter().map(|(chunk, _)| chunk.headers.len()).collect::<Vec<_>>(), [1, 1, 1]);
        assert!(chunks[0].1 <= budget);
        assert!(chunks[1].1 > budget);
        assert!(chunks[2].1 <= budget);
    }

    #[tokio::test]
    async fn trusted_chunks_roundtrip_and_build_subdag_without_standalone_ghostdag_data() {
        let headers = (0..7).map(|i| trusted_header(i, 1, 1, 2)).collect::<Vec<_>>();
        let budget = estimated_trusted_header_size(&headers[0]) * 2;
        let chunks = trusted_data_chunks(&headers, budget).collect::<Vec<_>>();
        assert_eq!(chunks.len(), 4);
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        let send = async move {
            for (chunk, _) in chunks {
                tx.send(make_message!(Payload::TrustedDataChunk, chunk)).await.unwrap();
            }
            tx.send(make_message!(Payload::TrustedDataChunksEnd, TrustedDataChunksEndMessage {})).await.unwrap();
        };
        let (_, received) = tokio::join!(send, receive_trusted_data(&mut route, true));
        let received = received.unwrap();
        assert!(received.ghostdag_window.is_empty());
        assert_eq!(
            received.daa_window.iter().map(pb::DaaBlockV4::from).collect::<Vec<_>>(),
            headers.iter().map(pb::DaaBlockV4::from).collect::<Vec<_>>()
        );
        let entries = vec![TrustedDataEntry::new(Block::from_header_arc(headers[0].header.clone()), vec![], vec![])];
        let blocks = received.build_trusted_subdag(entries).unwrap();
        assert_eq!(blocks.len(), headers.len());
        for header in headers {
            let block = blocks.iter().find(|block| block.block.hash() == header.header.hash).unwrap();
            assert_eq!(block.ghostdag.blue_score, header.ghostdag.blue_score);
            assert_eq!(block.ghostdag.blue_work, header.ghostdag.blue_work);
        }
    }
}
