use thiserror::Error;
use wasm_bindgen::JsError;
use wasm_bindgen::JsValue;
use workflow_core::channel::ChannelError;
use workflow_core::sendable::*;
use workflow_rpc::client::error::Error as RpcError;
use workflow_rpc::client::error::WebSocketError;
use workflow_wasm::printable::*;

#[derive(Debug, Error)]
pub enum Error {
    #[error("wRPC address error -> {0}")]
    AddressError(String),

    #[error("wRPC -> {0}")]
    RpcError(#[from] RpcError),

    #[error("Kaspa RpcApi -> {0}")]
    RpcApiError(#[from] kaspa_rpc_core::error::RpcError),

    #[error("Kaspa RpcApi -> {0}")]
    WebSocketError(#[from] WebSocketError),

    #[error("Notification subsystem -> {0}")]
    NotificationError(#[from] kaspa_notify::error::Error),

    #[error("Channel -> {0}")]
    ChannelError(String),

    #[error("Serde WASM bindgen ser/deser error: {0}")]
    SerdeWasmBindgen(Sendable<Printable>),

    #[error("{0}")]
    JsValue(Sendable<Printable>),
}

impl<T> From<ChannelError<T>> for Error {
    fn from(err: ChannelError<T>) -> Self {
        Error::ChannelError(err.to_string())
    }
}

impl From<serde_wasm_bindgen::Error> for Error {
    fn from(err: serde_wasm_bindgen::Error) -> Self {
        Error::SerdeWasmBindgen(Sendable(Printable::new(err.into())))
    }
}

impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        Error::JsValue(Sendable(Printable::new(err)))
    }
}

impl From<JsError> for Error {
    fn from(err: JsError) -> Self {
        Error::JsValue(Sendable(Printable::new(err.into())))
    }
}

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        match value {
            Error::JsValue(err) => err.as_ref().into(),
            _ => JsValue::from(value.to_string()),
        }
    }
}
