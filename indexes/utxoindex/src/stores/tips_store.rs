use std::sync::Arc;

use consensus::model::stores::{
    database::prelude::{CachedDbItem, DirectDbWriter},
    errors::{StoreError, StoreResult},
    DB,
};
use consensus_core::BlockHashSet;

/// Reader API for `UtxoIndexTipsStore`.
pub trait UtxoIndexTipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait UtxoIndexTipsStore: UtxoIndexTipsStoreReader {
    fn add_tips(&mut self, new_tips: BlockHashSet) -> StoreResult<()>;
}

pub const UTXO_INDEXED_TIPS_STORE_NAME: &[u8] = b"utxo-indexed-tips";

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbUtxoIndexTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbUtxoIndexTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), UTXO_INDEXED_TIPS_STORE_NAME) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }
}

impl UtxoIndexTipsStoreReader for DbUtxoIndexTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.access.read()
    }
}

impl UtxoIndexTipsStore for DbUtxoIndexTipsStore {
    fn add_tips(&mut self, new_tips: BlockHashSet) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), &Arc::new(new_tips))
    }
}
