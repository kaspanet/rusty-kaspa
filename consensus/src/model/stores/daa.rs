use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::{blockhash::BlockHashes, BlockHasher};
use hashes::Hash;
use rocksdb::WriteBatch;

pub trait DaaStoreReader {
    fn get_daa_added_blocks(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
}

pub trait DaaStore: DaaStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"daa-added-blocks";

/// A DB + cache implementation of `DaaStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDaaStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, BlockHashes, BlockHasher>,
}

impl DbDaaStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, added_blocks)?;
        Ok(())
    }
}

impl DaaStoreReader for DbDaaStore {
    fn get_daa_added_blocks(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.access.read(hash)
    }
}

impl DaaStore for DbDaaStore {
    fn insert(&self, hash: Hash, added_blocks: BlockHashes) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, added_blocks)?;
        Ok(())
    }
}
