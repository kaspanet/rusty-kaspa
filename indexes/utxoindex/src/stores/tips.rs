use std::sync::Arc;

use kaspa_database::{
    prelude::{CachedDbItem, DirectDbWriter, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};

use kaspa_consensus_core::BlockHashSet;

/// Reader API for `UtxoIndexTipsStore`.
pub trait UtxoIndexTipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait UtxoIndexTipsStore: UtxoIndexTipsStoreReader {
    fn set_tips(&mut self, new_tips: BlockHashSet) -> StoreResult<()>;
    fn remove(&mut self) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbUtxoIndexTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbUtxoIndexTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::UtxoIndexTips.into()) }
    }
}

impl UtxoIndexTipsStoreReader for DbUtxoIndexTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.access.read()
    }
}

impl UtxoIndexTipsStore for DbUtxoIndexTipsStore {
    fn set_tips(&mut self, new_tips: BlockHashSet) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), &Arc::new(new_tips))
    }

    fn remove(&mut self) -> Result<(), StoreError> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}
