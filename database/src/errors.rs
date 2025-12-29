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

/// Predicates that classify store errors into common semantic buckets.
///
/// This is used by result extension methods (e.g. `optional`, `idempotent`)
/// to treat certain expected error conditions as benign outcomes.
pub trait StoreErrorPredicates {
    /// Returns `true` if this error represents a missing entry (e.g. key not found).
    fn is_key_not_found(&self) -> bool;

    /// Returns `true` if this error represents a duplicate write (e.g. key/hash already exists).
    fn is_already_exists(&self) -> bool;
}

impl StoreErrorPredicates for StoreError {
    fn is_key_not_found(&self) -> bool {
        matches!(self, StoreError::KeyNotFound(_))
    }

    fn is_already_exists(&self) -> bool {
        matches!(self, StoreError::KeyAlreadyExists(_) | StoreError::HashAlreadyExists(_))
    }
}

/// Extension methods for store results.
pub trait StoreResultExt<T, E: StoreErrorPredicates> {
    /// Converts a "key not found" error into absence.
    ///
    /// Mapping:
    /// - `Ok(v)` -> `Ok(Some(v))`
    /// - `Err(e)` where `e.is_key_not_found()` -> `Ok(None)`
    /// - any other `Err(e)` -> `Err(e)`
    ///
    /// This method does **not** panic.
    fn optional(self) -> Result<Option<T>, E>;
}

impl<T, E: StoreErrorPredicates> StoreResultExt<T, E> for Result<T, E> {
    fn optional(self) -> Result<Option<T>, E> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(err) if err.is_key_not_found() => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Extension methods for unit (`()`) store results, typically produced by write operations.
pub trait StoreResultUnitExt<E: StoreErrorPredicates> {
    /// Treats a duplicate-write error as success, making the operation idempotent.
    ///
    /// Mapping:
    /// - `Ok(())` -> `Ok(())`
    /// - `Err(e)` where `e.is_already_exists()` -> `Ok(())`
    /// - any other `Err(e)` -> `Err(e)`
    ///
    /// This method does **not** panic.
    fn idempotent(self) -> Result<(), E>;
}

impl<E: StoreErrorPredicates> StoreResultUnitExt<E> for Result<(), E> {
    fn idempotent(self) -> Result<(), E> {
        match self {
            Ok(()) => Ok(()),
            Err(err) if err.is_already_exists() => Ok(()),
            Err(err) => Err(err),
        }
    }
}
