use secp256k1::Error as Secp256k1Error;
use std::sync::PoisonError;
use thiserror::Error;
use wasm_bindgen::prelude::*;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("{0}")]
    Wasm(#[from] workflow_wasm::error::Error),

    #[error("Secp256k1Error error: {0}")]
    Secp256k1Error(#[from] Secp256k1Error),

    #[error("PoisonError error: {0}")]
    PoisonError(String),
}

impl From<Error> for JsValue {
    fn from(err: Error) -> Self {
        JsValue::from_str(&err.to_string())
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Error::Custom(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError(format!("{err:?}"))
    }
}
