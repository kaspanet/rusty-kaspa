use super::{caching::CachedDbItem, errors::StoreResult, DB};
use crate::processes::ghostdag::ordering::SortableBlock;
use rocksdb::WriteBatch;
use std::sync::Arc;

/// Reader API for `SelectedTipStore`.
pub trait SelectedTipStoreReader {
    fn get(&self) -> StoreResult<SortableBlock>;
}

pub trait SelectedTipStore: SelectedTipStoreReader {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()>;
}

/// A DB + cache implementation of `SelectedTipStore` trait
#[derive(Clone)]
pub struct DbSelectedTipStore {
    raw_db: Arc<DB>,
    prefix: &'static [u8],
    cached_access: CachedDbItem<SortableBlock>,
}

impl DbSelectedTipStore {
    pub fn new(db: Arc<DB>, prefix: &'static [u8]) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbItem::new(db.clone(), prefix), prefix }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db), self.prefix)
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, block: SortableBlock) -> StoreResult<()> {
        self.cached_access.write_batch(batch, &block)
    }
}

impl SelectedTipStoreReader for DbSelectedTipStore {
    fn get(&self) -> StoreResult<SortableBlock> {
        self.cached_access.read()
    }
}

impl SelectedTipStore for DbSelectedTipStore {
    fn set(&mut self, block: SortableBlock) -> StoreResult<()> {
        self.cached_access.write(&block)
    }
}
