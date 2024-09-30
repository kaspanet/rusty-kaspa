use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `SelectedTipStore`.
pub trait GoodFinalityPointStoreReader {
    fn get(&self) -> StoreResult<Hash>;
}

pub trait GoodFinalityPointStore: GoodFinalityPointStoreReader {
    fn set(&mut self, hash: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `GoodFinalityPointStore` trait
#[derive(Clone)]
pub struct DbGoodFinalityPointStore {
    db: Arc<DB>,
    access: CachedDbItem<Hash>,
}

impl DbGoodFinalityPointStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::GoodFinalityPoint.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, hash: Hash) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &hash)
    }
}

impl GoodFinalityPointStoreReader for DbGoodFinalityPointStore {
    fn get(&self) -> StoreResult<Hash> {
        self.access.read()
    }
}

impl GoodFinalityPointStore for DbGoodFinalityPointStore {
    fn set(&mut self, hash: Hash) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &hash)
    }
}
