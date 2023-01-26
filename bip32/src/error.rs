//! Error type.

use core::fmt::{self, Display};

/// Result type.
pub type Result<T> = core::result::Result<T, Error>;

/// Error type.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum Error {
    /// Base58 errors.
    Base58,

    /// BIP39-related errors.
    Bip39,

    /// Child number-related errors.
    ChildNumber,

    /// Cryptographic errors.
    Crypto,

    /// Decoding errors (not related to Base58).
    Decode,

    /// Maximum derivation depth exceeded.
    Depth,

    /// Seed length invalid.
    SeedLength,

    /// Scalar OutOfRangeError
    ScalarOutOfRangeError,
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Base58 => f.write_str("base58 error"),
            Error::Bip39 => f.write_str("bip39 error"),
            Error::ChildNumber => f.write_str("invalid child number"),
            Error::Crypto => f.write_str("cryptographic error"),
            Error::Decode => f.write_str("decoding error"),
            Error::Depth => f.write_str("maximum derivation depth exceeded"),
            Error::SeedLength => f.write_str("seed length invalid"),
            Error::ScalarOutOfRangeError => f.write_str("scalar bytes length invalid"),
        }
    }
}

#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
impl std::error::Error for Error {}

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
    fn from(_: hmac::digest::InvalidLength) -> Error {
        Error::Crypto
    }
}

impl From<secp256k1_ffi::Error> for Error {
    fn from(_: secp256k1_ffi::Error) -> Error {
        Error::Crypto
    }
}

impl From<secp256k1_ffi::scalar::OutOfRangeError> for Error {
    fn from(_: secp256k1_ffi::scalar::OutOfRangeError) -> Error {
        Error::ScalarOutOfRangeError
    }
}
