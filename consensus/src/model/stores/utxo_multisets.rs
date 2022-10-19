use super::{caching::CachedDbAccessForCopy, errors::StoreError, DB};
use hashes::Hash;
use math::Uint3072;
use muhash::MuHash;
use rocksdb::WriteBatch;
use std::sync::Arc;

pub trait UtxoMultisetsStoreReader {
    fn get(&self, hash: Hash) -> Result<MuHash, StoreError>;
}

pub trait UtxoMultisetsStore: UtxoMultisetsStoreReader {
    fn insert(&self, hash: Hash, multiset: MuHash) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"utxo-multisets";

/// A DB + cache implementation of `DbUtxoMultisetsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoMultisetsStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbAccessForCopy<Hash, Uint3072>,
}

impl DbUtxoMultisetsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccessForCopy::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, multiset: MuHash) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write_batch(batch, hash, multiset.try_into().expect("multiset is expected to be finalized"))?;
        Ok(())
    }
}

impl UtxoMultisetsStoreReader for DbUtxoMultisetsStore {
    fn get(&self, hash: Hash) -> Result<MuHash, StoreError> {
        Ok(self.cached_access.read(hash)?.into())
    }
}

impl UtxoMultisetsStore for DbUtxoMultisetsStore {
    fn insert(&self, hash: Hash, multiset: MuHash) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(hash, multiset.try_into().expect("multiset is expected to be finalized"))?;
        Ok(())
    }
}
