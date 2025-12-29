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

pub trait StoreResultExtensions<T> {
    /// Unwrap or assert that the error is key not fund in which case `None` is returned
    fn optional(self) -> StoreResult<Option<T>>;
}

impl<T> StoreResultExtensions<T> for StoreResult<T> {
    fn optional(self) -> StoreResult<Option<T>> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
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
            Ok(_) | Err(StoreError::KeyAlreadyExists(_)) | Err(StoreError::HashAlreadyExists(_)) => Ok(()),
            Err(err) => Err(err),
        }
    }
}
