use crate::processes::ghostdag::ordering::SortableBlock;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `SelectedTipStore`.
pub trait HeadersSelectedTipStoreReader {
    fn get(&self) -> StoreResult<SortableBlock>;
}

pub trait HeadersSelectedTipStore: HeadersSelectedTipStoreReader {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()>;
}

/// A DB + cache implementation of `HeadersSelectedTipStore` trait
#[derive(Clone)]
pub struct DbHeadersSelectedTipStore {
    db: Arc<DB>,
    access: CachedDbItem<SortableBlock>,
}

impl DbHeadersSelectedTipStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::HeadersSelectedTip.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, block: SortableBlock) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &block)
    }
}

impl HeadersSelectedTipStoreReader for DbHeadersSelectedTipStore {
    fn get(&self) -> StoreResult<SortableBlock> {
        self.access.read()
    }
}

impl HeadersSelectedTipStore for DbHeadersSelectedTipStore {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &block)
    }
}
