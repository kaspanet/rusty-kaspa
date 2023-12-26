//! Error type.

use core::fmt::{self, Display};
use core::str::Utf8Error;
use std::sync::PoisonError;
use thiserror::Error;
use wasm_bindgen::JsValue;

/// Result type.
pub type Result<T> = core::result::Result<T, Error>;
pub type ResultConst<T> = core::result::Result<T, ErrorImpl>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorImpl {
    /// validate_str: Invalid length
    DecodeInvalidLength,

    /// validate_str: Invalid str
    DecodeInvalidStr,
}

impl Display for ErrorImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorImpl::DecodeInvalidStr => f.write_str("decoding error"),
            ErrorImpl::DecodeInvalidLength => f.write_str("decoding error"),
        }
    }
}

/// Error type.
#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Bip32 -> {0}")]
    String(String),

    /// Base58 errors.
    #[error("Base58Encode -> {0}")]
    Base58Encode(bs58::encode::Error),

    /// Base58 errors.
    #[error("Base58Decode -> {0}")]
    Base58Decode(bs58::decode::Error),

    /// BIP39-related errors.
    #[error("Bip39 error")]
    Bip39,

    /// Hmac-related errors.
    #[error("HMAC -> {0}")]
    Hmac(hmac::digest::InvalidLength),

    /// Child number-related errors.
    #[error("Invalid child number")]
    ChildNumber,

    /// Cryptographic errors.
    #[error("Secp256k1 -> {0}")]
    Crypto(#[from] secp256k1::Error),

    /// Decoding errors (not related to Base58).
    #[error("Decoding(TryFromSlice) -> {0}")]
    Decode(#[from] core::array::TryFromSliceError),

    /// Decoding errors (not related to Base58).
    #[error("Decoding(Length) -> {0}")]
    DecodeLength(usize, usize),

    /// Decoding errors (not related to Base58).
    #[error("DecodeIssue error")]
    DecodeIssue,

    /// Maximum derivation depth exceeded.
    #[error("Maximum derivation depth exceeded")]
    Depth,

    /// Seed length invalid.
    #[error("Invalid seed length")]
    SeedLength,

    /// Scalar OutOfRangeError
    #[error("Scalar bytes length invalid : {0}")]
    ScalarOutOfRangeError(#[from] secp256k1::scalar::OutOfRangeError),

    /// Utf8Error
    #[error("Utf8Error -> {0}")]
    Utf8Error(#[from] Utf8Error),

    #[error("Poison error -> {0:?}")]
    PoisonError(String),

    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),

    #[error("Mnemonic word count is not supported ({0})")]
    WordCount(usize),
}

impl From<ErrorImpl> for Error {
    fn from(err: ErrorImpl) -> Error {
        Error::String(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError(format!("{err:?}"))
    }
}

impl From<bs58::encode::Error> for Error {
    fn from(e: bs58::encode::Error) -> Error {
        Error::Base58Encode(e)
    }
}

impl From<bs58::decode::Error> for Error {
    fn from(e: bs58::decode::Error) -> Error {
        Error::Base58Decode(e)
    }
}

impl From<hmac::digest::InvalidLength> for Error {
    fn from(e: hmac::digest::InvalidLength) -> Error {
        Error::Hmac(e)
    }
}

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        JsValue::from(value.to_string())
    }
}
