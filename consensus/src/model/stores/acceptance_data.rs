use super::{
    caching::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceData {
    // TODO
}

pub trait AcceptanceDataStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError>;
}

pub trait AcceptanceDataStore: AcceptanceDataStoreReader {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"acceptance-data";

/// A DB + cache implementation of `DbAcceptanceDataStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbAcceptanceDataStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbAccess<Hash, AcceptanceData>,
}

impl DbAcceptanceDataStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(BatchDbWriter::new(batch), hash, &acceptance_data)?;
        Ok(())
    }
}

impl AcceptanceDataStoreReader for DbAcceptanceDataStore {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError> {
        self.cached_access.read(hash)
    }
}

impl AcceptanceDataStore for DbAcceptanceDataStore {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(DirectDbWriter::new(&self.raw_db), hash, &acceptance_data)?;
        Ok(())
    }
}
