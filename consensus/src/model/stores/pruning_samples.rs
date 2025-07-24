use std::sync::Arc;

use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

pub trait PruningSamplesStoreReader {
    fn pruning_sample_from_pov(&self, hash: Hash) -> Result<Hash, StoreError>;
}

pub trait PruningSamplesStore: PruningSamplesStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, pruning_sample_from_pov: Hash) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `PruningSamplesStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbPruningSamplesStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Hash, BlockHasher>,
}

impl DbPruningSamplesStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::PruningSamples.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, pruning_sample_from_pov: Hash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, pruning_sample_from_pov)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl PruningSamplesStoreReader for DbPruningSamplesStore {
    fn pruning_sample_from_pov(&self, hash: Hash) -> Result<Hash, StoreError> {
        self.access.read(hash)
    }
}

impl PruningSamplesStore for DbPruningSamplesStore {
    fn insert(&self, hash: Hash, pruning_sample_from_pov: Hash) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, pruning_sample_from_pov)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
