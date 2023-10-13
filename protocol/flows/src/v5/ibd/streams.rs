//!
//! Logical stream abstractions used throughout the IBD negotiation protocols
//!

use crate::flow_context::FlowContext;
use kaspa_consensus_core::{
    errors::consensus::ConsensusError,
    header::Header,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_consensus_notify::notification::{Notification, SyncStateChangedNotification};
use kaspa_core::{debug, info};
use kaspa_notify::notifier::Notify;
use kaspa_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    convert::model::trusted::TrustedDataEntry,
    make_message,
    pb::{
        kaspad_message::Payload, RequestNextHeadersMessage, RequestNextPruningPointAndItsAnticoneBlocksMessage,
        RequestNextPruningPointUtxoSetChunkMessage,
    },
    IncomingRoute, Router,
};
use std::sync::Arc;
use tokio::time::timeout;

pub const IBD_BATCH_SIZE: usize = 99;

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
        let res = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockWithTrustedDataV4(payload)) => Ok(Some(payload.try_into()?)),
                        Some(Payload::DoneBlocksWithTrustedData(_)) => {
                            debug!("trusted entry stream completed after {} items", self.i);
                            Ok(None)
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockWithTrustedDataV4 | Payload::DoneBlocksWithTrustedData),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(_)) = res {
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
        }

        res
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
        let res = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockHeaders(payload)) => {
                            if payload.block_headers.is_empty() {
                                // The syncer should have sent a done message if the search completed, and not an empty list
                                Err(ProtocolError::Other("Received an empty headers message"))
                            } else {
                                Ok(Some(payload.try_into()?))
                            }
                        }
                        Some(Payload::DoneHeaders(_)) => {
                            debug!("headers chunk stream completed after {} chunks", self.i);
                            Ok(None)
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(Payload::BlockHeaders | Payload::DoneHeaders),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(_)) = res {
            self.i += 1;
            self.router.enqueue(make_message!(Payload::RequestNextHeaders, RequestNextHeadersMessage {})).await?;
        }

        res
    }
}

/// A chunk of UTXOs
pub type UtxosetChunk = Vec<(TransactionOutpoint, UtxoEntry)>;

pub struct PruningPointUtxosetChunkStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    i: usize, // Chunk index
    utxo_count: usize,
    ctx: &'a FlowContext,
}

impl<'a, 'b> PruningPointUtxosetChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute, ctx: &'a FlowContext) -> Self {
        Self { router, incoming_route, i: 0, utxo_count: 0, ctx }
    }

    pub async fn next(&mut self) -> Result<Option<UtxosetChunk>, ProtocolError> {
        let res: Result<Option<UtxosetChunk>, ProtocolError> = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::PruningPointUtxoSetChunk(payload)) => Ok(Some(payload.try_into()?)),
                        Some(Payload::DonePruningPointUtxoSetChunks(_)) => {
                            info!("Finished receiving the UTXO set. Total UTXOs: {}", self.utxo_count);
                            Ok(None)
                        }
                        Some(Payload::UnexpectedPruningPoint(_)) => {
                            // Although this can happen also to an honest syncer (if his pruning point moves during the sync),
                            // we prefer erring and disconnecting to avoid possible exploits by a syncer repeating this failure
                            Err(ProtocolError::ConsensusError(ConsensusError::UnexpectedPruningPoint))
                        }
                        _ => Err(ProtocolError::UnexpectedMessage(
                            stringify!(
                                Payload::PruningPointUtxoSetChunk
                                    | Payload::DonePruningPointUtxoSetChunks
                                    | Payload::UnexpectedPruningPoint
                            ),
                            msg.payload.as_ref().map(|v| v.into()),
                        )),
                    }
                } else {
                    Err(ProtocolError::ConnectionClosed)
                }
            }
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        };

        // Request the next batch only if the stream is still live
        if let Ok(Some(chunk)) = res {
            self.i += 1;
            self.utxo_count += chunk.len();
            if self.i % IBD_BATCH_SIZE == 0 {
                info!("Received {} UTXO set chunks so far, totaling in {} UTXOs", self.i, self.utxo_count);
                if !self.ctx.consensus_manager.consensus().session().await.async_is_nearly_synced().await {
                    self.ctx
                        .notification_root
                        .notify(Notification::SyncStateChanged(SyncStateChangedNotification::new_utxo_sync(
                            self.i as u64,
                            self.utxo_count as u64,
                        )))
                        .expect("expecting an open unbounded channel");
                }
                self.router
                    .enqueue(make_message!(
                        Payload::RequestNextPruningPointUtxoSetChunk,
                        RequestNextPruningPointUtxoSetChunkMessage {}
                    ))
                    .await?;
            }
            Ok(Some(chunk))
        } else {
            res
        }
    }
}
