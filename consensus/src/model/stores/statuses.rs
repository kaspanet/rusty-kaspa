use kaspa_consensus_core::{blockstatus::BlockStatus, BlockHasher};
use kaspa_database::registry::DatabaseStorePrefixes;
use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, DB};
use kaspa_database::prelude::{StoreError, StoreResult};
use kaspa_hashes::Hash;

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
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `StatusesStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbStatusesStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, BlockStatus, BlockHasher>,
}

impl DbStatusesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::Statuses.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, hash: Hash, status: BlockStatus) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), hash, status)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

pub trait StatusesStoreBatchExtensions {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<'_, DbStatusesStore>, StoreError>;
}

impl StatusesStoreBatchExtensions for Arc<RwLock<DbStatusesStore>> {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<'_, DbStatusesStore>, StoreError> {
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

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
