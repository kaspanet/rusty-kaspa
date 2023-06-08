use kaspa_consensus_core::acceptance_data::AcceptanceData;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub trait AcceptanceDataStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError>;
}

pub trait AcceptanceDataStore: AcceptanceDataStoreReader {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `DbAcceptanceDataStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbAcceptanceDataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<AcceptanceData>, BlockHasher>,
}

impl DbAcceptanceDataStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, DatabaseStorePrefixes::AcceptanceData.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, acceptance_data)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl AcceptanceDataStoreReader for DbAcceptanceDataStore {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError> {
        self.access.read(hash)
    }
}

impl AcceptanceDataStore for DbAcceptanceDataStore {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, acceptance_data)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
