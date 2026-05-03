use crate::model::*;
use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_consensus_core::api::stats::BlockCount;
use kaspa_core::debug;
use kaspa_notify::subscription::{Command, context::SubscriptionContext, single::UtxosChangedSubscription};
use kaspa_utils::hex::ToHex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::{
    fmt::{Display, Formatter},
    str::FromStr,
    sync::Arc,
};
use workflow_serializer::prelude::*;

pub type RpcExtraData = Vec<u8>;

/// SubmitBlockRequest requests to submit a block into the DAG.
/// Blocks are generally expected to have been generated using the getBlockTemplate call.
///
/// See: [`GetBlockTemplateRequest`]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockRequest {
    pub block: RpcRawBlock,
    #[serde(alias = "allowNonDAABlocks")]
    pub allow_non_daa_blocks: bool,
}
impl SubmitBlockRequest {
    pub fn new(block: RpcRawBlock, allow_non_daa_blocks: bool) -> Self {
        Self { block, allow_non_daa_blocks }
    }
}

impl Serializer for SubmitBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcRawBlock, &self.block, writer)?;
        store!(bool, &self.allow_non_daa_blocks, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcRawBlock, reader)?;
        let allow_non_daa_blocks = load!(bool, reader)?;

        Ok(Self { block, allow_non_daa_blocks })
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum SubmitBlockRejectReason {
    BlockInvalid = 1,
    IsInIBD = 2,
    RouteIsFull = 3,
}
impl SubmitBlockRejectReason {
    fn as_str(&self) -> &'static str {
        // see app\appmessage\rpc_submit_block.go, line 35
        match self {
            SubmitBlockRejectReason::BlockInvalid => "block is invalid",
            SubmitBlockRejectReason::IsInIBD => "node is not synced",
            SubmitBlockRejectReason::RouteIsFull => "route is full",
        }
    }
}
impl Display for SubmitBlockRejectReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Eq, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type", content = "reason")]
#[borsh(use_discriminant = true)]
pub enum SubmitBlockReport {
    Success,
    Reject(SubmitBlockRejectReason),
}
impl SubmitBlockReport {
    pub fn is_success(&self) -> bool {
        *self == SubmitBlockReport::Success
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitBlockResponse {
    pub report: SubmitBlockReport,
}

impl Serializer for SubmitBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(SubmitBlockReport, &self.report, writer)?;
        Ok(())
    }
}

impl Deserializer for SubmitBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let report = load!(SubmitBlockReport, reader)?;

        Ok(Self { report })
    }
}

/// GetBlockTemplateRequest requests a current block template.
/// Callers are expected to solve the block template and submit it using the submitBlock call
///
/// See: [`SubmitBlockRequest`]
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateRequest {
    /// Which kaspa address should the coinbase block reward transaction pay into
    pub pay_address: RpcAddress,
    // TODO: replace with hex serialization
    pub extra_data: RpcExtraData,
}
impl GetBlockTemplateRequest {
    pub fn new(pay_address: RpcAddress, extra_data: RpcExtraData) -> Self {
        Self { pay_address, extra_data }
    }
}

impl Serializer for GetBlockTemplateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.pay_address, writer)?;
        store!(RpcExtraData, &self.extra_data, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockTemplateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let pay_address = load!(RpcAddress, reader)?;
        let extra_data = load!(RpcExtraData, reader)?;

        Ok(Self { pay_address, extra_data })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockTemplateResponse {
    pub block: RpcRawBlock,

    /// Whether kaspad thinks that it's synced.
    /// Callers are discouraged (but not forbidden) from solving blocks when kaspad is not synced.
    /// That is because when kaspad isn't in sync with the rest of the network there's a high
    /// chance the block will never be accepted, thus the solving effort would have been wasted.
    pub is_synced: bool,
}

impl Serializer for GetBlockTemplateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcRawBlock, &self.block, writer)?;
        store!(bool, &self.is_synced, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockTemplateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcRawBlock, reader)?;
        let is_synced = load!(bool, reader)?;

        Ok(Self { block, is_synced })
    }
}

/// GetBlockRequest requests information about a specific block
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockRequest {
    /// The hash of the requested block
    pub hash: RpcHash,

    /// Whether to include transaction data in the response
    pub include_transactions: bool,
}
impl GetBlockRequest {
    pub fn new(hash: RpcHash, include_transactions: bool) -> Self {
        Self { hash, include_transactions }
    }
}

impl Serializer for GetBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        store!(bool, &self.include_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;
        let include_transactions = load!(bool, reader)?;

        Ok(Self { hash, include_transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockResponse {
    pub block: RpcBlock,
}

impl Serializer for GetBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcBlock, &self.block, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcBlock, reader)?;

        Ok(Self { block })
    }
}

/// GetInfoRequest returns info about the node.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoRequest {}

impl Serializer for GetInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetInfoResponse {
    pub p2p_id: String,
    pub mempool_size: u64,
    pub server_version: String,
    pub is_utxo_indexed: bool,
    pub is_synced: bool,
    pub has_notify_command: bool,
    pub has_message_id: bool,
}

impl Serializer for GetInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(String, &self.p2p_id, writer)?;
        store!(u64, &self.mempool_size, writer)?;
        store!(String, &self.server_version, writer)?;
        store!(bool, &self.is_utxo_indexed, writer)?;
        store!(bool, &self.is_synced, writer)?;
        store!(bool, &self.has_notify_command, writer)?;
        store!(bool, &self.has_message_id, writer)?;

        Ok(())
    }
}

impl Deserializer for GetInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let p2p_id = load!(String, reader)?;
        let mempool_size = load!(u64, reader)?;
        let server_version = load!(String, reader)?;
        let is_utxo_indexed = load!(bool, reader)?;
        let is_synced = load!(bool, reader)?;
        let has_notify_command = load!(bool, reader)?;
        let has_message_id = load!(bool, reader)?;

        Ok(Self { p2p_id, mempool_size, server_version, is_utxo_indexed, is_synced, has_notify_command, has_message_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkRequest {}

impl Serializer for GetCurrentNetworkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCurrentNetworkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentNetworkResponse {
    pub network: RpcNetworkType,
}

impl GetCurrentNetworkResponse {
    pub fn new(network: RpcNetworkType) -> Self {
        Self { network }
    }
}

impl Serializer for GetCurrentNetworkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcNetworkType, &self.network, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCurrentNetworkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network = load!(RpcNetworkType, reader)?;
        Ok(Self { network })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesRequest {}

impl Serializer for GetPeerAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPeerAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPeerAddressesResponse {
    pub known_addresses: Vec<RpcPeerAddress>,
    pub banned_addresses: Vec<RpcIpAddress>,
}

impl GetPeerAddressesResponse {
    pub fn new(known_addresses: Vec<RpcPeerAddress>, banned_addresses: Vec<RpcIpAddress>) -> Self {
        Self { known_addresses, banned_addresses }
    }
}

impl Serializer for GetPeerAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcPeerAddress>, &self.known_addresses, writer)?;
        store!(Vec<RpcIpAddress>, &self.banned_addresses, writer)?;
        Ok(())
    }
}

