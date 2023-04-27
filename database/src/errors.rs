use crate::prelude::DbKey;
use kaspa_hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("key {0} not found in store")]
    KeyNotFound(DbKey),

    #[error("key {0} already exists in store")]
    KeyAlreadyExists(String),

    #[error("hash {0} already exists in store")]
    HashAlreadyExists(Hash),

    #[error("rocksdb error {0}")]
    DbError(#[from] rocksdb::Error),

    #[error("bincode error {0}")]
    DeserializationError(#[from] Box<bincode::ErrorKind>),
}

pub type StoreResult<T> = std::result::Result<T, StoreError>;

pub trait StoreResultExtensions<T> {
    /// Unwrap and convert key not fund error to None
    fn unwrap_option(self) -> Option<T>;
}

impl<T> StoreResultExtensions<T> for StoreResult<T> {
    fn unwrap_option(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(err) => panic!("Unexpected store error: {err:?}"),
        }
    }
}

pub trait StoreResultEmptyTuple {
    fn unwrap_and_ignore_key_already_exists(self);
}

impl StoreResultEmptyTuple for StoreResult<()> {
    fn unwrap_and_ignore_key_already_exists(self) {
        match self {
            Ok(_) => (),
            Err(StoreError::KeyAlreadyExists(_)) | Err(StoreError::HashAlreadyExists(_)) => (),
            Err(err) => panic!("Unexpected store error: {err:?}"),
        }
    }
}
