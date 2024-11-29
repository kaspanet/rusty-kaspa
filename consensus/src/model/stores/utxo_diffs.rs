use std::sync::Arc;

use kaspa_consensus_core::{utxo::utxo_diff::UtxoDiff, BlockHasher};
use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::StoreError;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
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
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `UtxoDifferencesStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbUtxoDiffsStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<UtxoDiff>, BlockHasher>,
}

impl DbUtxoDiffsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoDiffs.into()) }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, utxo_diff: Arc<UtxoDiff>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, utxo_diff)?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(BatchDbWriter::new(batch), hash)
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
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(DirectDbWriter::new(&self.db), hash, utxo_diff)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }
}