impl Deserializer for GetPeerAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let known_addresses = load!(Vec<RpcPeerAddress>, reader)?;
        let banned_addresses = load!(Vec<RpcIpAddress>, reader)?;
        Ok(Self { known_addresses, banned_addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkRequest {}

impl Serializer for GetSinkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkResponse {
    pub sink: RpcHash,
}

impl GetSinkResponse {
    pub fn new(selected_tip_hash: RpcHash) -> Self {
        Self { sink: selected_tip_hash }
    }
}

impl Serializer for GetSinkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.sink, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let sink = load!(RpcHash, reader)?;
        Ok(Self { sink })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryRequest {
    pub transaction_id: RpcTransactionId,
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntryRequest {
    pub fn new(transaction_id: RpcTransactionId, include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { transaction_id, include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntryRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntryRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { transaction_id, include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntryResponse {
    pub mempool_entry: RpcMempoolEntry,
}

impl GetMempoolEntryResponse {
    pub fn new(mempool_entry: RpcMempoolEntry) -> Self {
        Self { mempool_entry }
    }
}

impl Serializer for GetMempoolEntryResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcMempoolEntry, &self.mempool_entry, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMempoolEntryResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let mempool_entry = deserialize!(RpcMempoolEntry, reader)?;
        Ok(Self { mempool_entry })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesRequest {
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntriesRequest {
    pub fn new(include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntriesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesResponse {
    pub mempool_entries: Vec<RpcMempoolEntry>,
}

impl GetMempoolEntriesResponse {
    pub fn new(mempool_entries: Vec<RpcMempoolEntry>) -> Self {
        Self { mempool_entries }
    }
}

impl Serializer for GetMempoolEntriesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcMempoolEntry>, &self.mempool_entries, writer)?;
        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let mempool_entries = deserialize!(Vec<RpcMempoolEntry>, reader)?;
        Ok(Self { mempool_entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoRequest {}

impl Serializer for GetConnectedPeerInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectedPeerInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectedPeerInfoResponse {
    pub peer_info: Vec<RpcPeerInfo>,
}

impl GetConnectedPeerInfoResponse {
    pub fn new(peer_info: Vec<RpcPeerInfo>) -> Self {
        Self { peer_info }
    }
}

impl Serializer for GetConnectedPeerInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcPeerInfo>, &self.peer_info, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectedPeerInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let peer_info = load!(Vec<RpcPeerInfo>, reader)?;
        Ok(Self { peer_info })
    }
}

/// Borsh wire-version dispatch for [`AddPeerRequest`].
///
/// v1: struct `(RpcContextualPeerAddress, bool)` -- the historical
/// IP-only layout emitted by every previously-shipped wRPC client.
/// v2: struct `(String, bool)` where the string is the canonical
/// [`PeerEndpoint::Display`] form (accepting hostnames).
///
/// Wire-format compatibility rule (mandatory; required for rolling
/// upgrades across mixed-version clusters):
///
/// - `Address` payload (IP literal) MUST serialize as v1, byte-identical
///   to the historical wire frame. A v1 server still in the cluster
///   accepts the request unchanged; a new server's deserializer
///   recognises v1 and upgrades the payload into the
///   [`RpcPeerEndpoint::Address`] variant.
/// - `Hostname` payload MUST serialize as v2. A v1 server cannot decode
///   v2 frames -- but a hostname-using client requires a new server
///   anyway (the server's connection manager owns the resolution loop),
///   so this asymmetry is the documented contract.
///
/// The server-side deserializer accepts both versions and dispatches
/// on the version tag, so an old client (v1-only emit) remains
/// compatible with a new server.
const ADD_PEER_REQUEST_BORSH_V1: u16 = 1;
const ADD_PEER_REQUEST_BORSH_V2: u16 = 2;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerRequest {
    pub peer_address: RpcPeerEndpoint,
    pub is_permanent: bool,
}

impl AddPeerRequest {
    pub fn new(peer_address: RpcPeerEndpoint, is_permanent: bool) -> Self {
        Self { peer_address, is_permanent }
    }
}

impl Serializer for AddPeerRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        match &self.peer_address {
            RpcPeerEndpoint::Address(addr) => {
                store!(u16, &ADD_PEER_REQUEST_BORSH_V1, writer)?;
                store!(RpcContextualPeerAddress, addr, writer)?;
            }
            RpcPeerEndpoint::Hostname { .. } => {
                store!(u16, &ADD_PEER_REQUEST_BORSH_V2, writer)?;
                store!(String, &self.peer_address.to_string(), writer)?;
            }
        }
        store!(bool, &self.is_permanent, writer)?;
        Ok(())
    }
}

impl Deserializer for AddPeerRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let peer_address = match version {
            ADD_PEER_REQUEST_BORSH_V1 => {
                let legacy = load!(RpcContextualPeerAddress, reader)?;
                RpcPeerEndpoint::Address(legacy)
            }
            ADD_PEER_REQUEST_BORSH_V2 => {
                let s = load!(String, reader)?;
                RpcPeerEndpoint::from_str(&s).map_err(|e| {
                    // Wrap the structured `RpcError::InvalidPeerEndpoint`
                    // as the io::Error source so a downstream consumer
                    // that downcasts the wRPC borsh deserialization error
                    // can recover the typed variant -- matching the gRPC
                    // edge's `try_from!` mapping. The Display message
                    // (`invalid peer endpoint `{s}`: {e}`) is preserved
                    // verbatim through the thiserror `#[error(...)]`
                    // attribute on `RpcError::InvalidPeerEndpoint`.
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        crate::error::RpcError::invalid_peer_endpoint(s.clone(), e.to_string()),
                    )
                })?
            }
            other => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Unknown AddPeerRequest borsh version: {other}"),
                ));
            }
        };
        let is_permanent = load!(bool, reader)?;
        Ok(Self { peer_address, is_permanent })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddPeerResponse {}

impl Serializer for AddPeerResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for AddPeerResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionRequest {
    pub transaction: RpcTransaction,
    pub allow_orphan: bool,
}

impl SubmitTransactionRequest {
    pub fn new(transaction: RpcTransaction, allow_orphan: bool) -> Self {
        Self { transaction, allow_orphan }
    }
}

impl Serializer for SubmitTransactionRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcTransaction, &self.transaction, writer)?;
        store!(bool, &self.allow_orphan, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction = deserialize!(RpcTransaction, reader)?;
        let allow_orphan = load!(bool, reader)?;

        Ok(Self { transaction, allow_orphan })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionResponse {
    pub transaction_id: RpcTransactionId,
}

impl SubmitTransactionResponse {
    pub fn new(transaction_id: RpcTransactionId) -> Self {
        Self { transaction_id }
    }
}

impl Serializer for SubmitTransactionResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;

        Ok(Self { transaction_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionReplacementRequest {
    pub transaction: RpcTransaction,
}

impl SubmitTransactionReplacementRequest {
    pub fn new(transaction: RpcTransaction) -> Self {
        Self { transaction }
    }
}

impl Serializer for SubmitTransactionReplacementRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcTransaction, &self.transaction, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionReplacementRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction = deserialize!(RpcTransaction, reader)?;

        Ok(Self { transaction })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubmitTransactionReplacementResponse {
    pub transaction_id: RpcTransactionId,
    pub replaced_transaction: RpcTransaction,
}

impl SubmitTransactionReplacementResponse {
    pub fn new(transaction_id: RpcTransactionId, replaced_transaction: RpcTransaction) -> Self {
        Self { transaction_id, replaced_transaction }
    }
}

impl Serializer for SubmitTransactionReplacementResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcTransactionId, &self.transaction_id, writer)?;
        serialize!(RpcTransaction, &self.replaced_transaction, writer)?;

        Ok(())
    }
}

impl Deserializer for SubmitTransactionReplacementResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let transaction_id = load!(RpcTransactionId, reader)?;
        let replaced_transaction = deserialize!(RpcTransaction, reader)?;

        Ok(Self { transaction_id, replaced_transaction })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkRequest {
    pub subnetwork_id: RpcSubnetworkId,
}

impl GetSubnetworkRequest {
    pub fn new(subnetwork_id: RpcSubnetworkId) -> Self {
        Self { subnetwork_id }
    }
}

impl Serializer for GetSubnetworkRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcSubnetworkId, &self.subnetwork_id, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSubnetworkRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let subnetwork_id = load!(RpcSubnetworkId, reader)?;

        Ok(Self { subnetwork_id })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSubnetworkResponse {
    pub gas_limit: u64,
}

impl GetSubnetworkResponse {
    pub fn new(gas_limit: u64) -> Self {
        Self { gas_limit }
    }
}

impl Serializer for GetSubnetworkResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.gas_limit, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSubnetworkResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let gas_limit = load!(u64, reader)?;

        Ok(Self { gas_limit })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockRequest {
    pub start_hash: RpcHash,
    pub include_accepted_transaction_ids: bool,
    pub min_confirmation_count: Option<u64>,
}

impl GetVirtualChainFromBlockRequest {
    pub fn new(start_hash: RpcHash, include_accepted_transaction_ids: bool, min_confirmation_count: Option<u64>) -> Self {
        Self { start_hash, include_accepted_transaction_ids, min_confirmation_count }
    }
}

impl Serializer for GetVirtualChainFromBlockRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        store!(bool, &self.include_accepted_transaction_ids, writer)?;
        store!(Option<u64>, &self.min_confirmation_count, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let include_accepted_transaction_ids = load!(bool, reader)?;

        let min_confirmation_count = if version > 1 { load!(Option<u64>, reader)? } else { None };

        Ok(Self { start_hash, include_accepted_transaction_ids, min_confirmation_count })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockResponse {
    pub removed_chain_block_hashes: Vec<RpcHash>,
    pub added_chain_block_hashes: Vec<RpcHash>,
    pub accepted_transaction_ids: Vec<RpcAcceptedTransactionIds>,
}

impl GetVirtualChainFromBlockResponse {
    pub fn new(
        removed_chain_block_hashes: Vec<RpcHash>,
        added_chain_block_hashes: Vec<RpcHash>,
        accepted_transaction_ids: Vec<RpcAcceptedTransactionIds>,
    ) -> Self {
        Self { removed_chain_block_hashes, added_chain_block_hashes, accepted_transaction_ids }
    }
}

impl Serializer for GetVirtualChainFromBlockResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        store!(Vec<RpcAcceptedTransactionIds>, &self.accepted_transaction_ids, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let accepted_transaction_ids = load!(Vec<RpcAcceptedTransactionIds>, reader)?;

        Ok(Self { removed_chain_block_hashes, added_chain_block_hashes, accepted_transaction_ids })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksRequest {
    pub low_hash: Option<RpcHash>,
    pub include_blocks: bool,
    pub include_transactions: bool,
}

impl GetBlocksRequest {
    pub fn new(low_hash: Option<RpcHash>, include_blocks: bool, include_transactions: bool) -> Self {
        Self { low_hash, include_blocks, include_transactions }
    }
}

impl Serializer for GetBlocksRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Option<RpcHash>, &self.low_hash, writer)?;
        store!(bool, &self.include_blocks, writer)?;
        store!(bool, &self.include_transactions, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlocksRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let low_hash = load!(Option<RpcHash>, reader)?;
        let include_blocks = load!(bool, reader)?;
        let include_transactions = load!(bool, reader)?;

        Ok(Self { low_hash, include_blocks, include_transactions })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlocksResponse {
    pub block_hashes: Vec<RpcHash>,
    pub blocks: Vec<RpcBlock>,
}

impl GetBlocksResponse {
    pub fn new(block_hashes: Vec<RpcHash>, blocks: Vec<RpcBlock>) -> Self {
        Self { block_hashes, blocks }
    }
}

impl Serializer for GetBlocksResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.block_hashes, writer)?;
        serialize!(Vec<RpcBlock>, &self.blocks, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlocksResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block_hashes = load!(Vec<RpcHash>, reader)?;
        let blocks = deserialize!(Vec<RpcBlock>, reader)?;

        Ok(Self { block_hashes, blocks })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockCountRequest {}

impl Serializer for GetBlockCountRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockCountRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

pub type GetBlockCountResponse = BlockCount;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoRequest {}

impl Serializer for GetBlockDagInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockDagInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockDagInfoResponse {
    pub network: RpcNetworkId,
    pub block_count: u64,
    pub header_count: u64,
    pub tip_hashes: Vec<RpcHash>,
    pub difficulty: f64,
    pub past_median_time: u64, // NOTE: i64 in gRPC protowire
    pub virtual_parent_hashes: Vec<RpcHash>,
    pub pruning_point_hash: RpcHash,
    pub virtual_daa_score: u64,
    pub sink: RpcHash,
}

impl GetBlockDagInfoResponse {
    pub fn new(
        network: RpcNetworkId,
        block_count: u64,
        header_count: u64,
        tip_hashes: Vec<RpcHash>,
        difficulty: f64,
        past_median_time: u64,
        virtual_parent_hashes: Vec<RpcHash>,
        pruning_point_hash: RpcHash,
        virtual_daa_score: u64,
        sink: RpcHash,
    ) -> Self {
        Self {
            network,
            block_count,
            header_count,
            tip_hashes,
            difficulty,
            past_median_time,
            virtual_parent_hashes,
            pruning_point_hash,
            virtual_daa_score,
            sink,
        }
    }
}

impl Serializer for GetBlockDagInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcNetworkId, &self.network, writer)?;
        store!(u64, &self.block_count, writer)?;
        store!(u64, &self.header_count, writer)?;
        store!(Vec<RpcHash>, &self.tip_hashes, writer)?;
        store!(f64, &self.difficulty, writer)?;
        store!(u64, &self.past_median_time, writer)?;
        store!(Vec<RpcHash>, &self.virtual_parent_hashes, writer)?;
        store!(RpcHash, &self.pruning_point_hash, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;
        store!(RpcHash, &self.sink, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBlockDagInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network = load!(RpcNetworkId, reader)?;
        let block_count = load!(u64, reader)?;
        let header_count = load!(u64, reader)?;
        let tip_hashes = load!(Vec<RpcHash>, reader)?;
        let difficulty = load!(f64, reader)?;
        let past_median_time = load!(u64, reader)?;
        let virtual_parent_hashes = load!(Vec<RpcHash>, reader)?;
        let pruning_point_hash = load!(RpcHash, reader)?;
        let virtual_daa_score = load!(u64, reader)?;
        let sink = load!(RpcHash, reader)?;

        Ok(Self {
            network,
            block_count,
            header_count,
            tip_hashes,
            difficulty,
            past_median_time,
            virtual_parent_hashes,
            pruning_point_hash,
            virtual_daa_score,
            sink,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictRequest {
    pub finality_block_hash: RpcHash,
}

impl ResolveFinalityConflictRequest {
    pub fn new(finality_block_hash: RpcHash) -> Self {
        Self { finality_block_hash }
    }
}

impl Serializer for ResolveFinalityConflictRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.finality_block_hash, writer)?;

        Ok(())
    }
}

impl Deserializer for ResolveFinalityConflictRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let finality_block_hash = load!(RpcHash, reader)?;

        Ok(Self { finality_block_hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveFinalityConflictResponse {}

impl Serializer for ResolveFinalityConflictResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ResolveFinalityConflictResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownRequest {}

impl Serializer for ShutdownRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ShutdownRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownResponse {}

impl Serializer for ShutdownResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for ShutdownResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersRequest {
    pub start_hash: RpcHash,
    pub limit: u64,
    pub is_ascending: bool,
}

impl GetHeadersRequest {
    pub fn new(start_hash: RpcHash, limit: u64, is_ascending: bool) -> Self {
        Self { start_hash, limit, is_ascending }
    }
}

impl Serializer for GetHeadersRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        store!(u64, &self.limit, writer)?;
        store!(bool, &self.is_ascending, writer)?;

        Ok(())
    }
}

impl Deserializer for GetHeadersRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let limit = load!(u64, reader)?;
        let is_ascending = load!(bool, reader)?;

        Ok(Self { start_hash, limit, is_ascending })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetHeadersResponse {
    pub headers: Vec<RpcHeader>,
}

impl GetHeadersResponse {
    pub fn new(headers: Vec<RpcHeader>) -> Self {
        Self { headers }
    }
}

impl Serializer for GetHeadersResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHeader>, &self.headers, writer)?;

        Ok(())
    }
}

impl Deserializer for GetHeadersResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let headers = load!(Vec<RpcHeader>, reader)?;

        Ok(Self { headers })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressRequest {
    pub address: RpcAddress,
}

impl GetBalanceByAddressRequest {
    pub fn new(address: RpcAddress) -> Self {
        Self { address }
    }
}

impl Serializer for GetBalanceByAddressRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.address, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalanceByAddressRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let address = load!(RpcAddress, reader)?;

        Ok(Self { address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalanceByAddressResponse {
    pub balance: u64,
}

impl GetBalanceByAddressResponse {
    pub fn new(balance: u64) -> Self {
        Self { balance }
    }
}

impl Serializer for GetBalanceByAddressResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.balance, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalanceByAddressResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let balance = load!(u64, reader)?;

        Ok(Self { balance })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

impl GetBalancesByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>) -> Self {
        Self { addresses }
    }
}

impl Serializer for GetBalancesByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalancesByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;

        Ok(Self { addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBalancesByAddressesResponse {
    pub entries: Vec<RpcBalancesByAddressesEntry>,
}

impl GetBalancesByAddressesResponse {
    pub fn new(entries: Vec<RpcBalancesByAddressesEntry>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetBalancesByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcBalancesByAddressesEntry>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetBalancesByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcBalancesByAddressesEntry>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkBlueScoreRequest {}

impl Serializer for GetSinkBlueScoreRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSinkBlueScoreRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSinkBlueScoreResponse {
    pub blue_score: u64,
}

impl GetSinkBlueScoreResponse {
    pub fn new(blue_score: u64) -> Self {
        Self { blue_score }
    }
}

impl Serializer for GetSinkBlueScoreResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.blue_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSinkBlueScoreResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let blue_score = load!(u64, reader)?;

        Ok(Self { blue_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
}

impl GetUtxosByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>) -> Self {
        Self { addresses }
    }
}

impl Serializer for GetUtxosByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;

        Ok(Self { addresses })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxosByAddressesResponse {
    pub entries: Vec<RpcUtxosByAddressesEntry>,
}

impl GetUtxosByAddressesResponse {
    pub fn new(entries: Vec<RpcUtxosByAddressesEntry>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetUtxosByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxosByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanRequest {
    pub ip: RpcIpAddress,
}

impl BanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for BanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for BanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BanResponse {}

impl Serializer for BanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for BanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanRequest {
    pub ip: RpcIpAddress,
}

impl UnbanRequest {
    pub fn new(ip: RpcIpAddress) -> Self {
        Self { ip }
    }
}

impl Serializer for UnbanRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcIpAddress, &self.ip, writer)?;

        Ok(())
    }
}

impl Deserializer for UnbanRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let ip = load!(RpcIpAddress, reader)?;

        Ok(Self { ip })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnbanResponse {}

impl Serializer for UnbanResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for UnbanResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondRequest {
    pub window_size: u32,
    pub start_hash: Option<RpcHash>,
}

impl EstimateNetworkHashesPerSecondRequest {
    pub fn new(window_size: u32, start_hash: Option<RpcHash>) -> Self {
        Self { window_size, start_hash }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.window_size, writer)?;
        store!(Option<RpcHash>, &self.start_hash, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let window_size = load!(u32, reader)?;
        let start_hash = load!(Option<RpcHash>, reader)?;

        Ok(Self { window_size, start_hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EstimateNetworkHashesPerSecondResponse {
    pub network_hashes_per_second: u64,
}

impl EstimateNetworkHashesPerSecondResponse {
    pub fn new(network_hashes_per_second: u64) -> Self {
        Self { network_hashes_per_second }
    }
}

impl Serializer for EstimateNetworkHashesPerSecondResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.network_hashes_per_second, writer)?;

        Ok(())
    }
}

impl Deserializer for EstimateNetworkHashesPerSecondResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let network_hashes_per_second = load!(u64, reader)?;

        Ok(Self { network_hashes_per_second })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesRequest {
    pub addresses: Vec<RpcAddress>,
    pub include_orphan_pool: bool,
    // TODO: replace with `include_transaction_pool`
    pub filter_transaction_pool: bool,
}

impl GetMempoolEntriesByAddressesRequest {
    pub fn new(addresses: Vec<RpcAddress>, include_orphan_pool: bool, filter_transaction_pool: bool) -> Self {
        Self { addresses, include_orphan_pool, filter_transaction_pool }
    }
}

impl Serializer for GetMempoolEntriesByAddressesRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;
        store!(bool, &self.include_orphan_pool, writer)?;
        store!(bool, &self.filter_transaction_pool, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;
        let include_orphan_pool = load!(bool, reader)?;
        let filter_transaction_pool = load!(bool, reader)?;

        Ok(Self { addresses, include_orphan_pool, filter_transaction_pool })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMempoolEntriesByAddressesResponse {
    pub entries: Vec<RpcMempoolEntryByAddress>,
}

impl GetMempoolEntriesByAddressesResponse {
    pub fn new(entries: Vec<RpcMempoolEntryByAddress>) -> Self {
        Self { entries }
    }
}

impl Serializer for GetMempoolEntriesByAddressesResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcMempoolEntryByAddress>, &self.entries, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMempoolEntriesByAddressesResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let entries = deserialize!(Vec<RpcMempoolEntryByAddress>, reader)?;

        Ok(Self { entries })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyRequest {}

impl Serializer for GetCoinSupplyRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetCoinSupplyRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCoinSupplyResponse {
    pub max_sompi: u64,
    pub circulating_sompi: u64,
}

impl GetCoinSupplyResponse {
    pub fn new(max_sompi: u64, circulating_sompi: u64) -> Self {
        Self { max_sompi, circulating_sompi }
    }
}

impl Serializer for GetCoinSupplyResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.max_sompi, writer)?;
        store!(u64, &self.circulating_sompi, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCoinSupplyResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let max_sompi = load!(u64, reader)?;
        let circulating_sompi = load!(u64, reader)?;

        Ok(Self { max_sompi, circulating_sompi })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingRequest {}

impl Serializer for PingRequest {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        Ok(())
    }
}

impl Deserializer for PingRequest {
    fn deserialize<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResponse {}

impl Serializer for PingResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PingResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsProfileData {
    pub cpu_usage: f32,
    pub memory_usage: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsRequest {
    pub include_profile_data: bool,
}

impl Serializer for GetConnectionsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(bool, &self.include_profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let include_profile_data = load!(bool, reader)?;
        Ok(Self { include_profile_data })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetConnectionsResponse {
    pub clients: u32,
    pub peers: u16,
    pub profile_data: Option<ConnectionsProfileData>,
}

impl Serializer for GetConnectionsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.clients, writer)?;
        store!(u16, &self.peers, writer)?;
        store!(Option<ConnectionsProfileData>, &self.profile_data, writer)?;
        Ok(())
    }
}

impl Deserializer for GetConnectionsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let clients = load!(u32, reader)?;
        let peers = load!(u16, reader)?;
        let extra = load!(Option<ConnectionsProfileData>, reader)?;
        Ok(Self { clients, peers, profile_data: extra })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoRequest {}

impl Serializer for GetSystemInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(Self {})
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSystemInfoResponse {
    pub version: String,
    pub system_id: Option<Vec<u8>>,
    pub git_hash: Option<Vec<u8>>,
    pub cpu_physical_cores: u16,
    pub total_memory: u64,
    pub fd_limit: u32,
    pub proxy_socket_limit_per_cpu_core: Option<u32>,
}

impl std::fmt::Debug for GetSystemInfoResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GetSystemInfoResponse")
            .field("version", &self.version)
            .field("system_id", &self.system_id.as_ref().map(|id| id.to_hex()))
            .field("git_hash", &self.git_hash.as_ref().map(|hash| hash.to_hex()))
            .field("cpu_physical_cores", &self.cpu_physical_cores)
            .field("total_memory", &self.total_memory)
            .field("fd_limit", &self.fd_limit)
            .field("proxy_socket_limit_per_cpu_core", &self.proxy_socket_limit_per_cpu_core)
            .finish()
    }
}

impl Serializer for GetSystemInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &2, writer)?;
        store!(String, &self.version, writer)?;
        store!(Option<Vec<u8>>, &self.system_id, writer)?;
        store!(Option<Vec<u8>>, &self.git_hash, writer)?;
        store!(u16, &self.cpu_physical_cores, writer)?;
        store!(u64, &self.total_memory, writer)?;
        store!(u32, &self.fd_limit, writer)?;
        store!(Option<u32>, &self.proxy_socket_limit_per_cpu_core, writer)?;

        Ok(())
    }
}

