use kaspa_consensus_core::{BlockLevel, pruning::PruningPointProof};
use kaspa_p2p_lib::{
    IncomingRoute,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    dequeue_with_timeout,
    pb::{PruningPointProofHeaderArray, PruningPointProofMessage, kaspad_message::Payload},
};
use log::info;
use prost::Message;
use std::time::{Duration, Instant};

const MAX_PRUNING_POINT_PROOF_SIZE: usize = 1024 * 1024 * 1024;
const MAX_PRUNING_POINT_PROOF_RECEIVE_TIME: Duration = Duration::from_secs(900);

fn add_pruning_point_proof_chunk_size(cumulative_size: &mut usize, chunk_size: usize) -> Result<(), ProtocolError> {
    let new_size = cumulative_size
        .checked_add(chunk_size)
        .filter(|size| *size < MAX_PRUNING_POINT_PROOF_SIZE)
        .ok_or(ProtocolError::Other("Cumulative pruning point proof chunk size must be less than 1 GiB"))?;
    *cumulative_size = new_size;
    Ok(())
}

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
    let mut cumulative_size = 0;
    let started_at = Instant::now();
    // Proof generation can take several minutes, so we start with a long timeout and reset it to the default after the first chunk is received.
    let mut timeout = Duration::from_secs(600);
    for chunk_count in 1u64.. {
        let msg = tokio::time::timeout(timeout, incoming_route.recv())
            .await
            .map_err(|_| ProtocolError::Timeout(timeout))?
            .ok_or(ProtocolError::ConnectionClosed)?;
        match msg.payload {
            Some(Payload::PruningPointProofChunk(chunk)) => {
                timeout = DEFAULT_TIMEOUT;

                if started_at.elapsed() > MAX_PRUNING_POINT_PROOF_RECEIVE_TIME {
                    return Err(ProtocolError::Timeout(MAX_PRUNING_POINT_PROOF_RECEIVE_TIME));
                }

                if chunk.chunk.is_empty() {
                    return Err(ProtocolError::Other("Received an empty pruning point proof chunk"));
                }
                add_pruning_point_proof_chunk_size(&mut cumulative_size, chunk.encoded_len())?;
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
    unreachable!()
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects)]
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
            headers.chunks(TEST_CHUNK_SIZE).map(move |headers| PruningPointProofChunkMessage {
                chunk: headers.iter().map(|header| header.as_ref().into()).collect(),
                level: level as u32,
            })
        })
    }

    #[tokio::test]
    async fn proof_roundtrip_preserves_levels_and_header_order() {
        for sizes in [&[1][..], &[1, 101, 1], &[99], &[100], &[101], &[200], &[73, 4, 134, 2, 5]] {
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
            vec![PruningPointProofChunkMessage { chunk: vec![], level: 0 }],
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

    #[test]
    fn proof_rejects_cumulative_chunk_size_of_one_gib() {
        let mut cumulative_size = MAX_PRUNING_POINT_PROOF_SIZE - 2;
        add_pruning_point_proof_chunk_size(&mut cumulative_size, 1).unwrap();
        assert_eq!(cumulative_size, MAX_PRUNING_POINT_PROOF_SIZE - 1);
        assert!(add_pruning_point_proof_chunk_size(&mut cumulative_size, 1).is_err());
    }
}
