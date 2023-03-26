use base64::DecodeError;
use consensus_core::sign::Error as CoreSignError;
use consensus_core::wasm::error::Error as CoreWasmError;
use kaspa_bip32::Error as BIP32Error;
use kaspa_rpc_core::RpcError as KaspaRpcError;
use kaspa_wrpc_client::error::Error as KaspaWorkflowRpcError;
use secp256k1::Error as Secp256k1Error;
use std::sync::PoisonError;
use wasm_bindgen::JsValue;
use workflow_rpc::client::error::Error as RpcError;
use workflow_wasm::sendable::*;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    #[error("RPC error: {0}")]
    KaspaRpcClientResult(#[from] KaspaRpcError),

    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("RPC error: {0}")]
    KaspaWorkflowRpcError(#[from] KaspaWorkflowRpcError),

    #[error("BIP32 error: {0}")]
    BIP32Error(#[from] BIP32Error),

    #[error("Decoding error: {0}")]
    Decode(#[from] core::array::TryFromSliceError),

    #[error("PoisonError error: {0}")]
    PoisonError(String),

    #[error("Secp256k1 error: {0}")]
    Secp256k1Error(#[from] Secp256k1Error),

    #[error("consensus core WASM error: {0}")]
    CoreWasmError(#[from] CoreWasmError),

    #[error("consensus core sign() error: {0}")]
    CoreSignError(#[from] CoreSignError),

    #[error("SerdeJson error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("No wallet found")]
    NoWalletInStorage,

    #[error("invalid filename: {0}")]
    InvalidFilename(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JsValue error: {0:?}")]
    JsValue(Sendable<wasm_bindgen::JsValue>),

    #[error("Base64 decode error: {0}")]
    DecodeError(#[from] DecodeError),
}

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        JsValue::from(value.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError(format!("{err:?}"))
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::String(err)
    }
}

impl From<wasm_bindgen::JsValue> for Error {
    fn from(err: wasm_bindgen::JsValue) -> Self {
        Self::JsValue(Sendable(err))
    }
}
