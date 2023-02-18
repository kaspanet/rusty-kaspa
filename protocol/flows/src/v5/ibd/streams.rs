//!
//! Logical stream abstractions used throughout the IBD negotiation protocols
//!

use consensus_core::header::Header;
use kaspa_core::{debug, info};
use p2p_lib::{
    common::ProtocolError,
    convert::model::trusted::TrustedDataEntry,
    make_message,
    pb::{kaspad_message::Payload, RequestNextHeadersMessage, RequestNextPruningPointAndItsAnticoneBlocksMessage},
    IncomingRoute, Router,
};
use std::sync::Arc;

const IBD_BATCH_SIZE: usize = 99;

pub struct TrustedEntryStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize,
}

impl<'a, 'b> TrustedEntryStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<TrustedDataEntry>, ProtocolError> {
        let msg = match tokio::time::timeout(p2p_lib::common::DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockWithTrustedDataV4(payload)) => Ok(Some(payload.try_into()?)),
                        Some(Payload::DoneBlocksWithTrustedData(_)) => {
                            debug!("trusted entry stream completed after {} items", self.i);
                            return Ok(None);
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockWithTrustedDataV4 | Payload::DoneBlocksWithTrustedData),
                            Box::new(msg.payload),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(p2p_lib::common::DEFAULT_TIMEOUT)),
        };

        // Request the next batch
        // TODO: test that batch counting is correct and follows golang imp
        self.i += 1;
        if self.i % IBD_BATCH_SIZE == 0 {
            info!("Downloaded {} blocks from the pruning point anticone", self.i - 1);
            self.router
                .enqueue(make_message!(
                    Payload::RequestNextPruningPointAndItsAnticoneBlocks,
                    RequestNextPruningPointAndItsAnticoneBlocksMessage {}
                ))
                .await?;
        }

        msg
    }
}

/// A chunk of headers
pub type HeadersChunk = Vec<Arc<Header>>;

pub struct HeadersChunkStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize,
}

impl<'a, 'b> HeadersChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<HeadersChunk>, ProtocolError> {
        let msg = match tokio::time::timeout(p2p_lib::common::DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockHeaders(payload)) => {
                            if payload.block_headers.is_empty() {
                                // The syncer should have sent a done message if the search completed, and not an empty list
                                return Err(ProtocolError::Other("Received an empty headers message"));
                            }
                            Ok(Some(payload.try_into()?))
                        }
                        Some(Payload::DoneHeaders(_)) => {
                            debug!("headers chunk stream completed after {} chunks", self.i);
                            return Ok(None);
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockHeaders | Payload::DoneHeaders),
                            Box::new(msg.payload),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(p2p_lib::common::DEFAULT_TIMEOUT)),
        };

        // Request the next chunk
        self.i += 1;
        self.router.enqueue(make_message!(Payload::RequestNextHeaders, RequestNextHeadersMessage {})).await?;

        msg
    }
}