impl Deserializer for GetSystemInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let payload_version = load!(u16, reader)?;
        let version = load!(String, reader)?;
        let system_id = load!(Option<Vec<u8>>, reader)?;
        let git_hash = load!(Option<Vec<u8>>, reader)?;
        let cpu_physical_cores = load!(u16, reader)?;
        let total_memory = load!(u64, reader)?;
        let fd_limit = load!(u32, reader)?;

        let proxy_socket_limit_per_cpu_core = if payload_version > 1 { load!(Option<u32>, reader)? } else { None };

        Ok(Self { version, system_id, git_hash, cpu_physical_cores, total_memory, fd_limit, proxy_socket_limit_per_cpu_core })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsRequest {
    pub process_metrics: bool,
    pub connection_metrics: bool,
    pub bandwidth_metrics: bool,
    pub consensus_metrics: bool,
    pub storage_metrics: bool,
    pub custom_metrics: bool,
}

impl Serializer for GetMetricsRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.process_metrics, writer)?;
        store!(bool, &self.connection_metrics, writer)?;
        store!(bool, &self.bandwidth_metrics, writer)?;
        store!(bool, &self.consensus_metrics, writer)?;
        store!(bool, &self.storage_metrics, writer)?;
        store!(bool, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let process_metrics = load!(bool, reader)?;
        let connection_metrics = load!(bool, reader)?;
        let bandwidth_metrics = load!(bool, reader)?;
        let consensus_metrics = load!(bool, reader)?;
        let storage_metrics = load!(bool, reader)?;
        let custom_metrics = load!(bool, reader)?;

        Ok(Self { process_metrics, connection_metrics, bandwidth_metrics, consensus_metrics, storage_metrics, custom_metrics })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMetrics {
    pub resident_set_size: u64,
    pub virtual_memory_size: u64,
    pub core_num: u32,
    pub cpu_usage: f32,
    pub fd_num: u32,
    pub disk_io_read_bytes: u64,
    pub disk_io_write_bytes: u64,
    pub disk_io_read_per_sec: f32,
    pub disk_io_write_per_sec: f32,
}

impl Serializer for ProcessMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.resident_set_size, writer)?;
        store!(u64, &self.virtual_memory_size, writer)?;
        store!(u32, &self.core_num, writer)?;
        store!(f32, &self.cpu_usage, writer)?;
        store!(u32, &self.fd_num, writer)?;
        store!(u64, &self.disk_io_read_bytes, writer)?;
        store!(u64, &self.disk_io_write_bytes, writer)?;
        store!(f32, &self.disk_io_read_per_sec, writer)?;
        store!(f32, &self.disk_io_write_per_sec, writer)?;

        Ok(())
    }
}

