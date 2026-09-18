use kaspa_consensus_core::{BlockLevel, pruning::PruningPointProof};
use kaspa_p2p_lib::{
    IncomingRoute,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    dequeue_with_timeout,
    pb::{PruningPointProofHeaderArray, PruningPointProofMessage, kaspad_message::Payload},
};
use log::info;
use std::time::Duration;

pub(super) async fn receive_pruning_point_proof(
    incoming_route: &mut IncomingRoute,
    use_pruning_proof_chunks: bool,
) -> Result<PruningPointProof, ProtocolError> {
    if !use_pruning_proof_chunks {
        let msg = dequeue_with_timeout!(incoming_route, Payload::PruningPointProof, Duration::from_secs(600))?;
        return Ok(msg.try_into()?);
    }

    let mut proof = PruningPointProofMessage { headers: Vec::new() };
    let mut current_level: Option<BlockLevel> = None;
    let mut current_headers = PruningPointProofHeaderArray { headers: Vec::new() };
    let mut chunk_count = 0;
    // Proof generation can take several minutes, so we start with a long timeout and reset it to the default after the first chunk is received.
    let mut timeout = Duration::from_secs(600);
    loop {
        let msg = tokio::time::timeout(timeout, incoming_route.recv())
            .await
            .map_err(|_| ProtocolError::Timeout(timeout))?
            .ok_or(ProtocolError::ConnectionClosed)?;
        match msg.payload {
            Some(Payload::PruningPointProofChunk(chunk)) => {
                timeout = DEFAULT_TIMEOUT;
                let level =
                    BlockLevel::try_from(chunk.level).map_err(|_| ProtocolError::Other("Invalid pruning point proof chunk level"))?;
                if let Some(current_level) = current_level {
                    if level > current_level || level < current_level.saturating_sub(1) {
                        return Err(ProtocolError::Other(
                            "Pruning point proof chunk levels must be weakly monotone and decrease by at most one",
                        ));
                    }
                    if level < current_level {
                        proof.headers.push(current_headers);
                        current_headers = PruningPointProofHeaderArray { headers: Vec::new() };
                    }
                }
                current_level = Some(level);
                current_headers.headers.extend(chunk.chunk);
                chunk_count += 1;
                info!("Received pruning point proof chunk #{}: level {}", chunk_count, level);
            }
            Some(Payload::PruningPointProofChunksEnd(_)) => {
                proof.headers.push(current_headers);
                // Chunks arrive from highest to lowest to allow on-the-fly proof validation in the future;
                // until then, we restore the proof's canonical lowest-to-highest level order before conversion.
                proof.headers.reverse();
                return Ok(proof.try_into()?);
            }
            payload => {
                return Err(ProtocolError::UnexpectedMessage(
                    "PruningPointProofChunk | PruningPointProofChunksEnd",
                    payload.as_ref().map(Into::into),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::header::Header;
    use kaspa_p2p_lib::{
        make_message,
        pb::{PruningPointProofChunkMessage, PruningPointProofChunksEndMessage},
    };
    use std::sync::Arc;
    use tokio::sync::mpsc;

    fn proof_with_levels(level_sizes: &[usize]) -> PruningPointProof {
        let mut nonce = 0;
        level_sizes
            .iter()
            .map(|&size| {
                (0..size)
                    .map(|_| {
                        let mut header = Header::from_precomputed_hash(Default::default(), vec![]);
                        header.nonce = nonce;
                        nonce += 1;
                        header.finalize();
                        Arc::new(header)
                    })
                    .collect()
            })
            .collect()
    }

    fn proof_chunks(proof: &PruningPointProof) -> impl Iterator<Item = PruningPointProofChunkMessage> + '_ {
        const TEST_CHUNK_SIZE: usize = 3;
        proof.iter().enumerate().rev().flat_map(|(level, headers)| {
            headers.chunks(TEST_CHUNK_SIZE).chain(headers.is_empty().then_some(&[][..])).map(move |headers| {
                PruningPointProofChunkMessage {
                    chunk: headers.iter().map(|header| header.as_ref().into()).collect(),
                    level: level as u32,
                }
            })
        })
    }

    #[tokio::test]
    async fn proof_roundtrip_preserves_levels_and_header_order() {
        for sizes in [&[0][..], &[0, 0], &[0, 101, 0], &[1], &[99], &[100], &[101], &[200], &[73, 0, 134, 2, 0]] {
            let proof = proof_with_levels(sizes);
            let chunks: Vec<_> = proof_chunks(&proof).collect();

            // A bounded channel exercises incremental receipt without buffering
            // the entire proof before the receiver runs.
            let (tx, rx) = mpsc::channel(1);
            let mut route = IncomingRoute::new(rx);
            let send = async move {
                for chunk in chunks {
                    tx.send(make_message!(Payload::PruningPointProofChunk, chunk)).await.unwrap();
                }
                tx.send(make_message!(Payload::PruningPointProofChunksEnd, PruningPointProofChunksEndMessage {})).await.unwrap();
            };
            let (_, received) = tokio::join!(send, receive_pruning_point_proof(&mut route, true));
            let received = received.unwrap();
            let as_wire = |proof: &PruningPointProof| PruningPointProofMessage { headers: proof.iter().map(Into::into).collect() };
            assert_eq!(as_wire(&received), as_wire(&proof));
        }
    }

    #[tokio::test]
    async fn legacy_proof_completes_without_end_message() {
        let proof = proof_with_levels(&[73, 0, 134, 2]);
        let message = PruningPointProofMessage { headers: proof.iter().map(Into::into).collect() };
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        tx.send(make_message!(Payload::PruningPointProof, message.clone())).await.unwrap();
        let received = receive_pruning_point_proof(&mut route, false).await.unwrap();
        assert_eq!(PruningPointProofMessage { headers: received.iter().map(Into::into).collect() }, message);
    }

    #[tokio::test]
    async fn proof_rejects_messages_for_the_other_protocol_version() {
        let proof = proof_with_levels(&[1]);
        let legacy =
            make_message!(Payload::PruningPointProof, PruningPointProofMessage { headers: proof.iter().map(Into::into).collect() });
        let chunk = make_message!(Payload::PruningPointProofChunk, proof_chunks(&proof).next().unwrap());
        for (use_pruning_proof_chunks, message) in [(true, legacy), (false, chunk)] {
            let (tx, rx) = mpsc::channel(1);
            let mut route = IncomingRoute::new(rx);
            tx.send(message).await.unwrap();
            assert!(matches!(
                receive_pruning_point_proof(&mut route, use_pruning_proof_chunks).await,
                Err(ProtocolError::UnexpectedMessage(..))
            ));
        }
    }

    #[tokio::test]
    async fn proof_waits_for_end_message() {
        let proof = proof_with_levels(&[3]);
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        tx.send(make_message!(Payload::PruningPointProofChunk, proof_chunks(&proof).next().unwrap())).await.unwrap();
        let receive = receive_pruning_point_proof(&mut route, true);
        tokio::pin!(receive);
        assert!(futures::poll!(&mut receive).is_pending());
        tx.send(make_message!(Payload::PruningPointProofChunksEnd, PruningPointProofChunksEndMessage {})).await.unwrap();
        assert_eq!(receive.await.unwrap()[0].len(), 3);
    }

    #[tokio::test]
    async fn proof_rejects_disconnect_before_end() {
        let proof = proof_with_levels(&[1]);
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        tx.send(make_message!(Payload::PruningPointProofChunk, proof_chunks(&proof).next().unwrap())).await.unwrap();
        drop(tx);
        assert!(matches!(receive_pruning_point_proof(&mut route, true).await, Err(ProtocolError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn proof_rejects_invalid_chunks() {
        let proof = proof_with_levels(&[1]);
        let chunk = proof_chunks(&proof).next().unwrap();
        let level_one = PruningPointProofChunkMessage { level: 1, ..chunk.clone() };
        let level_two = PruningPointProofChunkMessage { level: 2, ..chunk.clone() };
        for chunks in [
            vec![level_two.clone(), chunk.clone()],
            vec![level_two, level_one, chunk.clone(), PruningPointProofChunkMessage { level: 1, ..chunk.clone() }],
            vec![PruningPointProofChunkMessage { level: u32::MAX, ..chunk.clone() }],
        ] {
            let (tx, rx) = mpsc::channel(chunks.len());
            let mut route = IncomingRoute::new(rx);
            for chunk in chunks {
                tx.send(make_message!(Payload::PruningPointProofChunk, chunk)).await.unwrap();
            }
            assert!(matches!(receive_pruning_point_proof(&mut route, true).await, Err(ProtocolError::Other(_))));
        }
    }
}
