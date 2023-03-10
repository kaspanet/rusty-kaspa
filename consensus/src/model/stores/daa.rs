use std::sync::Arc;

use consensus_core::{BlockHashSet, BlockHasher};
use database::prelude::StoreError;
use database::prelude::DB;
use database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use hashes::Hash;
use rocksdb::WriteBatch;

pub trait DaaStoreReader {
    fn get_mergeset_non_daa(&self, hash: Hash) -> Result<Arc<BlockHashSet>, StoreError>;
}

pub trait DaaStore: DaaStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, mergeset_non_daa: Arc<BlockHashSet>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"mergeset_non_daa";

/// A DB + cache implementation of `DaaStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDaaStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<BlockHashSet>, BlockHasher>,
}

impl DbDaaStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, STORE_PREFIX.to_vec()) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, mergeset_non_daa: Arc<BlockHashSet>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, mergeset_non_daa)?;
        Ok(())
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
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, mergeset_non_daa)?;
        Ok(())
    }
}
