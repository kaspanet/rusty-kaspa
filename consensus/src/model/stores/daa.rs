use std::sync::Arc;

use super::{
    caching::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::blockhash::BlockHashes;
use hashes::Hash;
use rocksdb::WriteBatch;

pub trait DaaStoreReader {
    fn get_daa_added_blocks(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
}

pub trait DaaStore: DaaStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError>;
}

const ADDED_BLOCKS_STORE_PREFIX: &[u8] = b"daa-added-blocks";

/// A DB + cache implementation of `DaaStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDaaStore {
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    cached_daa_added_blocks_access: CachedDbAccess<Hash, Vec<Hash>>,
}

impl DbDaaStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&db),
            cached_daa_added_blocks_access: CachedDbAccess::new(db, cache_size, ADDED_BLOCKS_STORE_PREFIX),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError> {
        if self.cached_daa_added_blocks_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_daa_added_blocks_access.write(BatchDbWriter::new(batch), hash, &added_blocks)?;
        Ok(())
    }
}

impl DaaStoreReader for DbDaaStore {
    fn get_daa_added_blocks(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.cached_daa_added_blocks_access.read(hash)
    }
}

impl DaaStore for DbDaaStore {
    fn insert(&self, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError> {
        if self.cached_daa_added_blocks_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_daa_added_blocks_access.write(DirectDbWriter::new(&self.raw_db), hash, &added_blocks)?;
        Ok(())
    }
}