impl Deserializer for ProcessMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let resident_set_size = load!(u64, reader)?;
        let virtual_memory_size = load!(u64, reader)?;
        let core_num = load!(u32, reader)?;
        let cpu_usage = load!(f32, reader)?;
        let fd_num = load!(u32, reader)?;
        let disk_io_read_bytes = load!(u64, reader)?;
        let disk_io_write_bytes = load!(u64, reader)?;
        let disk_io_read_per_sec = load!(f32, reader)?;
        let disk_io_write_per_sec = load!(f32, reader)?;

        Ok(Self {
            resident_set_size,
            virtual_memory_size,
            core_num,
            cpu_usage,
            fd_num,
            disk_io_read_bytes,
            disk_io_write_bytes,
            disk_io_read_per_sec,
            disk_io_write_per_sec,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionMetrics {
    pub borsh_live_connections: u32,
    pub borsh_connection_attempts: u64,
    pub borsh_handshake_failures: u64,
    pub json_live_connections: u32,
    pub json_connection_attempts: u64,
    pub json_handshake_failures: u64,

    pub active_peers: u32,
}

impl Serializer for ConnectionMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u32, &self.borsh_live_connections, writer)?;
        store!(u64, &self.borsh_connection_attempts, writer)?;
        store!(u64, &self.borsh_handshake_failures, writer)?;
        store!(u32, &self.json_live_connections, writer)?;
        store!(u64, &self.json_connection_attempts, writer)?;
        store!(u64, &self.json_handshake_failures, writer)?;
        store!(u32, &self.active_peers, writer)?;

        Ok(())
    }
}

impl Deserializer for ConnectionMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_live_connections = load!(u32, reader)?;
        let borsh_connection_attempts = load!(u64, reader)?;
        let borsh_handshake_failures = load!(u64, reader)?;
        let json_live_connections = load!(u32, reader)?;
        let json_connection_attempts = load!(u64, reader)?;
        let json_handshake_failures = load!(u64, reader)?;
        let active_peers = load!(u32, reader)?;

        Ok(Self {
            borsh_live_connections,
            borsh_connection_attempts,
            borsh_handshake_failures,
            json_live_connections,
            json_connection_attempts,
            json_handshake_failures,
            active_peers,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BandwidthMetrics {
    pub borsh_bytes_tx: u64,
    pub borsh_bytes_rx: u64,
    pub json_bytes_tx: u64,
    pub json_bytes_rx: u64,
    pub p2p_bytes_tx: u64,
    pub p2p_bytes_rx: u64,
    pub grpc_bytes_tx: u64,
    pub grpc_bytes_rx: u64,
}

impl Serializer for BandwidthMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.borsh_bytes_tx, writer)?;
        store!(u64, &self.borsh_bytes_rx, writer)?;
        store!(u64, &self.json_bytes_tx, writer)?;
        store!(u64, &self.json_bytes_rx, writer)?;
        store!(u64, &self.p2p_bytes_tx, writer)?;
        store!(u64, &self.p2p_bytes_rx, writer)?;
        store!(u64, &self.grpc_bytes_tx, writer)?;
        store!(u64, &self.grpc_bytes_rx, writer)?;

        Ok(())
    }
}

impl Deserializer for BandwidthMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let borsh_bytes_tx = load!(u64, reader)?;
        let borsh_bytes_rx = load!(u64, reader)?;
        let json_bytes_tx = load!(u64, reader)?;
        let json_bytes_rx = load!(u64, reader)?;
        let p2p_bytes_tx = load!(u64, reader)?;
        let p2p_bytes_rx = load!(u64, reader)?;
        let grpc_bytes_tx = load!(u64, reader)?;
        let grpc_bytes_rx = load!(u64, reader)?;

        Ok(Self {
            borsh_bytes_tx,
            borsh_bytes_rx,
            json_bytes_tx,
            json_bytes_rx,
            p2p_bytes_tx,
            p2p_bytes_rx,
            grpc_bytes_tx,
            grpc_bytes_rx,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsensusMetrics {
    pub node_blocks_submitted_count: u64,
    pub node_headers_processed_count: u64,
    pub node_dependencies_processed_count: u64,
    pub node_bodies_processed_count: u64,
    pub node_transactions_processed_count: u64,
    pub node_chain_blocks_processed_count: u64,
    pub node_mass_processed_count: u64,

    pub node_database_blocks_count: u64,
    pub node_database_headers_count: u64,

    pub network_mempool_size: u64,
    pub network_tip_hashes_count: u32,
    pub network_difficulty: f64,
    pub network_past_median_time: u64,
    pub network_virtual_parent_hashes_count: u32,
    pub network_virtual_daa_score: u64,
}

impl Serializer for ConsensusMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.node_blocks_submitted_count, writer)?;
        store!(u64, &self.node_headers_processed_count, writer)?;
        store!(u64, &self.node_dependencies_processed_count, writer)?;
        store!(u64, &self.node_bodies_processed_count, writer)?;
        store!(u64, &self.node_transactions_processed_count, writer)?;
        store!(u64, &self.node_chain_blocks_processed_count, writer)?;
        store!(u64, &self.node_mass_processed_count, writer)?;
        store!(u64, &self.node_database_blocks_count, writer)?;
        store!(u64, &self.node_database_headers_count, writer)?;
        store!(u64, &self.network_mempool_size, writer)?;
        store!(u32, &self.network_tip_hashes_count, writer)?;
        store!(f64, &self.network_difficulty, writer)?;
        store!(u64, &self.network_past_median_time, writer)?;
        store!(u32, &self.network_virtual_parent_hashes_count, writer)?;
        store!(u64, &self.network_virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for ConsensusMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let node_blocks_submitted_count = load!(u64, reader)?;
        let node_headers_processed_count = load!(u64, reader)?;
        let node_dependencies_processed_count = load!(u64, reader)?;
        let node_bodies_processed_count = load!(u64, reader)?;
        let node_transactions_processed_count = load!(u64, reader)?;
        let node_chain_blocks_processed_count = load!(u64, reader)?;
        let node_mass_processed_count = load!(u64, reader)?;
        let node_database_blocks_count = load!(u64, reader)?;
        let node_database_headers_count = load!(u64, reader)?;
        let network_mempool_size = load!(u64, reader)?;
        let network_tip_hashes_count = load!(u32, reader)?;
        let network_difficulty = load!(f64, reader)?;
        let network_past_median_time = load!(u64, reader)?;
        let network_virtual_parent_hashes_count = load!(u32, reader)?;
        let network_virtual_daa_score = load!(u64, reader)?;

        Ok(Self {
            node_blocks_submitted_count,
            node_headers_processed_count,
            node_dependencies_processed_count,
            node_bodies_processed_count,
            node_transactions_processed_count,
            node_chain_blocks_processed_count,
            node_mass_processed_count,
            node_database_blocks_count,
            node_database_headers_count,
            network_mempool_size,
            network_tip_hashes_count,
            network_difficulty,
            network_past_median_time,
            network_virtual_parent_hashes_count,
            network_virtual_daa_score,
        })
    }
}

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageMetrics {
    pub storage_size_bytes: u64,
}

