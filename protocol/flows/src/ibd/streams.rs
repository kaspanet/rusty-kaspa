//!
//! Logical stream abstractions used throughout the IBD negotiation protocols
//!

use kaspa_consensus_core::{
    errors::consensus::ConsensusError,
    header::Header,
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_core::{debug, info};
use kaspa_hashes::Hash;
use kaspa_p2p_lib::{
    IncomingRoute, Router,
    common::{DEFAULT_TIMEOUT, ProtocolError},
    convert::{header::HeaderFormat, header::Versioned, model::trusted::TrustedDataEntry},
    make_message,
    pb::{
        RequestNextHeadersMessage, RequestNextPruningPointAndItsAnticoneBlocksMessage, RequestNextPruningPointUtxoSetChunkMessage,
        kaspad_message::Payload,
    },
};
use std::sync::Arc;
use tokio::time::timeout;

pub const IBD_BATCH_SIZE: usize = 99;

pub struct TrustedEntryStream<'a, 'b> {
    router: &'a Router,
    incoming_route: &'b mut IncomingRoute,
    header_format: HeaderFormat,
    i: usize,
}

impl<'a, 'b> TrustedEntryStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute, header_format: HeaderFormat) -> Self {
        Self { router, incoming_route, header_format, i: 0 }
    }

    pub async fn next(&mut self) -> Result<Option<TrustedDataEntry>, ProtocolError> {
        let res = match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(op) => {
                if let Some(msg) = op {
                    match msg.payload {
                        Some(Payload::BlockWithTrustedDataV4(payload)) => {
                            let entry: TrustedDataEntry = Versioned(self.header_format, payload).try_into()?;
                            Ok(Some(entry))
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
            if self.i.is_multiple_of(IBD_BATCH_SIZE) {
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
    header_format: HeaderFormat,
    i: usize,
}

impl<'a, 'b> HeadersChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute, header_format: HeaderFormat) -> Self {
        Self { router, incoming_route, header_format, i: 0 }
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
                                Ok(Some(Versioned(self.header_format, payload).try_into()?))
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
}

impl<'a, 'b> PruningPointUtxosetChunkStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { router, incoming_route, i: 0, utxo_count: 0 }
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
            if self.i.is_multiple_of(IBD_BATCH_SIZE) {
                info!("Received {} UTXO set chunks so far, totaling in {} UTXOs", self.i, self.utxo_count);
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

/// Proof is required for the first lane and every 16th lane thereafter.
const SMT_PROOF_INTERVAL: usize = 16;

/// Stream of SMT lane entries. Pure one-way — no flow control.
/// Enforces that the first and every 16th entry carries proof bytes.
pub struct SmtStream<'a, 'b> {
    incoming_route: &'b mut IncomingRoute,
    _router: &'a Router,
    lane_count: usize,
}

impl<'a, 'b> SmtStream<'a, 'b> {
    pub fn new(router: &'a Router, incoming_route: &'b mut IncomingRoute) -> Self {
        Self { _router: router, incoming_route, lane_count: 0 }
    }

    pub async fn recv_metadata(&mut self) -> Result<kaspa_consensus_core::api::SmtExportMetadata, ProtocolError> {
        match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(Some(msg)) => match msg.payload {
                Some(Payload::SmtMetadata(payload)) => {
                    let (chunks, rem) = payload.data.as_chunks::<32>();
                    let &[lanes_root, context_hash, payload_root, parent_seq_commit] = chunks else {
                        return Err(ProtocolError::Other("SmtMetadata data must be exactly 128 bytes"));
                    };
                    if !rem.is_empty() {
                        return Err(ProtocolError::Other("SmtMetadata data must be exactly 128 bytes"));
                    }
                    let [lanes_root, context_hash, payload_root, parent_seq_commit] =
                        [lanes_root, context_hash, payload_root, parent_seq_commit].map(Hash::from_bytes);
                    Ok(kaspa_consensus_core::api::SmtExportMetadata { lanes_root, context_hash, payload_root, parent_seq_commit })
                }
                Some(Payload::UnexpectedPruningPoint(_)) => Err(ProtocolError::ConsensusError(ConsensusError::UnexpectedPruningPoint)),
                _ => Err(ProtocolError::UnexpectedMessage(stringify!(Payload::SmtMetadata), msg.payload.as_ref().map(|v| v.into()))),
            },
            Ok(None) => Err(ProtocolError::ConnectionClosed),
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        }
    }

    pub async fn next(&mut self) -> Result<Option<kaspa_consensus_core::api::ImportLane>, ProtocolError> {
        match timeout(DEFAULT_TIMEOUT, self.incoming_route.recv()).await {
            Ok(Some(msg)) => match msg.payload {
                Some(Payload::SmtLaneEntry(payload)) => {
                    let Some((&lane_id, rem)) = payload.data.split_first_chunk::<20>() else {
                        return Err(ProtocolError::Other("SmtLaneEntry data too short for lane_id"));
                    };
                    let Some(&tip_bytes) = rem.first_chunk::<32>() else {
                        return Err(ProtocolError::Other("SmtLaneEntry data too short for lane_tip"));
                    };
                    if rem.len() != 32 {
                        return Err(ProtocolError::Other("SmtLaneEntry data must be exactly 52 bytes"));
                    }
                    let lane_tip = Hash::from_bytes(tip_bytes);

                    let requires_proof = self.lane_count.is_multiple_of(SMT_PROOF_INTERVAL);
                    let proof = if requires_proof {
                        if payload.proof.is_empty() {
                            return Err(ProtocolError::Other("SMT proof required for first and every 16th lane entry"));
                        }
                        Some(
                            kaspa_smt::proof::OwnedSmtProof::from_bytes(&payload.proof)
                                .map_err(|e| ProtocolError::OtherOwned(format!("invalid SMT proof: {e}")))?,
                        )
                    } else {
                        None
                    };

                    self.lane_count += 1;
                    Ok(Some(kaspa_consensus_core::api::ImportLane { lane_id, lane_tip, blue_score: payload.blue_score, proof }))
                }
                Some(Payload::DoneSmtChunks(_)) => {
                    info!("Finished receiving SMT state. Total lanes: {}", self.lane_count);
                    Ok(None)
                }
                Some(Payload::UnexpectedPruningPoint(_)) => Err(ProtocolError::ConsensusError(ConsensusError::UnexpectedPruningPoint)),
                _ => Err(ProtocolError::UnexpectedMessage(
                    stringify!(Payload::SmtLaneEntry | Payload::DoneSmtChunks),
                    msg.payload.as_ref().map(|v| v.into()),
                )),
            },
            Ok(None) => Err(ProtocolError::ConnectionClosed),
            Err(_) => Err(ProtocolError::Timeout(DEFAULT_TIMEOUT)),
        }
    }

    pub fn lane_count(&self) -> usize {
        self.lane_count
    }
}
