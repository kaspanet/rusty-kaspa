use base64::DecodeError;
use downcast::DowncastError;
use kaspa_bip32::Error as BIP32Error;
use kaspa_consensus_core::sign::Error as CoreSignError;
use kaspa_rpc_core::RpcError as KaspaRpcError;
use kaspa_wrpc_client::error::Error as KaspaWorkflowRpcError;
use std::sync::PoisonError;
use wasm_bindgen::JsValue;
use workflow_core::abortable::Aborted;
use workflow_core::sendable::*;
use workflow_rpc::client::error::Error as RpcError;
use workflow_wasm::jserror::*;
use workflow_wasm::printable::*;

use thiserror::Error;

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
    Secp256k1Error(#[from] secp256k1::Error),

    #[error("(consensus core sign()) {0}")]
    CoreSignError(#[from] CoreSignError),

    #[error("SerdeJson -> {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("No wallet named '{0}' found")]
    NoWalletInStorage(String),

    #[error("Wallet already exists")]
    WalletAlreadyExists,

    #[error("This wallet name is not allowed")]
    WalletNameNotAllowed,

    #[error("Wallet is not open")]
    WalletNotOpen,

    #[error("Wallet is not connected")]
    NotConnected,

    #[error("No network selected. Please use `network (mainnet|testnet-10|testnet-11)` to select a network.")]
    MissingNetworkId,

    #[error("Invalid or unsupported network id: {0}")]
    InvalidNetworkId(String),

    #[error("Invalid network type - expected: {0} connected to: {1}")]
    InvalidNetworkType(String, String),

    #[error("Invalid network suffix '{0}'")]
    InvalidNetworkSuffix(String),

    #[error("Network suffix is required for network '{0}'")]
    MissingNetworkSuffix(String),

    #[error("Unexpected extra network suffix '{0}'")]
    UnexpectedExtraSuffixToken(String),

    #[error("Unable to set network type while the wallet is connected")]
    NetworkTypeConnected,

    #[error("{0}")]
    NetworkType(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error("{0}")]
    NetworkId(#[from] kaspa_consensus_core::network::NetworkIdError),

    #[error("The server UTXO index is not enabled")]
    MissingUtxoIndex,

    #[error("Invalid filename: {0}")]
    InvalidFilename(String),

    #[error("(I/O) {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    JsValue(JsErrorData),

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

    #[error(transparent)]
    FasterHexError(#[from] faster_hex::Error),

    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),

    #[error("Unable to decrypt this wallet")]
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

    #[error("xprv key is not supported for this key type")]
    XPrvSupport,

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

    #[error("{0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error("Receiving duplicate UTXO entry")]
    DuplicateUtxoEntry,

    #[error("{0}")]
    ToValue(String),

    #[error("No records found")]
    NoRecordsFound,

    #[error("The feature is not supported")]
    NotImplemented,

    #[error("Not allowed on a resident account")]
    ResidentAccount,

    #[error("This feature is not supported by this account type")]
    AccountKindFeature,

    #[error("Address derivation processing is not supported by this account type")]
    AccountAddressDerivationCaps,

    #[error("{0}")]
    DowncastError(String),

    #[error(transparent)]
    ConsensusWasm(#[from] kaspa_consensus_wasm::error::Error),

    #[error("Fees::Include or Fees::Exclude are not allowed in sweep transactions")]
    GeneratorFeesInSweepTransaction,

    #[error("Change address does not match supplied network type")]
    GeneratorChangeAddressNetworkTypeMismatch,

    #[error("Payment output address does not match supplied network type")]
    GeneratorPaymentOutputNetworkTypeMismatch,

    #[error("Priority fees can not be included into transactions with multiple outputs")]
    GeneratorIncludeFeesRequiresOneOutput,

    #[error("Requested transaction is too heavy")]
    GeneratorTransactionIsTooHeavy,

    #[error(transparent)]
    MultisigCreateError(#[from] kaspa_txscript::MultisigCreateError),

    #[error(transparent)]
    TxScriptError(#[from] kaspa_txscript_errors::TxScriptError),
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
            Error::JsValue(js_error_data) => js_error_data.into(),
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
        Self::JsValue(err.into())
    }
}

impl From<wasm_bindgen::JsError> for Error {
    fn from(err: wasm_bindgen::JsError) -> Self {
        Self::JsValue(err.into())
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

impl From<workflow_wasm::serde::Error> for Error {
    fn from(err: workflow_wasm::serde::Error) -> Self {
        Self::ToValue(err.to_string())
    }
}

impl<T> From<DowncastError<T>> for Error {
    fn from(e: DowncastError<T>) -> Self {
        Error::DowncastError(e.to_string())
    }
}