impl Serializer for StorageMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.storage_size_bytes, writer)?;

        Ok(())
    }
}

impl Deserializer for StorageMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let storage_size_bytes = load!(u64, reader)?;

        Ok(Self { storage_size_bytes })
    }
}

// TODO: Custom metrics dictionary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CustomMetricValue {
    Placeholder,
}

impl Serializer for CustomMetricValue {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        Ok(())
    }
}

impl Deserializer for CustomMetricValue {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        Ok(CustomMetricValue::Placeholder)
    }
}

/// Hostname-origin peer metrics, exported via [`GetMetricsResponse`].
///
/// `resolutions_total_*` are counters split across eight
/// `(status, trigger)` buckets: `status` in `{ok, failed}`, `trigger`
/// in `{initial, initial_retry, dial_failure, periodic}`. `active`
/// and `resolved_addrs` are gauges over the live hostname registry.
///
/// Wire-level field names (JSON-RPC keys via serde camelCase rename;
/// gRPC field names via the `.proto` schema) drop the redundant
/// `peerHostname` / `peer_hostname_` prefix; the type name
/// `PeerHostnameMetrics` already carries the group, consistent with
/// `BandwidthMetrics`, `ConnectionMetrics`, `ProcessMetrics`, etc.
/// The Prometheus-canonical metric NAMES
/// (`peer_hostname_resolutions_total{status, trigger}`,
/// `peer_hostname_active`, `peer_hostname_resolved_addrs`) remain as
/// declared in the spec; a future Prometheus exporter prepends the
/// type-group prefix at presentation time.
///
/// **Field ordering -- borsh vs proto, maintenance hint.** Field-
/// declaration order here is semantic-grouped
/// (`Initial -> InitialRetry -> DialFailure -> Periodic`) so the eight
/// resolution buckets read naturally as a 4 trigger x 2 status grid.
/// The matching gRPC `.proto` schema (`rpc/grpc/core/proto/rpc.proto`
/// `message PeerHostnameMetrics`) is history-ordered:
/// `Initial=1-2, DialFailure=3-4, Periodic=5-6, InitialRetry=7-8`,
/// with `reserved 9, 10` allocating the next trigger pair slot. This
/// is NOT a wire-compatibility issue (proto3 is field-number-keyed;
/// borsh is positional), but a future trigger label addition has to
/// keep both orderings consistent: append the new pair at the borsh
/// tail (which forces a `PEER_HOSTNAME_METRICS_BORSH_V1 -> V2` bump
/// for forward-compat readers), allocate the next reserved pair in
/// proto, and update the `from!` / `try_from!` mappings in
/// `rpc/grpc/core/src/convert/metrics.rs`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerHostnameMetrics {
    pub resolutions_total_initial_ok: u64,
    pub resolutions_total_initial_failed: u64,
    pub resolutions_total_initial_retry_ok: u64,
    pub resolutions_total_initial_retry_failed: u64,
    pub resolutions_total_dial_failure_ok: u64,
    pub resolutions_total_dial_failure_failed: u64,
    pub resolutions_total_periodic_ok: u64,
    pub resolutions_total_periodic_failed: u64,
    pub active: u64,
    pub resolved_addrs: u64,
}

/// Wire version of the [`PeerHostnameMetrics`] borsh encoding.
///
/// v1: ten `u64` fields in struct-declaration order
/// (`resolutions_total_initial_ok`, `_initial_failed`,
/// `_initial_retry_ok`, `_initial_retry_failed`, `_dial_failure_ok`,
/// `_dial_failure_failed`, `_periodic_ok`, `_periodic_failed`,
/// `active`, `resolved_addrs`).
///
/// Future layout extensions bump the leading `u16` tag and append at
/// the tail; the `Deserializer` matches on the tag and dispatches to
/// the matching field-set so v1 readers continue to decode v1 payloads
/// without surprises and reject unknown versions explicitly.
const PEER_HOSTNAME_METRICS_BORSH_V1: u16 = 1;

impl Serializer for PeerHostnameMetrics {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &PEER_HOSTNAME_METRICS_BORSH_V1, writer)?;
        store!(u64, &self.resolutions_total_initial_ok, writer)?;
        store!(u64, &self.resolutions_total_initial_failed, writer)?;
        store!(u64, &self.resolutions_total_initial_retry_ok, writer)?;
        store!(u64, &self.resolutions_total_initial_retry_failed, writer)?;
        store!(u64, &self.resolutions_total_dial_failure_ok, writer)?;
        store!(u64, &self.resolutions_total_dial_failure_failed, writer)?;
        store!(u64, &self.resolutions_total_periodic_ok, writer)?;
        store!(u64, &self.resolutions_total_periodic_failed, writer)?;
        store!(u64, &self.active, writer)?;
        store!(u64, &self.resolved_addrs, writer)?;
        Ok(())
    }
}

impl Deserializer for PeerHostnameMetrics {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let version = load!(u16, reader)?;
        match version {
            PEER_HOSTNAME_METRICS_BORSH_V1 => {
                let resolutions_total_initial_ok = load!(u64, reader)?;
                let resolutions_total_initial_failed = load!(u64, reader)?;
                let resolutions_total_initial_retry_ok = load!(u64, reader)?;
                let resolutions_total_initial_retry_failed = load!(u64, reader)?;
                let resolutions_total_dial_failure_ok = load!(u64, reader)?;
                let resolutions_total_dial_failure_failed = load!(u64, reader)?;
                let resolutions_total_periodic_ok = load!(u64, reader)?;
                let resolutions_total_periodic_failed = load!(u64, reader)?;
                let active = load!(u64, reader)?;
                let resolved_addrs = load!(u64, reader)?;
                Ok(Self {
                    resolutions_total_initial_ok,
                    resolutions_total_initial_failed,
                    resolutions_total_initial_retry_ok,
                    resolutions_total_initial_retry_failed,
                    resolutions_total_dial_failure_ok,
                    resolutions_total_dial_failure_failed,
                    resolutions_total_periodic_ok,
                    resolutions_total_periodic_failed,
                    active,
                    resolved_addrs,
                })
            }
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown PeerHostnameMetrics borsh version: {other}"),
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMetricsResponse {
    pub server_time: u64,
    pub process_metrics: Option<ProcessMetrics>,
    pub connection_metrics: Option<ConnectionMetrics>,
    pub bandwidth_metrics: Option<BandwidthMetrics>,
    pub consensus_metrics: Option<ConsensusMetrics>,
    pub storage_metrics: Option<StorageMetrics>,
    // TODO: this is currently a placeholder
    pub custom_metrics: Option<HashMap<String, CustomMetricValue>>,
}

impl GetMetricsResponse {
    pub fn new(
        server_time: u64,
        process_metrics: Option<ProcessMetrics>,
        connection_metrics: Option<ConnectionMetrics>,
        bandwidth_metrics: Option<BandwidthMetrics>,
        consensus_metrics: Option<ConsensusMetrics>,
        storage_metrics: Option<StorageMetrics>,
        custom_metrics: Option<HashMap<String, CustomMetricValue>>,
    ) -> Self {
        Self {
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            server_time,
            custom_metrics,
        }
    }
}

impl Serializer for GetMetricsResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.server_time, writer)?;
        serialize!(Option<ProcessMetrics>, &self.process_metrics, writer)?;
        serialize!(Option<ConnectionMetrics>, &self.connection_metrics, writer)?;
        serialize!(Option<BandwidthMetrics>, &self.bandwidth_metrics, writer)?;
        serialize!(Option<ConsensusMetrics>, &self.consensus_metrics, writer)?;
        serialize!(Option<StorageMetrics>, &self.storage_metrics, writer)?;
        serialize!(Option<HashMap<String, CustomMetricValue>>, &self.custom_metrics, writer)?;

        Ok(())
    }
}

impl Deserializer for GetMetricsResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let server_time = load!(u64, reader)?;
        let process_metrics = deserialize!(Option<ProcessMetrics>, reader)?;
        let connection_metrics = deserialize!(Option<ConnectionMetrics>, reader)?;
        let bandwidth_metrics = deserialize!(Option<BandwidthMetrics>, reader)?;
        let consensus_metrics = deserialize!(Option<ConsensusMetrics>, reader)?;
        let storage_metrics = deserialize!(Option<StorageMetrics>, reader)?;
        let custom_metrics = deserialize!(Option<HashMap<String, CustomMetricValue>>, reader)?;

        Ok(Self {
            server_time,
            process_metrics,
            connection_metrics,
            bandwidth_metrics,
            consensus_metrics,
            storage_metrics,
            custom_metrics,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "camelCase")]
#[borsh(use_discriminant = true)]
pub enum RpcCaps {
    Full = 0,
    Blocks,
    UtxoIndex,
    Mempool,
    Metrics,
    Visualizer,
    Mining,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoRequest {}

impl Serializer for GetServerInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetServerInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetServerInfoResponse {
    pub rpc_api_version: u16,
    pub rpc_api_revision: u16,
    pub server_version: String,
    pub network_id: RpcNetworkId,
    pub has_utxo_index: bool,
    pub is_synced: bool,
    pub virtual_daa_score: u64,
}

impl Serializer for GetServerInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;

        store!(u16, &self.rpc_api_version, writer)?;
        store!(u16, &self.rpc_api_revision, writer)?;

