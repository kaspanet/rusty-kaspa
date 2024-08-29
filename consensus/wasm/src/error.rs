use thiserror::Error;
use wasm_bindgen::{JsError, JsValue};
use workflow_wasm::jserror::JsErrorData;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error(transparent)]
    JsValue(JsErrorData),
    // JsValue(String),
    #[error(transparent)]
    WasmError(#[from] workflow_wasm::error::Error),

    #[error(transparent)]
    ScriptBuilderError(#[from] kaspa_txscript::script_builder::ScriptBuilderError),

    #[error("{0}")]
    ParseIntError(#[from] std::num::ParseIntError),

    #[error(transparent)]
    FasterHexError(#[from] faster_hex::Error),

    #[error("invalid transaction outpoint: {0}")]
    InvalidTransactionOutpoint(String),

    #[error(transparent)]
    Secp256k1(#[from] secp256k1::Error),

    #[error(transparent)]
    Sign(#[from] kaspa_consensus_core::sign::Error),

    #[error(transparent)]
    SerdeWasmBindgen(JsErrorData),

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),

    #[error(transparent)]
    NetworkTypeError(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error(transparent)]
    ConsensusClient(#[from] kaspa_consensus_client::error::Error),
}

// unsafe impl Send for Error {}
// unsafe impl Sync for Error {}

impl Error {
    pub fn custom<T: Into<String>>(msg: T) -> Self {
        Error::Custom(msg.into())
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

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        match value {
            Error::JsValue(js_error_data) => js_error_data.into(),
            _ => JsValue::from(value.to_string()),
        }
    }
}

impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        // Self::JsValue(format!("{:?}", err))
        Self::JsValue(err.into())
        // Self::JsValue(Sendable(err.into()))
    }
}

impl From<JsError> for Error {
    fn from(err: JsError) -> Self {
        // Self::JsValue(format!("jserror"))
        Self::JsValue(err.into())
    }
}

impl From<serde_wasm_bindgen::Error> for Error {
    fn from(err: serde_wasm_bindgen::Error) -> Self {
        Self::SerdeWasmBindgen(JsValue::from(err).into())
        // Self::JsValue(err.into())
    }
}
