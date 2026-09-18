use std::sync::Arc;

use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::ProtocolError,
    dequeue_with_request_id, make_response,
    pb::{PruningPointProofChunkMessage, PruningPointProofChunksEndMessage, PruningPointProofMessage, kaspad_message::Payload},
};
use log::debug;

use crate::{
    flow_context::FlowContext,
    flow_trait::Flow,
    v10::request_headers::{HEADERS_CHUNK_SIZE, header_chunks},
};

pub struct RequestPruningPointProofFlow {
    ctx: FlowContext,
    router: Arc<Router>,
    incoming_route: IncomingRoute,
    use_pruning_proof_chunks: bool,
}

#[async_trait::async_trait]
impl Flow for RequestPruningPointProofFlow {
    fn router(&self) -> Option<Arc<Router>> {
        Some(self.router.clone())
    }

    async fn start(&mut self) -> Result<(), ProtocolError> {
        self.start_impl().await
    }
}

impl RequestPruningPointProofFlow {
    pub fn new(ctx: FlowContext, router: Arc<Router>, incoming_route: IncomingRoute, use_pruning_proof_chunks: bool) -> Self {
        Self { ctx, router, incoming_route, use_pruning_proof_chunks }
    }

    async fn start_impl(&mut self) -> Result<(), ProtocolError> {
        loop {
            let (_, request_id) = dequeue_with_request_id!(self.incoming_route, Payload::RequestPruningPointProof)?;
            debug!("Got pruning point proof request");
            let proof = self.ctx.consensus().unguarded_session().async_get_pruning_point_proof().await;
            if self.use_pruning_proof_chunks {
                // Send levels from highest to lowest to allow on-the-fly proof validation in the future.
                for (level, headers) in proof.iter().enumerate().rev() {
                    for chunk in header_chunks(headers.iter(), HEADERS_CHUNK_SIZE, self.ctx.config.max_block_level) {
                        self.router
                            .enqueue(make_response!(
                                Payload::PruningPointProofChunk,
                                PruningPointProofChunkMessage { chunk, level: level as u32 },
                                request_id
                            ))
                            .await?;
                    }
                }
                self.router
                    .enqueue(make_response!(Payload::PruningPointProofChunksEnd, PruningPointProofChunksEndMessage {}, request_id))
                    .await?;
            } else {
                self.router
                    .enqueue(make_response!(
                        Payload::PruningPointProof,
                        PruningPointProofMessage { headers: proof.iter().map(|headers| headers.into()).collect() },
                        request_id
                    ))
                    .await?;
            }
            debug!("Sent pruning point proof");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::header::Header;
    use kaspa_hashes::Hash;

    fn header_with_parents(levels: u8, parents_per_level: usize, nonce: u64) -> Arc<Header> {
        let mut header = Header::from_precomputed_hash(Default::default(), vec![]);
        header.parents_by_level = vec![(levels, vec![Hash::from(1u64); parents_per_level])].try_into().unwrap();
        header.nonce = nonce;
        Arc::new(header)
    }

    #[test]
    fn header_chunks_pack_by_expanded_header_size() {
        let large = header_with_parents(100, 2500, 1);
        let small = header_with_parents(1, 1, 2);
        // The large header has one compressed run but 250,000 expanded parents.
        // Two such headers fit alongside 200 small headers; a third does not.
        let mut first_level = vec![large.clone(), large.clone()];
        first_level.extend(vec![small.clone(); 200]);
        first_level.extend([large.clone(), large]);
        let proof = [first_level, vec![small; 201]];
        let chunks = proof
            .iter()
            .enumerate()
            .rev()
            .flat_map(|(level, headers)| {
                header_chunks(headers.iter(), HEADERS_CHUNK_SIZE, 250)
                    .map(move |chunk| PruningPointProofChunkMessage { chunk, level: level as u32 })
            })
            .collect::<Vec<_>>();
        assert_eq!(chunks.iter().map(|chunk| (chunk.level, chunk.chunk.len())).collect::<Vec<_>>(), [(1, 201), (0, 202), (0, 2)]);

        let mut consumed = vec![0; proof.len()];
        for chunk in chunks {
            let level = chunk.level as usize;
            let start = consumed[level];
            let end = start + chunk.chunk.len();
            let headers = &proof[level][start..end];
            assert_eq!(chunk.chunk, headers.iter().map(|header| header.as_ref().into()).collect::<Vec<_>>());
            consumed[level] = end;
        }
        assert_eq!(consumed, proof.iter().map(Vec::len).collect::<Vec<_>>());
    }

    #[test]
    fn header_chunks_allow_a_single_header_larger_than_the_budget() {
        let oversized = header_with_parents(250, 2500, 1);
        let chunks = header_chunks(std::iter::once(oversized.clone()), 1, 250).collect::<Vec<_>>();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], vec![oversized.as_ref().into()]);
    }
}