        store!(String, &self.server_version, writer)?;
        store!(RpcNetworkId, &self.network_id, writer)?;
        store!(bool, &self.has_utxo_index, writer)?;
        store!(bool, &self.is_synced, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetServerInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;

        let rpc_api_version = load!(u16, reader)?;
        let rpc_api_revision = load!(u16, reader)?;

        let server_version = load!(String, reader)?;
        let network_id = load!(RpcNetworkId, reader)?;
        let has_utxo_index = load!(bool, reader)?;
        let is_synced = load!(bool, reader)?;
        let virtual_daa_score = load!(u64, reader)?;

        Ok(Self { rpc_api_version, rpc_api_revision, server_version, network_id, has_utxo_index, is_synced, virtual_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusRequest {}

impl Serializer for GetSyncStatusRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSyncStatusResponse {
    pub is_synced: bool,
}

impl Serializer for GetSyncStatusResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.is_synced, writer)?;
        Ok(())
    }
}

impl Deserializer for GetSyncStatusResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let is_synced = load!(bool, reader)?;
        Ok(Self { is_synced })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateRequest {
    pub daa_scores: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateRequest {
    pub fn new(daa_scores: Vec<u64>) -> Self {
        Self { daa_scores }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.daa_scores, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let daa_scores = load!(Vec<u64>, reader)?;
        Ok(Self { daa_scores })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetDaaScoreTimestampEstimateResponse {
    pub timestamps: Vec<u64>,
}

impl GetDaaScoreTimestampEstimateResponse {
    pub fn new(timestamps: Vec<u64>) -> Self {
        Self { timestamps }
    }
}

impl Serializer for GetDaaScoreTimestampEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<u64>, &self.timestamps, writer)?;
        Ok(())
    }
}

impl Deserializer for GetDaaScoreTimestampEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let timestamps = load!(Vec<u64>, reader)?;
        Ok(Self { timestamps })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// Fee rate estimations

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateRequest {}

impl Serializer for GetFeeEstimateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateResponse {
    pub estimate: RpcFeeEstimate,
}

impl Serializer for GetFeeEstimateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        Ok(Self { estimate })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalRequest {
    pub verbose: bool,
}

impl Serializer for GetFeeEstimateExperimentalRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let verbose = load!(bool, reader)?;
        Ok(Self { verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFeeEstimateExperimentalResponse {
    /// The usual feerate estimate response
    pub estimate: RpcFeeEstimate,

    /// Experimental verbose data
    pub verbose: Option<RpcFeeEstimateVerboseExperimentalData>,
}

impl Serializer for GetFeeEstimateExperimentalResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcFeeEstimate, &self.estimate, writer)?;
        serialize!(Option<RpcFeeEstimateVerboseExperimentalData>, &self.verbose, writer)?;
        Ok(())
    }
}

impl Deserializer for GetFeeEstimateExperimentalResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let estimate = deserialize!(RpcFeeEstimate, reader)?;
        let verbose = deserialize!(Option<RpcFeeEstimateVerboseExperimentalData>, reader)?;
        Ok(Self { estimate, verbose })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorRequest {
    pub hash: RpcHash,
}

impl Serializer for GetCurrentBlockColorRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;

        Ok(Self { hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetCurrentBlockColorResponse {
    pub blue: bool,
}

impl Serializer for GetCurrentBlockColorResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.blue, writer)?;

        Ok(())
    }
}

impl Deserializer for GetCurrentBlockColorResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let blue = load!(bool, reader)?;

        Ok(Self { blue })
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "lowercase")]
#[borsh(use_discriminant = true)]
#[repr(i32)]
pub enum RpcBlockColor {
    Unknown = 0,
    Blue = 1,
    Red = 2,
}

impl From<RpcBlockColor> for i32 {
    fn from(value: RpcBlockColor) -> Self {
        value as i32
    }
}

impl From<i32> for RpcBlockColor {
    fn from(value: i32) -> Self {
        match value {
            1 => Self::Blue,
            2 => Self::Red,
            _ => Self::Unknown,
        }
    }
}

impl Serializer for RpcBlockColor {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u8, &1, writer)?;
        store!(i32, &i32::from(*self), writer)?;
        Ok(())
    }
}

impl Deserializer for RpcBlockColor {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u8, reader)?;
        let value = load!(i32, reader)?;
        Ok(RpcBlockColor::from(value))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockRewardInfoRequest {
    pub hash: RpcHash,
}

impl GetBlockRewardInfoRequest {
    pub fn new(hash: RpcHash) -> Self {
        Self { hash }
    }
}

impl Serializer for GetBlockRewardInfoRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.hash, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockRewardInfoRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let hash = load!(RpcHash, reader)?;
        Ok(Self { hash })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetBlockRewardInfoResponse {
    pub header: RpcHeader,
    pub block_color: RpcBlockColor,
    /// guaranteed to be populated when block color != Unknown
    pub confirmation_count: Option<u64>,
    /// guaranteed to be populated when block color != Unknown
    pub merging_chain_block_hash: Option<RpcHash>,
    /// guaranteed to be populated when block color == Blue
    pub reward_amount: Option<u64>,
}

impl GetBlockRewardInfoResponse {
    pub fn new(
        header: RpcHeader,
        block_color: RpcBlockColor,
        confirmation_count: Option<u64>,
        merging_chain_block_hash: Option<RpcHash>,
        reward_amount: Option<u64>,
    ) -> Self {
        Self { header, block_color, confirmation_count, merging_chain_block_hash, reward_amount }
    }
}

impl Serializer for GetBlockRewardInfoResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHeader, &self.header, writer)?;
        store!(RpcBlockColor, &self.block_color, writer)?;
        store!(Option<u64>, &self.confirmation_count, writer)?;
        store!(Option<RpcHash>, &self.merging_chain_block_hash, writer)?;
        store!(Option<u64>, &self.reward_amount, writer)?;
        Ok(())
    }
}

impl Deserializer for GetBlockRewardInfoResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let header = load!(RpcHeader, reader)?;
        let block_color = load!(RpcBlockColor, reader)?;
        let confirmation_count = load!(Option<u64>, reader)?;
        let merging_chain_block_hash = load!(Option<RpcHash>, reader)?;
        let reward_amount = load!(Option<u64>, reader)?;
        Ok(Self { header, block_color, confirmation_count, merging_chain_block_hash, reward_amount })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressRequest {
    pub txid: RpcHash,
    pub accepting_block_daa_score: u64,
}

impl GetUtxoReturnAddressRequest {
    pub fn new(txid: RpcHash, accepting_block_daa_score: u64) -> Self {
        Self { txid, accepting_block_daa_score }
    }
}

impl Serializer for GetUtxoReturnAddressRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.txid, writer)?;
        store!(u64, &self.accepting_block_daa_score, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let txid = load!(RpcHash, reader)?;
        let accepting_block_daa_score = load!(u64, reader)?;

        Ok(Self { txid, accepting_block_daa_score })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetUtxoReturnAddressResponse {
    pub return_address: RpcAddress,
}

impl GetUtxoReturnAddressResponse {
    pub fn new(return_address: RpcAddress) -> Self {
        Self { return_address }
    }
}

impl Serializer for GetUtxoReturnAddressResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcAddress, &self.return_address, writer)?;

        Ok(())
    }
}

impl Deserializer for GetUtxoReturnAddressResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let return_address = load!(RpcAddress, reader)?;

        Ok(Self { return_address })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Request {
    pub start_hash: RpcHash,
    pub data_verbosity_level: Option<RpcDataVerbosityLevel>,
    pub min_confirmation_count: Option<u64>,
}

impl GetVirtualChainFromBlockV2Request {
    pub fn new(start_hash: RpcHash, data_verbosity_level: Option<RpcDataVerbosityLevel>, min_confirmation_count: Option<u64>) -> Self {
        Self { start_hash, data_verbosity_level, min_confirmation_count }
    }
}

impl Serializer for GetVirtualChainFromBlockV2Request {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.start_hash, writer)?;
        serialize!(Option<RpcDataVerbosityLevel>, &self.data_verbosity_level, writer)?;
        store!(Option<u64>, &self.min_confirmation_count, writer)?;

        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Request {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let start_hash = load!(RpcHash, reader)?;
        let data_verbosity_level = deserialize!(Option<RpcDataVerbosityLevel>, reader)?;
        let min_confirmation_count = load!(Option<u64>, reader)?;

        Ok(Self { start_hash, data_verbosity_level, min_confirmation_count })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetVirtualChainFromBlockV2Response {
    /// always present, no matter the verbosity level
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// always present, no matter the verbosity level
    pub added_chain_block_hashes: Arc<Vec<RpcHash>>,
    /// struct properties are optionally returned depending on the verbosity level
    pub chain_block_accepted_transactions: Arc<Vec<RpcChainBlockAcceptedTransactions>>,
}

impl Serializer for GetVirtualChainFromBlockV2Response {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        serialize!(Vec<RpcChainBlockAcceptedTransactions>, &self.chain_block_accepted_transactions, writer)?;
        Ok(())
    }
}

impl Deserializer for GetVirtualChainFromBlockV2Response {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let chain_block_accepted_transactions = deserialize!(Vec<RpcChainBlockAcceptedTransactions>, reader)?;
        Ok(Self {
            removed_chain_block_hashes: removed_chain_block_hashes.into(),
            added_chain_block_hashes: added_chain_block_hashes.into(),
            chain_block_accepted_transactions: chain_block_accepted_transactions.into(),
        })
    }
}

// ----------------------------------------------------------------------------
// Subscriptions & notifications
// ----------------------------------------------------------------------------

// ~~~~~~~~~~~~~~~~~~~~~~
// BlockAddedNotification

/// NotifyBlockAddedRequest registers this connection for blockAdded notifications.
///
/// See: BlockAddedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedRequest {
    pub command: Command,
}
impl NotifyBlockAddedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyBlockAddedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyBlockAddedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBlockAddedResponse {}

impl Serializer for NotifyBlockAddedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyBlockAddedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// BlockAddedNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: NotifyBlockAddedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockAddedNotification {
    pub block: Arc<RpcBlock>,
}

impl Serializer for BlockAddedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(RpcBlock, &self.block, writer)?;
        Ok(())
    }
}

impl Deserializer for BlockAddedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let block = deserialize!(RpcBlock, reader)?;
        Ok(Self { block: block.into() })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// VirtualChainChangedNotification

