use std::sync::Arc;

use super::{caching::CachedDbAccess, errors::StoreError, DB};
use consensus_core::utxo::utxo_diff::UtxoDiff;
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
    raw_db: Arc<DB>,
    cached_access: CachedDbAccess<Hash, UtxoDiff>,
}

impl DbUtxoDiffsStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write_batch(batch, hash, &utxo_diff)?;
        Ok(())
    }
}

impl UtxoDiffsStoreReader for DbUtxoDiffsStore {
    fn get(&self, hash: Hash) -> Result<Arc<UtxoDiff>, StoreError> {
        self.cached_access.read(hash)
    }
}

impl UtxoDiffsStore for DbUtxoDiffsStore {
    fn insert(&self, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(hash, &utxo_diff)?;
        Ok(())
    }
}
