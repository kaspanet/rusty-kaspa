use crate::prelude::DbKey;
use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("key {0} not found in store")]
    KeyNotFound(DbKey),

    #[error("key {0} already exists in store")]
    KeyAlreadyExists(String),

    /// Specialization of key not found for the common `Hash` case.
    /// Added for avoiding the `String` allocation
    #[error("hash {0} already exists in store")]
    HashAlreadyExists(Hash),

    #[error("data inconsistency: {0}")]
    DataInconsistency(String),

    #[error("rocksdb error {0}")]
    DbError(#[from] rocksdb::Error),

    #[error("bincode error {0}")]
    DeserializationError(#[from] Box<bincode::ErrorKind>),
}

pub type StoreResult<T> = std::result::Result<T, StoreError>;

pub trait ErrorTraits {
    fn is_key_not_found(&self) -> bool;
    fn is_key_already_exists(&self) -> bool;
}

impl ErrorTraits for StoreError {
    fn is_key_not_found(&self) -> bool {
        matches!(self, StoreError::KeyNotFound(_))
    }

    fn is_key_already_exists(&self) -> bool {
        matches!(self, StoreError::KeyAlreadyExists(_) | StoreError::HashAlreadyExists(_))
    }
}

pub trait StoreResultExtensions<T, E: ErrorTraits> {
    /// Unwrap or assert that the error is key not fund in which case `None` is returned
    fn optional(self) -> Result<Option<T>, E>;
}

impl<T, E: ErrorTraits> StoreResultExtensions<T, E> for Result<T, E> {
    fn optional(self) -> Result<Option<T>, E> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) if err.is_key_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

pub trait StoreResultEmptyTuple {
    /// Unwrap or assert that the error is key already exists
    fn idempotent(self) -> StoreResult<()>;
}

impl StoreResultEmptyTuple for StoreResult<()> {
    fn idempotent(self) -> StoreResult<()> {
        match self {
            Ok(()) => Ok(()),
            Err(err) if err.is_key_already_exists() => Ok(()),
            Err(err) => Err(err),
        }
    }
}
