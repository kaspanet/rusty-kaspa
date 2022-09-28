use std::sync::Arc;

use super::{caching::CachedDbItem, errors::StoreResult, DB};
use hashes::Hash;

/// Reader API for `PruningStore`.
pub trait PruningStoreReader {
    fn pruning_point(&self) -> StoreResult<Hash>;
    fn pruning_point_candidate(&self) -> StoreResult<Hash>;
}

pub trait PruningStore: PruningStoreReader {
    fn set_pruning_point_and_candidate(&mut self, pruning_point: Hash, candidate: Hash) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"pruning";

/// A DB + cache implementation of `PruningStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningStore {
    raw_db: Arc<DB>,
    pruning_point_and_candidate: CachedDbItem<(Hash, Hash)>,
}

const PRUNING_POINT_KEY: &[u8] = b"pruning-point";

impl DbPruningStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { raw_db: Arc::clone(&db), pruning_point_and_candidate: CachedDbItem::new(db.clone(), PRUNING_POINT_KEY) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db))
    }
}

impl PruningStoreReader for DbPruningStore {
    fn pruning_point(&self) -> StoreResult<Hash> {
        Ok(self.pruning_point_and_candidate.read()?.0)
    }

    fn pruning_point_candidate(&self) -> StoreResult<Hash> {
        Ok(self.pruning_point_and_candidate.read()?.1)
    }
}

impl PruningStore for DbPruningStore {
    fn set_pruning_point_and_candidate(&mut self, pruning_point: Hash, candidate: Hash) -> StoreResult<()> {
        self.pruning_point_and_candidate.write(&(pruning_point, candidate))
    }
}
