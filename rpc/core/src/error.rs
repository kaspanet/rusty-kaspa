use kaspa_consensus_core::{subnets::SubnetworkConversionError, tx::TransactionId};
use kaspa_utils::networking::IpAddress;
use std::{net::AddrParseError, num::TryFromIntError};
use thiserror::Error;
use workflow_core::channel::ChannelError;

use crate::{api::ctl::RpcState, RpcHash, RpcTransactionId, SubmitBlockRejectReason};

#[derive(Clone, Debug, Error)]
pub enum RpcError {
    #[error("Not implemented")]
    NotImplemented,

    #[error("Integer downsize conversion error {0}")]
    IntConversionError(#[from] TryFromIntError),

    #[error("Hex parsing error: {0}")]
    HexParsingError(#[from] faster_hex::Error),

    #[error("Blue work parsing error {0}")]
    RpcBlueWorkTypeParseError(std::num::ParseIntError),

    #[error("Integer parsing error: {0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("Ip address parsing error {0}")]
    ParseIpAddressError(#[from] AddrParseError),

    #[error("Wrong rpc api version format")]
    RpcApiVersionFormatError,

    #[error("Invalid script class: {0}")]
    InvalidRpcScriptClass(String),

    #[error("Missing required field {0}.{1}")]
    MissingRpcFieldError(String, String),

    #[error("Feature not supported")]
    UnsupportedFeature,

    #[error("Primitive to enum conversion error")]
    PrimitiveToEnumConversionError,

    #[error("Coinbase payload is above max length ({0}). Try to shorten the extra data.")]
    CoinbasePayloadLengthAboveMax(usize),

    #[error("Rejected transaction {0}: {1}")]
    RejectedTransaction(RpcTransactionId, String),

    #[error("Block {0} is invalid. No verbose data can be built.")]
    InvalidBlock(RpcHash),

    #[error("If includeTransactions is set, then includeBlockVerboseData must be set as well.")]
    InvalidGetBlocksRequest,

    #[error("Transaction {0} not found")]
    TransactionNotFound(TransactionId),

    #[error("Method unavailable. Run the node with the --utxoindex argument.")]
    NoUtxoIndex,

    #[error("Method unavailable. No connection manager is currently available.")]
    NoConnectionManager,

    #[error("Requested window size {0} is larger than max {1} allowed in RPC safe mode.")]
    WindowSizeExceedingMaximum(u32, u32),

    #[error("Requested window size {0} is larger than pruning point depth {1}.")]
    WindowSizeExceedingPruningDepth(u32, u64),

    #[error("Method unavailable in safe mode. Run the node with --unsaferpc argument.")]
    UnavailableInSafeMode,

    #[error("Cannot ban IP {0} because it has some permanent connection.")]
    IpHasPermanentConnection(IpAddress),

    #[error("IP {0} is not registered as banned.")]
    IpIsNotBanned(IpAddress),

    #[error("Block {0} doesn't have any merger block.")]
    MergerNotFound(RpcHash),

    #[error("Block was not submitted: {0}")]
    SubmitBlockError(SubmitBlockRejectReason),

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),

    #[error(transparent)]
    NetworkTypeError(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error(transparent)]
    NetworkIdError(#[from] kaspa_consensus_core::network::NetworkIdError),

    #[error(transparent)]
    NotificationError(#[from] kaspa_notify::error::Error),

    #[error(transparent)]
    MiningManagerError(#[from] kaspa_mining_errors::manager::MiningManagerError),

    #[error(transparent)]
    ConsensusError(#[from] kaspa_consensus_core::errors::consensus::ConsensusError),

    #[error(transparent)]
    ScriptClassError(#[from] kaspa_txscript::script_class::Error),

    #[error(transparent)]
    NodeIdError(#[from] uuid::Error),

    #[error("RPC Server (remote error) -> {0}")]
    RpcSubsystem(String),

    #[error("{0}")]
    General(String),

    #[error("RpcCtl dispatch error")]
    RpcCtlDispatchError,

    #[error("transaction query must either not filter transactions or include orphans")]
    InconsistentMempoolTxQuery,

    #[error(transparent)]
    SubnetParsingError(#[from] SubnetworkConversionError),

    #[error(transparent)]
    WasmError(#[from] workflow_wasm::error::Error),

    #[error("{0}")]
    SerdeWasmBindgen(String),

    #[error(transparent)]
    ConsensusClient(#[from] kaspa_consensus_client::error::Error),
}

impl From<String> for RpcError {
    fn from(value: String) -> Self {
        RpcError::General(value)
    }
}

impl From<&str> for RpcError {
    fn from(value: &str) -> Self {
        RpcError::General(value.to_string())
    }
}

impl From<ChannelError<RpcState>> for RpcError {
    fn from(_: ChannelError<RpcState>) -> Self {
        RpcError::RpcCtlDispatchError
    }
}

impl From<serde_wasm_bindgen::Error> for RpcError {
    fn from(value: serde_wasm_bindgen::Error) -> Self {
        RpcError::SerdeWasmBindgen(value.to_string())
    }
}

pub type RpcResult<T> = std::result::Result<T, crate::RpcError>;
