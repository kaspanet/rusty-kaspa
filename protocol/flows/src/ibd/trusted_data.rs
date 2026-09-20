use kaspa_p2p_lib::{
    IncomingRoute,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    convert::model::trusted::TrustedDataPackage,
    dequeue_with_timeout,
    pb::kaspad_message::Payload,
};
use log::info;
use prost::Message;

const MAX_TRUSTED_DATA_SIZE: usize = 1024 * 1024 * 1024;

fn add_trusted_data_chunk_size(cumulative_size: &mut usize, chunk_size: usize) -> Result<(), ProtocolError> {
    let new_size = cumulative_size
        .checked_add(chunk_size)
        .filter(|size| *size < MAX_TRUSTED_DATA_SIZE)
        .ok_or(ProtocolError::Other("Cumulative trusted data chunk size must be less than 1 GiB"))?;
    *cumulative_size = new_size;
    Ok(())
}

pub(crate) async fn receive_trusted_data(
    incoming_route: &mut IncomingRoute,
    use_trusted_data_chunks: bool,
) -> Result<TrustedDataPackage, ProtocolError> {
    if !use_trusted_data_chunks {
        let msg = dequeue_with_timeout!(incoming_route, Payload::TrustedData)?;
        return Ok(msg.try_into()?);
    }

    let mut pkg = TrustedDataPackage::new(Vec::new(), Vec::new());
    let mut chunk_count = 0;
    let mut cumulative_size = 0;
    loop {
        let msg = tokio::time::timeout(DEFAULT_TIMEOUT, incoming_route.recv())
            .await
            .map_err(|_| ProtocolError::Timeout(DEFAULT_TIMEOUT))?
            .ok_or(ProtocolError::ConnectionClosed)?;
        match msg.payload {
            Some(Payload::TrustedDataChunk(chunk)) => {
                if chunk.headers.is_empty() {
                    return Err(ProtocolError::Other("Received an empty trusted data chunk"));
                }
                add_trusted_data_chunk_size(&mut cumulative_size, chunk.encoded_len())?;
                chunk_count += 1;
                info!("Received trusted data chunk #{}: {} DAA blocks", chunk_count, chunk.headers.len());
                for header in chunk.headers {
                    pkg.daa_window.push(header.try_into()?);
                }
            }
            Some(Payload::TrustedDataChunksEnd(_)) => return Ok(pkg),
            payload => {
                return Err(ProtocolError::UnexpectedMessage(
                    "TrustedDataChunk | TrustedDataChunksEnd",
                    payload.as_ref().map(Into::into),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_p2p_lib::{make_message, pb};
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn trusted_data_waits_for_end_and_leaves_following_message() {
        let (tx, rx) = mpsc::channel(2);
        let mut route = IncomingRoute::new(rx);
        let pkg = {
            let receive = receive_trusted_data(&mut route, true);
            tokio::pin!(receive);
            assert!(futures::poll!(&mut receive).is_pending());
            tx.send(make_message!(Payload::TrustedDataChunksEnd, pb::TrustedDataChunksEndMessage {})).await.unwrap();
            tx.send(make_message!(Payload::DoneBlocksWithTrustedData, pb::DoneBlocksWithTrustedDataMessage {})).await.unwrap();
            receive.await.unwrap()
        };
        assert!(pkg.daa_window.is_empty());
        assert!(pkg.ghostdag_window.is_empty());
        assert!(matches!(route.recv().await.unwrap().payload, Some(Payload::DoneBlocksWithTrustedData(_))));
    }

    #[tokio::test]
    async fn trusted_data_rejects_disconnect_without_end() {
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        drop(tx);
        assert!(matches!(receive_trusted_data(&mut route, true).await, Err(ProtocolError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn trusted_data_rejects_empty_chunk() {
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        tx.send(make_message!(Payload::TrustedDataChunk, pb::TrustedDataChunkMessage { headers: vec![] })).await.unwrap();
        assert!(matches!(receive_trusted_data(&mut route, true).await, Err(ProtocolError::Other(_))));
    }

    #[tokio::test]
    async fn trusted_data_rejects_wrong_protocol_and_premature_blocks() {
        for (chunked, msg) in [
            (true, make_message!(Payload::TrustedData, pb::TrustedDataMessage::default())),
            (false, make_message!(Payload::TrustedDataChunk, pb::TrustedDataChunkMessage::default())),
            (true, make_message!(Payload::BlockWithTrustedDataV4, pb::BlockWithTrustedDataV4Message::default())),
        ] {
            let (tx, rx) = mpsc::channel(1);
            let mut route = IncomingRoute::new(rx);
            tx.send(msg).await.unwrap();
            assert!(matches!(receive_trusted_data(&mut route, chunked).await, Err(ProtocolError::UnexpectedMessage(..))));
        }
    }

    #[tokio::test]
    async fn trusted_data_rejects_malformed_daa_entry() {
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        // A DAA entry must contain both a header and GHOSTDAG metadata.
        tx.send(make_message!(Payload::TrustedDataChunk, pb::TrustedDataChunkMessage { headers: vec![pb::DaaBlockV4::default()] }))
            .await
            .unwrap();
        assert!(matches!(receive_trusted_data(&mut route, true).await, Err(ProtocolError::ConversionError(_))));
    }

    #[tokio::test]
    async fn legacy_trusted_data_preserves_standalone_ghostdag_without_end() {
        let ghostdag =
            pb::GhostdagData { selected_parent: Some(kaspa_hashes::Hash::from(1u64).into()), blue_score: 123, ..Default::default() };
        let pair = pb::BlockGhostdagDataHashPair { hash: Some(kaspa_hashes::Hash::from(2u64).into()), ghostdag_data: Some(ghostdag) };
        let (tx, rx) = mpsc::channel(1);
        let mut route = IncomingRoute::new(rx);
        tx.send(make_message!(Payload::TrustedData, pb::TrustedDataMessage { daa_window: vec![], ghostdag_data: vec![pair.clone()] }))
            .await
            .unwrap();
        let pkg = receive_trusted_data(&mut route, false).await.unwrap();
        assert_eq!(pkg.ghostdag_window.len(), 1);
        assert_eq!(pb::BlockGhostdagDataHashPair::from(&pkg.ghostdag_window[0]), pair);
    }

    #[test]
    fn trusted_data_rejects_cumulative_chunk_size_of_one_gib() {
        let mut cumulative_size = MAX_TRUSTED_DATA_SIZE - 2;
        add_trusted_data_chunk_size(&mut cumulative_size, 1).unwrap();
        assert_eq!(cumulative_size, MAX_TRUSTED_DATA_SIZE - 1);
        assert!(add_trusted_data_chunk_size(&mut cumulative_size, 1).is_err());
    }
}
