use base64::DecodeError;
use faster_hex::Error as FasterHexError;
use kaspa_bip32::Error as BIP32Error;
use kaspa_consensus_core::sign::Error as CoreSignError;
use kaspa_rpc_core::RpcError as KaspaRpcError;
use kaspa_wrpc_client::error::Error as KaspaWorkflowRpcError;
use secp256k1::Error as Secp256k1Error;
use std::sync::PoisonError;
use wasm_bindgen::JsValue;
use workflow_core::abortable::Aborted;
use workflow_core::sendable::*;
use workflow_rpc::client::error::Error as RpcError;
use workflow_wasm::printable::*;

use thiserror::Error;

// use crate::wallet::Events;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("please select an account")]
    AccountSelection,

    #[error("{0}")]
    KaspaRpcClientResult(#[from] KaspaRpcError),

    #[error("wRPC -> {0}")]
    RpcError(#[from] RpcError),

    #[error("Wallet wRPC -> {0}")]
    KaspaWorkflowRpcError(#[from] KaspaWorkflowRpcError),

    #[error("Bip32 -> {0}")]
    BIP32Error(#[from] BIP32Error),

    #[error("Decoding -> {0}")]
    Decode(#[from] core::array::TryFromSliceError),

    #[error("Poison error -> {0}")]
    PoisonError(String),

    #[error("Secp256k1 -> {0}")]
    Secp256k1Error(#[from] Secp256k1Error),

    #[error("(consensus core sign()) {0}")]
    CoreSignError(#[from] CoreSignError),

    #[error("SerdeJson -> {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("No wallet found")]
    NoWalletInStorage,

    #[error("Wallet already exists")]
    WalletAlreadyExists,

    #[error("This wallet name is not allowed")]
    WalletNameNotAllowed,

    #[error("Wallet is not open")]
    WalletNotOpen,

    #[error("Wallet is not connected")]
    WalletNotConnected,

    #[error("Unable to determine network type, please use `network (mainnet|testnet|testnet-11)` to select a network")]
    MissingNetworkId,

    #[error("Invalif network type - client {0} node {1}")]
    InvalidNetworkType(String, String),

    #[error("Unable to set network type while the wallet is connected")]
    NetworkTypeConnected,

    #[error("{0}")]
    NetworkType(#[from] kaspa_consensus_core::networktype::NetworkTypeError),

    #[error("{0}")]
    NetworkId(#[from] kaspa_consensus_core::networktype::NetworkIdError),

    #[error("The server UTXO index is not enabled")]
    MissingUtxoIndex,

    #[error("Invalid filename: {0}")]
    InvalidFilename(String),

    #[error("(I/O) {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    JsValue(Sendable<Printable>),

    #[error("Base64 decode -> {0}")]
    DecodeError(#[from] DecodeError),

    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),

    #[error(transparent)]
    WorkflowStore(#[from] workflow_store::error::Error),

    #[error(transparent)]
    Address(#[from] kaspa_addresses::AddressError),

    #[error("Serde WASM bindgen -> {0}")]
    SerdeWasmBindgen(Sendable<Printable>),

    #[error("FasterHex -> {0:?}")]
    FasterHexError(#[from] FasterHexError),

    #[error("data decryption error (chacha20poly1305 -> {0})")]
    Chacha20poly1305(chacha20poly1305::Error),

    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    ScriptBuilderError(#[from] kaspa_txscript::script_builder::ScriptBuilderError),

    #[error("argon2 -> {0}")]
    Argon2(argon2::Error),

    #[error("argon2::password_hash -> {0}")]
    Argon2ph(argon2::password_hash::Error),

    #[error(transparent)]
    VarError(#[from] std::env::VarError),

    #[error("private key {0} not found")]
    PrivateKeyNotFound(String),

    #[error("private key {0} already exists")]
    PrivateKeyAlreadyExists(String),

    #[error("invalid key id: {0}")]
    KeyId(String),

    #[error("wallet secret is required")]
    WalletSecretRequired,

    #[error("task aborted")]
    Aborted,

    #[error("{0}")]
    TryFromEnum(#[from] workflow_core::enums::TryFromError),

    #[error("Invalid account type (must be one of: bip32|multisig|legacy")]
    InvalidAccountKind,

    #[error("Insufficient funds")]
    InsufficientFunds,

    #[error(transparent)]
    Utf8Error(#[from] std::str::Utf8Error),

    #[error("invalid transaction outpoint: {0}")]
    InvalidTransactionOutpoint(String),

    #[error("{0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("Receiving duplicate UTXO entry")]
    DuplicateUtxoEntry,

    #[error("{0}")]
    ToValue(String),

    #[error("The feature is not supported")]
    NotImplemented,
}

impl From<Aborted> for Error {
    fn from(_value: Aborted) -> Self {
        Self::Aborted
    }
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
    }
}

impl From<chacha20poly1305::Error> for Error {
    fn from(e: chacha20poly1305::Error) -> Self {
        Error::Chacha20poly1305(e)
    }
}

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        match value {
            Error::JsValue(js_value) => js_value.as_ref().as_ref().clone(),
            _ => JsValue::from(value.to_string()),
        }
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError(format!("{err:?}"))
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Custom(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::Custom(err.to_string())
    }
}

impl From<wasm_bindgen::JsValue> for Error {
    fn from(err: wasm_bindgen::JsValue) -> Self {
        Self::JsValue(Sendable(err.into()))
    }
}

impl From<wasm_bindgen::JsError> for Error {
    fn from(err: wasm_bindgen::JsError) -> Self {
        Self::JsValue(Sendable(err.into()))
    }
}

impl From<serde_wasm_bindgen::Error> for Error {
    fn from(err: serde_wasm_bindgen::Error) -> Self {
        Self::SerdeWasmBindgen(Sendable(Printable::new(err.into())))
    }
}

impl From<argon2::Error> for Error {
    fn from(err: argon2::Error) -> Self {
        Self::Argon2(err)
    }
}

impl From<argon2::password_hash::Error> for Error {
    fn from(err: argon2::password_hash::Error) -> Self {
        Self::Argon2ph(err)
    }
}

impl From<workflow_wasm::tovalue::Error> for Error {
    fn from(err: workflow_wasm::tovalue::Error) -> Self {
        Self::ToValue(err.to_string())
    }
}
