use std::sync::Arc;

use kaspa_consensus_core::{BlockHashSet, BlockHasher};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

pub trait DaaStoreReader {
    fn get_mergeset_non_daa(&self, hash: Hash) -> Result<Arc<BlockHashSet>, StoreError>;
}

pub trait DaaStore: DaaStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, mergeset_non_daa: Arc<BlockHashSet>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `DaaStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDaaStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<BlockHashSet>, BlockHasher>,
}

impl DbDaaStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::NonDaaMergeset.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, mergeset_non_daa: Arc<BlockHashSet>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, mergeset_non_daa)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl DaaStoreReader for DbDaaStore {
    fn get_mergeset_non_daa(&self, hash: Hash) -> Result<Arc<BlockHashSet>, StoreError> {
        self.access.read(hash)
    }
}

impl DaaStore for DbDaaStore {
    fn insert(&self, hash: Hash, mergeset_non_daa: Arc<BlockHashSet>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, mergeset_non_daa)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
