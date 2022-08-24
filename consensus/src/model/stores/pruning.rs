use std::sync::Arc;

use super::{caching::CachedDbItem, errors::StoreResult, DB};
use hashes::Hash;

/// Reader API for `PruningStore`.
pub trait PruningStoreReader {
    fn pruning_point(&self) -> StoreResult<Hash>;
}

const STORE_PREFIX: &[u8] = b"pruning";

/// A DB + cache implementation of `StatusesStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningStore {
    raw_db: Arc<DB>,
    pruning_point: CachedDbItem<Hash>,
}

const PRUNING_POINT_KEY: &[u8] = b"pruning-point";

impl DbPruningStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { raw_db: Arc::clone(&db), pruning_point: CachedDbItem::new(db, PRUNING_POINT_KEY) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self {
            raw_db: Arc::clone(&self.raw_db),
            pruning_point: CachedDbItem::new(Arc::clone(&self.raw_db), PRUNING_POINT_KEY),
        }
    }
}

impl PruningStoreReader for DbPruningStore {
    fn pruning_point(&self) -> StoreResult<Hash> {
        self.pruning_point.read()
    }
}
