use thiserror::Error;
use wasm_bindgen::JsError;
use wasm_bindgen::JsValue;
use workflow_core::channel::ChannelError;
use workflow_core::sendable::*;
use workflow_http::error::Error as HttpError;
use workflow_rpc::client::error::Error as RpcError;
use workflow_rpc::client::error::WebSocketError;
use workflow_wasm::error::Error as WasmError;
use workflow_wasm::printable::*;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("wRPC address error -> {0}")]
    UrlError(String),

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

    #[error("Serde WASM bindgen serialization or deserialization error: {0}")]
    SerdeWasmBindgen(Sendable<Printable>),

    #[error("{0}")]
    JsValue(Sendable<Printable>),

    #[error("{0}")]
    ToValue(String),

    #[error("invalid network type: {0}")]
    NetworkType(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error(transparent)]
    ConsensusWasm(#[from] kaspa_consensus_wasm::error::Error),

    #[error(transparent)]
    HttpError(#[from] HttpError),

    #[error(transparent)]
    WasmError(#[from] WasmError),

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    NetworkId(#[from] kaspa_consensus_core::network::NetworkIdError),
}

impl Error {
    pub fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
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

// impl From<workflow_wasm::serde::Error> for Error {
//     fn from(err: workflow_wasm::serde::Error) -> Self {
//         Self::ToValue(err.to_string())
//     }
// }
