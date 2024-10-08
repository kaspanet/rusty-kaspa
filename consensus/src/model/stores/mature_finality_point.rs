use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `MatureFinalityPointStore`.
pub trait MatureFinalityPointStoreReader {
    fn get(&self) -> StoreResult<Hash>;
}

pub trait MatureFinalityPointStore: MatureFinalityPointStoreReader {
    fn set(&mut self, hash: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `MatureFinalityPointStore` trait
#[derive(Clone)]
pub struct DbMatureFinalityPointStore {
    db: Arc<DB>,
    access: CachedDbItem<Hash>,
}

// This store saves the last known mature finality point.
impl DbMatureFinalityPointStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::MatureFinalityPoint.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, hash: Hash) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &hash)
    }
}

impl MatureFinalityPointStoreReader for DbMatureFinalityPointStore {
    fn get(&self) -> StoreResult<Hash> {
        self.access.read()
    }
}

impl MatureFinalityPointStore for DbMatureFinalityPointStore {
    fn set(&mut self, hash: Hash) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &hash)
    }
}
