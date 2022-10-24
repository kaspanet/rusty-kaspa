use super::{
    caching::{BatchDbWriter, CachedDbItem, DirectDbWriter},
    errors::StoreResult,
    DB,
};
use crate::processes::ghostdag::ordering::SortableBlock;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `SelectedTipStore`.
pub trait HeadersSelectedTipStoreReader {
    fn get(&self) -> StoreResult<SortableBlock>;
}

pub trait HeadersSelectedTipStore: HeadersSelectedTipStoreReader {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()>;
}

pub const STORE_NAME: &[u8] = b"headers-selected-tip";

/// A DB + cache implementation of `HeadersSelectedTipStore` trait
#[derive(Clone)]
pub struct DbHeadersSelectedTipStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbItem<SortableBlock>,
}

impl DbHeadersSelectedTipStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbItem::new(db.clone(), STORE_NAME) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, block: SortableBlock) -> StoreResult<()> {
        self.cached_access.write(BatchDbWriter::new(batch), &block)
    }
}

impl HeadersSelectedTipStoreReader for DbHeadersSelectedTipStore {
    fn get(&self) -> StoreResult<SortableBlock> {
        self.cached_access.read()
    }
}

impl HeadersSelectedTipStore for DbHeadersSelectedTipStore {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()> {
        self.cached_access.write(DirectDbWriter::new(&self.raw_db), &block)
    }
}
