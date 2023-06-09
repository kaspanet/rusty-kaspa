use std::sync::Arc;

use kaspa_consensus_core::{tx::Transaction, BlockHasher};
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

pub trait BlockTransactionsStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<Transaction>>, StoreError>;
}

pub trait BlockTransactionsStore: BlockTransactionsStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `BlockTransactionsStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbBlockTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<Vec<Transaction>>, BlockHasher>,
}

impl DbBlockTransactionsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, DatabaseStorePrefixes::BlockTransactions.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, transactions)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl BlockTransactionsStoreReader for DbBlockTransactionsStore {
    fn get(&self, hash: Hash) -> Result<Arc<Vec<Transaction>>, StoreError> {
        self.access.read(hash)
    }
}

impl BlockTransactionsStore for DbBlockTransactionsStore {
    fn insert(&self, hash: Hash, transactions: Arc<Vec<Transaction>>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, transactions)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