// NotifyVirtualChainChangedRequest registers this connection for
// virtualDaaScoreChanged notifications.
//
// See: VirtualChainChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualChainChangedRequest {
    pub include_accepted_transaction_ids: bool,
    pub command: Command,
}

impl NotifyVirtualChainChangedRequest {
    pub fn new(include_accepted_transaction_ids: bool, command: Command) -> Self {
        Self { include_accepted_transaction_ids, command }
    }
}

impl Serializer for NotifyVirtualChainChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(bool, &self.include_accepted_transaction_ids, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualChainChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let include_accepted_transaction_ids = load!(bool, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { include_accepted_transaction_ids, command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualChainChangedResponse {}

impl Serializer for NotifyVirtualChainChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualChainChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// VirtualChainChangedNotification is sent whenever the DAG's selected parent
// chain had changed.
//
// See: NotifyVirtualChainChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualChainChangedNotification {
    pub removed_chain_block_hashes: Arc<Vec<RpcHash>>,
    pub added_chain_block_hashes: Arc<Vec<RpcHash>>,
    pub accepted_transaction_ids: Arc<Vec<RpcAcceptedTransactionIds>>,
}

impl Serializer for VirtualChainChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcHash>, &self.removed_chain_block_hashes, writer)?;
        store!(Vec<RpcHash>, &self.added_chain_block_hashes, writer)?;
        store!(Vec<RpcAcceptedTransactionIds>, &self.accepted_transaction_ids, writer)?;
        Ok(())
    }
}

impl Deserializer for VirtualChainChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let removed_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let added_chain_block_hashes = load!(Vec<RpcHash>, reader)?;
        let accepted_transaction_ids = load!(Vec<RpcAcceptedTransactionIds>, reader)?;
        Ok(Self {
            removed_chain_block_hashes: removed_chain_block_hashes.into(),
            added_chain_block_hashes: added_chain_block_hashes.into(),
            accepted_transaction_ids: accepted_transaction_ids.into(),
        })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// FinalityConflictNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictRequest {
    pub command: Command,
}

impl NotifyFinalityConflictRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyFinalityConflictRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResponse {}

impl Serializer for NotifyFinalityConflictResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalityConflictNotification {
    pub violating_block_hash: RpcHash,
}

impl Serializer for FinalityConflictNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.violating_block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for FinalityConflictNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let violating_block_hash = load!(RpcHash, reader)?;
        Ok(Self { violating_block_hash })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// FinalityConflictResolvedNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResolvedRequest {
    pub command: Command,
}

impl NotifyFinalityConflictResolvedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyFinalityConflictResolvedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResolvedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyFinalityConflictResolvedResponse {}

impl Serializer for NotifyFinalityConflictResolvedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyFinalityConflictResolvedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalityConflictResolvedNotification {
    pub finality_block_hash: RpcHash,
}

impl Serializer for FinalityConflictResolvedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(RpcHash, &self.finality_block_hash, writer)?;
        Ok(())
    }
}

impl Deserializer for FinalityConflictResolvedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let finality_block_hash = load!(RpcHash, reader)?;
        Ok(Self { finality_block_hash })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~
// UtxosChangedNotification

// NotifyUtxosChangedRequestMessage registers this connection for utxoChanged notifications
// for the given addresses. Depending on the provided `command`, notifications will
// start or stop for the provided `addresses`.
//
// If `addresses` is empty, the notifications will start or stop for all addresses.
//
// This call is only available when this kaspad was started with `--utxoindex`
//
// See: UtxosChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedRequest {
    pub addresses: Vec<RpcAddress>,
    pub command: Command,
}

impl NotifyUtxosChangedRequest {
    pub fn new(addresses: Vec<RpcAddress>, command: Command) -> Self {
        Self { addresses, command }
    }
}

impl Serializer for NotifyUtxosChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Vec<RpcAddress>, &self.addresses, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyUtxosChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let addresses = load!(Vec<RpcAddress>, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { addresses, command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyUtxosChangedResponse {}

impl Serializer for NotifyUtxosChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyUtxosChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// UtxosChangedNotificationMessage is sent whenever the UTXO index had been updated.
//
// See: NotifyUtxosChangedRequest
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UtxosChangedNotification {
    pub added: Arc<Vec<RpcUtxosByAddressesEntry>>,
    pub removed: Arc<Vec<RpcUtxosByAddressesEntry>>,
}

impl UtxosChangedNotification {
    pub(crate) fn apply_utxos_changed_subscription(
        &self,
        subscription: &UtxosChangedSubscription,
        context: &SubscriptionContext,
    ) -> Option<Self> {
        if subscription.to_all() {
            Some(self.clone())
        } else {
            let added = Self::filter_utxos(&self.added, subscription, context);
            let removed = Self::filter_utxos(&self.removed, subscription, context);
            if added.is_empty() && removed.is_empty() {
                None
            } else {
                debug!("CRPC, Creating UtxosChanged notifications with {} added and {} removed utxos", added.len(), removed.len());
                Some(Self { added: Arc::new(added), removed: Arc::new(removed) })
            }
        }
    }

    fn filter_utxos(
        utxo_set: &[RpcUtxosByAddressesEntry],
        subscription: &UtxosChangedSubscription,
        context: &SubscriptionContext,
    ) -> Vec<RpcUtxosByAddressesEntry> {
        let subscription_data = subscription.data();
        utxo_set.iter().filter(|x| subscription_data.contains(&x.utxo_entry.script_public_key, context)).cloned().collect()
    }
}

impl Serializer for UtxosChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.added, writer)?;
        serialize!(Vec<RpcUtxosByAddressesEntry>, &self.removed, writer)?;
        Ok(())
    }
}

impl Deserializer for UtxosChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let added = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        let removed = deserialize!(Vec<RpcUtxosByAddressesEntry>, reader)?;
        Ok(Self { added: added.into(), removed: removed.into() })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// SinkBlueScoreChangedNotification

// NotifySinkBlueScoreChangedRequest registers this connection for
// sinkBlueScoreChanged notifications.
//
// See: SinkBlueScoreChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySinkBlueScoreChangedRequest {
    pub command: Command,
}

impl NotifySinkBlueScoreChangedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifySinkBlueScoreChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifySinkBlueScoreChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifySinkBlueScoreChangedResponse {}

impl Serializer for NotifySinkBlueScoreChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifySinkBlueScoreChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// SinkBlueScoreChangedNotification is sent whenever the blue score
// of the virtual's selected parent changes.
//
/// See: NotifySinkBlueScoreChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SinkBlueScoreChangedNotification {
    pub sink_blue_score: u64,
}

impl Serializer for SinkBlueScoreChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.sink_blue_score, writer)?;
        Ok(())
    }
}

impl Deserializer for SinkBlueScoreChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let sink_blue_score = load!(u64, reader)?;
        Ok(Self { sink_blue_score })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// VirtualDaaScoreChangedNotification

// NotifyVirtualDaaScoreChangedRequest registers this connection for
// virtualDaaScoreChanged notifications.
//
// See: VirtualDaaScoreChangedNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualDaaScoreChangedRequest {
    pub command: Command,
}

impl NotifyVirtualDaaScoreChangedRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyVirtualDaaScoreChangedRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualDaaScoreChangedRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyVirtualDaaScoreChangedResponse {}

impl Serializer for NotifyVirtualDaaScoreChangedResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyVirtualDaaScoreChangedResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// VirtualDaaScoreChangedNotification is sent whenever the DAA score
// of the virtual changes.
//
// See NotifyVirtualDaaScoreChangedRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualDaaScoreChangedNotification {
    pub virtual_daa_score: u64,
}

impl Serializer for VirtualDaaScoreChangedNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.virtual_daa_score, writer)?;
        Ok(())
    }
}

impl Deserializer for VirtualDaaScoreChangedNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let virtual_daa_score = load!(u64, reader)?;
        Ok(Self { virtual_daa_score })
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// PruningPointUtxoSetOverrideNotification

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPruningPointUtxoSetOverrideRequest {
    pub command: Command,
}

impl NotifyPruningPointUtxoSetOverrideRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyPruningPointUtxoSetOverrideRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyPruningPointUtxoSetOverrideRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyPruningPointUtxoSetOverrideResponse {}

impl Serializer for NotifyPruningPointUtxoSetOverrideResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyPruningPointUtxoSetOverrideResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PruningPointUtxoSetOverrideNotification {}

impl Serializer for PruningPointUtxoSetOverrideNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for PruningPointUtxoSetOverrideNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// NewBlockTemplateNotification

/// NotifyNewBlockTemplateRequest registers this connection for blockAdded notifications.
///
/// See: NewBlockTemplateNotification
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateRequest {
    pub command: Command,
}
impl NotifyNewBlockTemplateRequest {
    pub fn new(command: Command) -> Self {
        Self { command }
    }
}

impl Serializer for NotifyNewBlockTemplateRequest {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(Command, &self.command, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyNewBlockTemplateRequest {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let command = load!(Command, reader)?;
        Ok(Self { command })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyNewBlockTemplateResponse {}

impl Serializer for NotifyNewBlockTemplateResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NotifyNewBlockTemplateResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

/// NewBlockTemplateNotification is sent whenever a blocks has been added (NOT accepted)
/// into the DAG.
///
/// See: NotifyNewBlockTemplateRequest
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewBlockTemplateNotification {}

impl Serializer for NewBlockTemplateNotification {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        Ok(())
    }
}

impl Deserializer for NewBlockTemplateNotification {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        Ok(Self {})
    }
}

///
///  wRPC response for RpcApiOps::Subscribe request
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeResponse {
    id: u64,
}

impl SubscribeResponse {
    pub fn new(id: u64) -> Self {
        Self { id }
    }
}

impl Serializer for SubscribeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)?;
        store!(u64, &self.id, writer)?;
        Ok(())
    }
}

impl Deserializer for SubscribeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader)?;
        let id = load!(u64, reader)?;
        Ok(Self { id })
    }
}

///
///  wRPC response for RpcApiOps::Unsubscribe request
///
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsubscribeResponse {}

impl Serializer for UnsubscribeResponse {
    fn serialize<W: std::io::Write>(&self, writer: &mut W) -> std::io::Result<()> {
        store!(u16, &1, writer)
    }
}

