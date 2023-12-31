use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::prelude::{CachePolicy, DB};
use kaspa_database::prelude::{StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

use super::U64Key;

pub trait PastPruningPointsStoreReader {
    fn get(&self, index: u64) -> StoreResult<Hash>;
}

pub trait PastPruningPointsStore: PastPruningPointsStoreReader {
    // This is append only
    fn insert(&self, index: u64, pruning_point: Hash) -> StoreResult<()>;
    fn set(&self, index: u64, pruning_point: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `PastPruningPointsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbPastPruningPointsStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, Hash>,
}

impl DbPastPruningPointsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PastPruningPoints.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, index: u64, pruning_point: Hash) -> Result<(), StoreError> {
        if self.access.has(index.into())? {
            return Err(StoreError::KeyAlreadyExists(index.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), index.into(), pruning_point)?;
        Ok(())
    }
}

impl PastPruningPointsStoreReader for DbPastPruningPointsStore {
    fn get(&self, index: u64) -> StoreResult<Hash> {
        self.access.read(index.into())
    }
}

impl PastPruningPointsStore for DbPastPruningPointsStore {
    fn insert(&self, index: u64, pruning_point: Hash) -> StoreResult<()> {
        if self.access.has(index.into())? {
            return Err(StoreError::KeyAlreadyExists(index.to_string()));
        }
        self.set(index, pruning_point)
    }

    fn set(&self, index: u64, pruning_point: Hash) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), index.into(), pruning_point)?;
        Ok(())
    }
}
