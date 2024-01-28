//!
//! Logical stream abstractions used throughout the IBD negotiation protocols
//!

use kaspa_consensus_core::{
    errors::consensus::ConsensusError,
    header::Header,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_core::{debug, info};
use kaspa_p2p_lib::{
    common::{ProtocolError, DEFAULT_TIMEOUT},
    convert::{error::ConversionError, model::trusted::TrustedDataEntry},
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
                        Some(Payload::BlockWithTrustedDataV4(payload)) => {
                            let entry: TrustedDataEntry = payload.try_into()?;
                            if entry.block.is_header_only() {
                                Err(ProtocolError::OtherOwned(format!("trusted entry block {} is header only", entry.block.hash())))
                            } else {
                                Ok(Some(entry))
                            }
                        }
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
    signaled_utxoset_size: usize,
}

impl<'a, 'b> PruningPointUtxosetChunkStream<'a, 'b> {
    pub const IDENT: &'static str = "PruningPointUtxosetChunkStream";

    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0, utxo_count: 0, signaled_utxoset_size: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<(UtxosetChunk, usize)>, ProtocolError> {
        let res: Result<Option<(UtxosetChunk, usize)>, ProtocolError> =
            match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
                Ok(op) => {
                    if let Some(msg) = op {
                        match msg.payload {
                            Some(Payload::PruningPointUtxoSetChunk(payload)) => {
                                Ok(Some(payload.try_into().map_err(|_| ConversionError::General)?))
                            }
                            Some(Payload::DonePruningPointUtxoSetChunks(_)) => {
                                info!("[{0}] Finished receiving the UTXO set. Total UTXOs: {1}", Self::IDENT, self.utxo_count);
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
            self.utxo_count += chunk.0.len();
            if self.i == 1 && chunk.1 > 0 {
                // We expect a signaled set size only in first chunk, and if `chunk.1 == 0`, we are probably ibding from a node without this feature.
                info!("[{0}]: Start Streaming of pruning point Utxo set; signaled set size: {1}", Self::IDENT, chunk.1);
                self.signaled_utxoset_size = chunk.1;
            }
            if self.i % IBD_BATCH_SIZE == 0 || self.utxo_count == self.signaled_utxoset_size {
                info!(
                    "[{0}]: Received {1} + {2} / {3} signaled UTXOs ({4:.0}%)",
                    Self::IDENT,
                    self.utxo_count,
                    chunk.0.len(),
                    if self.signaled_utxoset_size > 0 { self.signaled_utxoset_size.to_string() } else { f64::NAN.to_string() },
                    if self.signaled_utxoset_size > 0 {
                        (self.utxo_count as f64 / self.signaled_utxoset_size as f64) * 100.0
                    } else {
                        f64::NAN
                    }
                );
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
