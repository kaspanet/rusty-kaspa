use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::{tx::Transaction, BlockHasher};
use hashes::Hash;
use rocksdb::WriteBatch;

pub trait BlockTransactionsStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<Transaction>>, StoreError>;
}

pub trait BlockTransactionsStore: BlockTransactionsStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"block-transactions";

/// A DB + cache implementation of `BlockTransactionsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbBlockTransactionsStore {
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    cached_access: CachedDbAccess<Hash, Vec<Transaction>, BlockHasher>,
}

impl DbBlockTransactionsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(BatchDbWriter::new(batch), hash, &transactions)?;
        Ok(())
    }
}

impl BlockTransactionsStoreReader for DbBlockTransactionsStore {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<Transaction>>, StoreError> {
        self.cached_access.read(hash)
    }
}

impl BlockTransactionsStore for DbBlockTransactionsStore {
    fn insert(&self, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(DirectDbWriter::new(&self.raw_db), hash, &transactions)?;
        Ok(())
    }
}
