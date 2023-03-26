use std::sync::Arc;

use database::prelude::{CachedDbItem, DirectDbWriter, StoreError, StoreResult, DB};

use consensus_core::BlockHashSet;

/// Reader API for `UtxoIndexTipsStore`.
pub trait UtxoIndexTipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait UtxoIndexTipsStore: UtxoIndexTipsStoreReader {
    fn set_tips(&mut self, new_tips: BlockHashSet) -> StoreResult<()>;

    fn remove(&mut self) -> Result<(), StoreError>;
}

pub const TIPS_STORE_PREFIX: &[u8] = b"tips";

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbUtxoIndexTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbUtxoIndexTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), TIPS_STORE_PREFIX.to_vec()) }
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
