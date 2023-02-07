use consensus_core::{blockstatus::BlockStatus, BlockHasher};
use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use std::sync::Arc;

use database::db::DB;
use database::errors::{StoreError, StoreResult};
use database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use hashes::Hash;

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
    db: Arc<DB>,
    access: CachedDbAccess<Hash, BlockStatus, BlockHasher>,
}

impl DbStatusesStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, STORE_PREFIX.to_vec()) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }
}

pub trait StatusesStoreBatchExtensions {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError>;
}

impl StatusesStoreBatchExtensions for Arc<RwLock<DbStatusesStore>> {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError> {
        let write_guard = self.write();
        write_guard.access.write(BatchDbWriter::new(batch), hash, status)?;
        Ok(write_guard)
    }
}

impl StatusesStoreReader for DbStatusesStore {
    fn get(&self, hash: Hash) -> StoreResult<BlockStatus> {
        self.access.read(hash)
    }

    fn has(&self, hash: Hash) -> StoreResult<bool> {
        self.access.has(hash)
    }
}

impl StatusesStore for DbStatusesStore {
    fn set(&mut self, hash: Hash, status: BlockStatus) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), hash, status)
    }
}
