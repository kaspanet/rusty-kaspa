//! Error type.

use core::fmt::{self, Display};
use thiserror::Error;
use core::str::Utf8Error;
use std::sync::PoisonError;

/// Result type.
pub type Result<T> = core::result::Result<T, Error>;
pub type ResultConst<T> = core::result::Result<T, ErrorImpl>;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorImpl {
    /// validate_str: Invalid length
    DecodeInvalidLength,

    /// validate_str: Invalid str
    DecodeInvalidStr
}

impl Display for ErrorImpl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorImpl::DecodeInvalidStr => f.write_str("decoding error"),
            ErrorImpl::DecodeInvalidLength => f.write_str("decoding error")
        }
    }
}

/// Error type.
#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    /// Base58 errors.
    #[error("Base58Encode error: {0}")]
    Base58Encode(#[from] bs58::encode::Error),

    /// Base58 errors.
    #[error("Base58Decode error: {0}")]
    Base58Decode(#[from] bs58::decode::Error),

    /// BIP39-related errors.
    #[error("Bip39 error")]
    Bip39,

    /// Hmac-related errors.
    #[error("HMAC error: {0}")]
    Hmac(#[from] hmac::digest::InvalidLength),

    /// Child number-related errors.
    #[error("Invalid child number")]
    ChildNumber,

    /// Cryptographic errors.
    #[error("Cryptographic error: {0}")]
    Crypto(#[from] secp256k1::Error),

    /// Decoding errors (not related to Base58).
    #[error("Decoding error: {0}")]
    Decode(#[from] core::array::TryFromSliceError),

    /// Decoding errors (not related to Base58).
    #[error("Decoding error: {0}")]
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
    #[error("Utf8Error : {0}")]
    Utf8Error(#[from] Utf8Error),

    #[error("PoisonError: {0:?}")]
    PoisonError(String),
}


impl From<ErrorImpl> for Error {
    fn from(err: ErrorImpl) -> Error {
        Error::String(err.to_string())
    }
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::PoisonError(format!("{:?}", err))
    }
}

//#[cfg(feature = "std")]
//#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
//impl std::error::Error for Error {}

/*
impl From<bs58::decode::Error> for Error {
    fn from(_: bs58::decode::Error) -> Error {
        Error::Base58
    }
}

impl From<bs58::encode::Error> for Error {
    fn from(_: bs58::encode::Error) -> Error {
        Error::Base58
    }
}


impl From<core::array::TryFromSliceError> for Error {
    fn from(_: core::array::TryFromSliceError) -> Error {
        Error::Decode
    }
}


impl From<hmac::digest::InvalidLength> for Error {
    fn from(e: hmac::digest::InvalidLength) -> Error {
        Error::Hmac(e)
    }
}

impl From<secp256k1::Error> for Error {
    fn from(_: secp256k1::Error) -> Error {
        Error::Crypto
    }
}


impl From<secp256k1::scalar::OutOfRangeError> for Error {
    fn from(_: secp256k1::scalar::OutOfRangeError) -> Error {
        Error::ScalarOutOfRangeError
    }
}
*/
