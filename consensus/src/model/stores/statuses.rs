use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{
    caching::CachedDbAccessForCopy,
    errors::{StoreError, StoreResult},
    DB,
};
use hashes::Hash;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum BlockStatus {
    /// StatusInvalid indicates that the block is invalid.
    StatusInvalid = 0,

    /// StatusUTXOValid indicates the block is valid from any UTXO related aspects and has passed all the other validations as well.
    StatusUTXOValid,

    /// StatusUTXOPendingVerification indicates that the block is pending verification against its past UTXO-Set, either
    /// because it was not yet verified since the block was never in the selected parent chain, or if the
    /// block violates finality.
    StatusUTXOPendingVerification,

    /// StatusDisqualifiedFromChain indicates that the block is not eligible to be a selected parent.
    StatusDisqualifiedFromChain,

    /// StatusHeaderOnly indicates that the block transactions are not held (pruned or wasn't added yet)
    StatusHeaderOnly,
}

/// Reader API for `StatusesStore`.
pub trait StatusesStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<BlockStatus>;
    fn has(&self, hash: Hash) -> StoreResult<bool>;
}

/// Write API for `StatusesStore`. The set function is deliberately `mut`
/// since status is not append-only and thus needs to be guarded.
/// TODO: can be optimized to avoid the locking if needed.
pub trait StatusesStore: StatusesStoreReader {
    fn set(&mut self, hash: Hash, status: BlockStatus) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"block-statuses";

/// A DB + cache implementation of `StatusesStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbStatusesStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbAccessForCopy<Hash, BlockStatus>,
}

impl DbStatusesStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccessForCopy::new(db, cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&self.raw_db),
            cached_access: CachedDbAccessForCopy::new(Arc::clone(&self.raw_db), cache_size, STORE_PREFIX),
        }
    }
}

pub trait StatusesStoreBatchExtensions {
    fn set_batch(
        &self, batch: &mut WriteBatch, hash: Hash, status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError>;
}

impl StatusesStoreBatchExtensions for Arc<RwLock<DbStatusesStore>> {
    fn set_batch(
        &self, batch: &mut WriteBatch, hash: Hash, status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError> {
        let write_guard = self.write();
        write_guard
            .cached_access
            .write_batch(batch, hash, status)?;
        Ok(write_guard)
    }
}

impl StatusesStoreReader for DbStatusesStore {
    fn get(&self, hash: Hash) -> StoreResult<BlockStatus> {
        self.cached_access.read(hash)
    }

    fn has(&self, hash: Hash) -> StoreResult<bool> {
        self.cached_access.has(hash)
    }
}

impl StatusesStore for DbStatusesStore {
    fn set(&mut self, hash: Hash, status: BlockStatus) -> StoreResult<()> {
        self.cached_access.write(hash, status)
    }
}
