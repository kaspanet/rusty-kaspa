use std::sync::Arc;

use database::prelude::DB;
use database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use database::prelude::{StoreError, StoreResult};
use hashes::Hash;
use rocksdb::WriteBatch;

use super::U64Key;

pub trait PastPruningPointsStoreReader {
    fn get(&self, index: u64) -> StoreResult<Hash>;
}

pub trait PastPruningPointsStore: PastPruningPointsStoreReader {
    // This is append only
    fn insert(&self, index: u64, pruning_point: Hash) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"past-pruning-points";

/// A DB + cache implementation of `PastPruningPointsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbPastPruningPointsStore {
    db: Arc<DB>,
    access: CachedDbAccess<U64Key, Hash>,
}

impl DbPastPruningPointsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX.to_vec()) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
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
        self.access.write(DirectDbWriter::new(&self.db), index.into(), pruning_point)?;
        Ok(())
    }
}
