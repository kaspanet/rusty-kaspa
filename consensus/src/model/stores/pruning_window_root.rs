use kaspa_consensus_core::BlockHasher;
use kaspa_database::registry::DatabaseStorePrefixes;
use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, DB};
use kaspa_database::prelude::{StoreError, StoreResult};
use kaspa_hashes::Hash;

/// Reader API for `PruningWindowRootStore`.
pub trait PruningWindowRootStoreReader {
    fn get(&self, pruning_point: Hash) -> StoreResult<Hash>;
    fn has(&self, pruning_point: Hash) -> StoreResult<bool>;
}

/// Write API for `PruningWindowRootStore`. The set function is deliberately `mut`
/// since pruning window root is not append-only and thus needs to be guarded.
/// TODO: can be optimized to avoid the locking if needed.
pub trait PruningWindowRootStore: PruningWindowRootStoreReader {
    fn set(&mut self, pruning_point: Hash, root: Hash) -> StoreResult<()>;
    fn delete(&self, pruning_point: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `PruningWindowRootStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningWindowRootStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Hash, BlockHasher>,
}

impl DbPruningWindowRootStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PruningWindowRoot.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, pruning_point: Hash, root: Hash) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), pruning_point, root)
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, pruning_point: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), pruning_point)
    }
}

pub trait PruningWindowRootStoreBatchExtensions {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        pruning_point: Hash,
        root: Hash,
    ) -> Result<RwLockWriteGuard<DbPruningWindowRootStore>, StoreError>;
}

impl PruningWindowRootStoreBatchExtensions for Arc<RwLock<DbPruningWindowRootStore>> {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        pruning_point: Hash,
        root: Hash,
    ) -> Result<RwLockWriteGuard<DbPruningWindowRootStore>, StoreError> {
        let write_guard = self.write();
        write_guard.access.write(BatchDbWriter::new(batch), pruning_point, root)?;
        Ok(write_guard)
    }
}

impl PruningWindowRootStoreReader for DbPruningWindowRootStore {
    fn get(&self, pruning_point: Hash) -> StoreResult<Hash> {
        self.access.read(pruning_point)
    }

    fn has(&self, pruning_point: Hash) -> StoreResult<bool> {
        self.access.has(pruning_point)
    }
}

impl PruningWindowRootStore for DbPruningWindowRootStore {
    fn set(&mut self, pruning_point: Hash, root: Hash) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), pruning_point, root)
    }

    fn delete(&self, pruning_point: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), pruning_point)
    }
}
