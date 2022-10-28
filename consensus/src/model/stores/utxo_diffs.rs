use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::StoreError,
    DB,
};
use consensus_core::{utxo::utxo_diff::UtxoDiff, BlockHasher};
use hashes::Hash;
use rocksdb::WriteBatch;

/// Store for holding the UTXO difference (delta) of a block relative to its selected parent.
/// Note that this data is lazy-computed only for blocks which are candidates to being chain
/// blocks. However, once the diff is computed, it is permanent. This store has a relation to
/// block status, such that if a block has status `StatusUTXOValid` then it is expected to have
/// utxo diff data as well as utxo multiset data and acceptance data.

pub trait UtxoDiffsStoreReader {
    fn get(&self, hash: Hash) -> Result<Arc<UtxoDiff>, StoreError>;
}

pub trait UtxoDiffsStore: UtxoDiffsStoreReader {
    fn insert(&self, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"utxo-diffs";

/// A DB + cache implementation of `UtxoDifferencesStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoDiffsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<UtxoDiff>, BlockHasher>,
}

impl DbUtxoDiffsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(BatchDbWriter::new(batch), hash, utxo_diff)?;
        Ok(())
    }
}

impl UtxoDiffsStoreReader for DbUtxoDiffsStore {
    fn get(&self, hash: Hash) -> Result<Arc<UtxoDiff>, StoreError> {
        self.access.read(hash)
    }
}

impl UtxoDiffsStore for DbUtxoDiffsStore {
    fn insert(&self, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, utxo_diff)?;
        Ok(())
    }
}
