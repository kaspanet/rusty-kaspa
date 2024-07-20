//!
//! Error types used by the wallet framework.
//!

use kaspa_bip32::Error as BIP32Error;
use std::sync::PoisonError;
use thiserror::Error;
use wasm_bindgen::JsValue;
use workflow_core::sendable::*;
use workflow_wasm::jserror::*;
use workflow_wasm::printable::*;

/// [`Error`](enum@Error) variants emitted by the wallet framework.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("Bip32 -> {0}")]
    BIP32Error(#[from] BIP32Error),

    #[error("Decoding -> {0}")]
    Decode(#[from] core::array::TryFromSliceError),

    #[error("Poison error -> {0}")]
    PoisonError(String),

    #[error("Secp256k1 -> {0}")]
    Secp256k1Error(#[from] secp256k1::Error),

    #[error("{0}")]
    JsValue(JsErrorData),

    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),

    #[error("Serde WASM bindgen -> {0}")]
    SerdeWasmBindgen(Sendable<Printable>),

    #[error("Invalid account type (must be one of: bip32|multisig|legacy")]
    InvalidAccountKind,

    #[error("Invalid XPrv (must be a string or an instance of XPrv)")]
    InvalidXPrv,

    #[error("Invalid XPub (must be a string or an instance of XPub)")]
    InvalidXPub,

    #[error("Invalid PrivateKey (must be a string or an instance of PrivateKey)")]
    InvalidPrivateKey,

    #[error("Invalid PublicKey (must be a string or an instance of PrivateKey)")]
    InvalidPublicKey,

    #[error("Invalid PublicKey Array (must be string[] or PrivateKey[])")]
    InvalidPublicKeyArray,

    #[error(transparent)]
    NetworkId(#[from] kaspa_consensus_core::network::NetworkIdError),

    #[error(transparent)]
    NetworkType(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error("Invalid UTF-8 sequence")]
    Utf8(#[from] std::str::Utf8Error),
}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
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
