use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbAccessForCopy, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::BlockHasher;
use hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

pub trait DepthStoreReader {
    fn merge_depth_root(&self, hash: Hash) -> Result<Hash, StoreError>;
    fn finality_point(&self, hash: Hash) -> Result<Hash, StoreError>;
}

pub trait DepthStore: DepthStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, merge_depth_root: Hash, finality_point: Hash) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"block-at-depth";

#[derive(Clone, Copy, Serialize, Deserialize)]
struct BlockDepthInfo {
    merge_depth_root: Hash,
    finality_point: Hash,
}

/// A DB + cache implementation of `DepthStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbDepthStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbAccessForCopy<Hash, BlockDepthInfo, BlockHasher>,
}

impl DbDepthStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccessForCopy::new(db, cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        merge_depth_root: Hash,
        finality_point: Hash,
    ) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(BatchDbWriter::new(batch), hash, BlockDepthInfo { merge_depth_root, finality_point })?;
        Ok(())
    }
}

impl DepthStoreReader for DbDepthStore {
    fn merge_depth_root(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.cached_access.read(hash)?.merge_depth_root)
    }

    fn finality_point(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.cached_access.read(hash)?.finality_point)
    }
}

impl DepthStore for DbDepthStore {
    fn insert(&self, hash: Hash, merge_depth_root: Hash, finality_point: Hash) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(DirectDbWriter::new(&self.raw_db), hash, BlockDepthInfo { merge_depth_root, finality_point })?;
        Ok(())
    }
}
