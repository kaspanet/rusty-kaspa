use kaspa_bip32::Error as BIP32Error;
use kaspa_rpc_core::RpcError as KaspaRpcError;
use kaspa_wrpc_client::error::Error as KaspaWorkflowRpcError;
use secp256k1::Error as Secp256k1Error;
use std::sync::PoisonError;
use wasm_bindgen::JsValue;
use workflow_rpc::client::error::Error as RpcError;

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

    #[error("Secp256k1Error error: {0}")]
    Secp256k1Error(#[from] Secp256k1Error),
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