impl Deserializer for UnsubscribeResponse {
    fn deserialize<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        let _version = load!(u16, reader);
        Ok(Self {})
    }
}

#[cfg(test)]
mod add_peer_request_borsh_tests {
    use super::*;
    use kaspa_utils::networking::{ContextualNetAddress, IpAddress};
    use std::net::{IpAddr, Ipv4Addr};

    /// Hand-encoded v1 wire buffer for `AddPeerRequest { peer_address:
    /// RpcContextualPeerAddress(1.2.3.4:16111), is_permanent: true }`.
    ///
    /// Layout: u16=1 LE | IpAddress { variant=0 (V4), octets [1,2,3,4] }
    ///       | Option<u16>::Some=1, port=16111 LE | bool=true
    fn v1_fixture_bytes() -> Vec<u8> {
        vec![
            0x01, 0x00, // u16 version = 1
            0x00, // IpAddress variant tag: V4
            0x01, 0x02, 0x03, 0x04, // octets 1.2.3.4
            0x01, // Option::Some
            0xef, 0x3e, // u16 LE port = 16111 (0x3eef)
            0x01, // bool is_permanent = true
        ]
    }

    fn workflow_serialize(req: &AddPeerRequest) -> Vec<u8> {
        let mut out = Vec::new();
        Serializer::serialize(req, &mut out).unwrap();
        out
    }

    fn workflow_deserialize(bytes: &[u8]) -> AddPeerRequest {
        let mut cursor = std::io::Cursor::new(bytes);
        Deserializer::deserialize(&mut cursor).unwrap()
    }

    #[test]
    fn add_peer_request_v1_byte_buffer_decodes_to_address() {
        let bytes = v1_fixture_bytes();
        let req = workflow_deserialize(&bytes);
        match req.peer_address {
            RpcPeerEndpoint::Address(addr) => {
                let want = ContextualNetAddress::new(IpAddress::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))), Some(16111));
                assert_eq!(addr, want);
            }
            other => panic!("expected Address variant, got {other:?}"),
        }
        assert!(req.is_permanent);
    }

    #[test]
    fn add_peer_request_address_emits_v1_byte_identical() {
        // Cross-version compatibility (new client -> old server): an
        // Address-variant request from a newly-built client MUST emit a
        // wire frame byte-identical to the v1 fixture, so an older
        // server that only decodes v1 still accepts it.
        let original = AddPeerRequest::new(
            RpcPeerEndpoint::Address(ContextualNetAddress::new(IpAddress::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4))), Some(16111))),
            true,
        );
        let emitted = workflow_serialize(&original);
        assert_eq!(emitted, v1_fixture_bytes(), "Address-variant request must serialize byte-identical to the v1 fixture");
        // And the new client's emitted buffer round-trips through the
        // new server's deserializer back to the Address variant.
        let back = workflow_deserialize(&emitted);
        assert_eq!(back.peer_address, original.peer_address);
        assert_eq!(back.is_permanent, original.is_permanent);
    }

    #[test]
    fn add_peer_request_v2_roundtrip_hostname() {
        let original = AddPeerRequest::new(RpcPeerEndpoint::from_str("node.example.com:16111").unwrap(), false);
        let bytes = workflow_serialize(&original);
        // Hostname payload MUST serialize as v2.
        assert_eq!(&bytes[..2], &[0x02, 0x00]);
        let back = workflow_deserialize(&bytes);
        assert_eq!(back.peer_address, original.peer_address);
        assert!(!back.is_permanent);
        // Hostname-with-no-port also roundtrips.
        let np = AddPeerRequest::new(RpcPeerEndpoint::from_str("pod-1.svc.cluster.local").unwrap(), true);
        let np_bytes = workflow_serialize(&np);
        assert_eq!(&np_bytes[..2], &[0x02, 0x00]);
        let np_back = workflow_deserialize(&np_bytes);
        assert_eq!(np_back.peer_address, np.peer_address);
        assert!(np_back.is_permanent);
    }

    #[test]
    fn add_peer_request_v2_string_payload_format() {
        // The body of the v2 wire must be the canonical Display form of
        // the endpoint, encoded as a borsh String (u32 LE length prefix
        // + UTF-8 bytes). Only Hostname-variant payloads are emitted as
        // v2; Address-variant payloads (numeric IP literals) emit v1.
        for input in ["node.example.com:16111", "node.example.com", "pod-1.svc.cluster.local"] {
            let endpoint = RpcPeerEndpoint::from_str(input).unwrap();
            assert!(matches!(endpoint, RpcPeerEndpoint::Hostname { .. }), "test setup: {input} must parse as Hostname");
            let req = AddPeerRequest::new(endpoint.clone(), true);
            let bytes = workflow_serialize(&req);
            // bytes[0..2]    = version u16
            // bytes[2..6]    = string length u32 LE
            // bytes[6..6+L]  = string bytes (UTF-8)
            // bytes[6+L]     = bool
            assert_eq!(&bytes[..2], &[0x02, 0x00]);
            let display = endpoint.to_string();
            let len = display.len();
            let len_bytes = (len as u32).to_le_bytes();
            assert_eq!(&bytes[2..6], &len_bytes, "length-prefix mismatch for {input}");
            assert_eq!(&bytes[6..6 + len], display.as_bytes(), "string payload mismatch for {input}");
            assert_eq!(bytes[6 + len], 1u8, "bool tail mismatch for {input}");
        }
    }

    #[test]
    fn add_peer_request_address_v1_roundtrip_for_ipv6_and_no_port() {
        // Address-variant requests with IPv6 literals or no explicit port
        // also serialize as v1 (numeric forms never need hostname-aware
        // wire encoding).
        for input in ["[::1]:16111", "1.2.3.4"] {
            let endpoint = RpcPeerEndpoint::from_str(input).unwrap();
            assert!(matches!(endpoint, RpcPeerEndpoint::Address(_)), "test setup: {input} must parse as Address");
            let req = AddPeerRequest::new(endpoint.clone(), false);
            let bytes = workflow_serialize(&req);
            assert_eq!(&bytes[..2], &[0x01, 0x00], "expected v1 version prefix for Address-variant input {input}");
            let back = workflow_deserialize(&bytes);
            assert_eq!(back.peer_address, endpoint);
            assert!(!back.is_permanent);
        }
    }

    /// Deterministic counterpart to the random-Mock-driven
    /// `test!(AddPeerRequest)` round trip in `tests.rs`: drives a
    /// `Hostname`-variant `AddPeerRequest` through the same
    /// `workflow_serializer` framing the macro uses (PREFIX | payload |
    /// SUFFIX), so every run -- not just runs where Mock happens to pick
    /// the Hostname arm -- exercises the v2 codegen path.
    #[test]
    fn add_peer_request_v2_macro_framed_roundtrip() {
        const PREFIX: u32 = 0x12345678;
        const SUFFIX: u32 = 0x90abcdef;

        for input in ["node.example.com:16111", "node.example.com", "pod-1.svc.cluster.local"] {
            let original = AddPeerRequest::new(RpcPeerEndpoint::from_str(input).unwrap(), true);
            assert!(matches!(original.peer_address, RpcPeerEndpoint::Hostname { .. }), "test setup: {input} must parse as Hostname",);

            let mut buffer1 = Vec::new();
            {
                let writer = &mut buffer1;
                store!(u32, &PREFIX, writer).unwrap();
                serialize!(AddPeerRequest, &original, writer).unwrap();
                store!(u32, &SUFFIX, writer).unwrap();
            }

            let reader = &mut buffer1.as_slice();
            let prefix: u32 = load!(u32, reader).unwrap();
            assert_eq!(prefix, PREFIX, "frame misalignment in `{input}`");
            let decoded: AddPeerRequest = deserialize!(AddPeerRequest, reader).unwrap();
            let suffix: u32 = load!(u32, reader).unwrap();
            assert_eq!(suffix, SUFFIX, "frame misalignment in `{input}`");

            assert_eq!(decoded.peer_address, original.peer_address);
            assert_eq!(decoded.is_permanent, original.is_permanent);

            let mut buffer2 = Vec::new();
            {
                let writer = &mut buffer2;
                store!(u32, &PREFIX, writer).unwrap();
                serialize!(AddPeerRequest, &decoded, writer).unwrap();
                store!(u32, &SUFFIX, writer).unwrap();
            }
            assert_eq!(buffer1, buffer2, "second emit must be byte-identical for `{input}`");
        }
    }

    /// A v2 frame whose string body fails [`RpcPeerEndpoint::from_str`]
    /// surfaces a typed [`RpcError::InvalidPeerEndpoint`] as the
    /// [`std::io::Error::source`]. Downstream wRPC consumers can
    /// downcast the structured variant -- matching the gRPC edge's
    /// `try_from!` mapping at the rpc-core boundary.
    #[test]
    fn add_peer_request_v2_invalid_endpoint_yields_typed_rpc_error() {
        use crate::error::RpcError;

        // Leading whitespace fails RFC 1123 strict validation.
        let invalid = " not a valid host";
        let mut bytes = Vec::new();
        store!(u16, &ADD_PEER_REQUEST_BORSH_V2, &mut bytes).unwrap();
        store!(String, &invalid.to_string(), &mut bytes).unwrap();
        store!(bool, &true, &mut bytes).unwrap();

        let mut cursor = std::io::Cursor::new(&bytes);
        let err = <AddPeerRequest as Deserializer>::deserialize(&mut cursor).expect_err("invalid hostname must fail to deserialize");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);

        let rpc_err =
            err.get_ref().and_then(|inner| inner.downcast_ref::<RpcError>()).expect("io::Error source must downcast to RpcError");
        match rpc_err {
            RpcError::InvalidPeerEndpoint { endpoint, reason } => {
                assert_eq!(endpoint, invalid, "endpoint field must carry the bad input verbatim");
                assert!(!reason.is_empty(), "reason field must be non-empty");
            }
            other => panic!("expected RpcError::InvalidPeerEndpoint, got {other:?}"),
        }

        // Display chains through to the canonical message text -- byte-
        // for-byte equivalent to the format!() literal the prior cycle
        // produced, so message-text consumers stay unaffected.
        let msg = err.to_string();
        assert!(msg.starts_with("invalid peer endpoint"), "Display chain mismatch: {msg}");
        assert!(msg.contains(invalid), "Display chain must include the bad endpoint: {msg}");
    }
}
