use kaspa_consensus_core::acceptance_data::AcceptanceData;
use kaspa_consensus_core::acceptance_data::AcceptedTxEntry;
use kaspa_consensus_core::acceptance_data::MergesetBlockAcceptanceData;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

pub trait AcceptanceDataStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError>;
}

pub trait AcceptanceDataStore: AcceptanceDataStoreReader {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// Simple wrapper for implementing `MemSizeEstimator`
#[derive(Clone, Serialize, Deserialize)]
struct AcceptanceDataEntry(Arc<AcceptanceData>);

impl MemSizeEstimator for AcceptanceDataEntry {
    fn estimate_mem_bytes(&self) -> usize {
        self.0.iter().map(|l| l.accepted_transactions.len()).sum::<usize>() * size_of::<AcceptedTxEntry>()
            + self.0.len() * size_of::<MergesetBlockAcceptanceData>()
            + size_of::<AcceptanceData>()
            + size_of::<Self>()
    }
}

/// A DB + cache implementation of `DbAcceptanceDataStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbAcceptanceDataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, AcceptanceDataEntry, BlockHasher>,
}

impl DbAcceptanceDataStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::AcceptanceData.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, AcceptanceDataEntry(acceptance_data))?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl AcceptanceDataStoreReader for DbAcceptanceDataStore {
    fn get(&self, hash: Hash) -> Result<Arc<AcceptanceData>, StoreError> {
        Ok(self.access.read(hash)?.0)
    }
}

impl AcceptanceDataStore for DbAcceptanceDataStore {
    fn insert(&self, hash: Hash, acceptance_data: Arc<AcceptanceData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, AcceptanceDataEntry(acceptance_data))?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
