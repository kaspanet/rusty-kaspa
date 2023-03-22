use crate::processes::ghostdag::ordering::SortableBlock;
use database::prelude::StoreResult;
use database::prelude::DB;
use database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
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
    db: Arc<DB>,
    access: CachedDbItem<SortableBlock>,
}

impl DbHeadersSelectedTipStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), STORE_NAME.to_vec()) }
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
