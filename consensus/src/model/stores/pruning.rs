use std::sync::Arc;

use super::{caching::CachedDbItem, errors::StoreResult, DB};
use hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `PruningStore`.
pub trait PruningStoreReader {
    fn pruning_point(&self) -> StoreResult<Hash>;
    fn pruning_point_candidate(&self) -> StoreResult<Hash>;
    fn pruning_point_index(&self) -> StoreResult<u64>;
}

pub trait PruningStore: PruningStoreReader {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"pruning";

/// A DB + cache implementation of `PruningStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningStore {
    raw_db: Arc<DB>,
    pruning_point_and_candidate_and_current_index: CachedDbItem<(Hash, Hash, u64)>,
}

const PRUNING_POINT_KEY: &[u8] = b"pruning-point";

impl DbPruningStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            raw_db: Arc::clone(&db),
            pruning_point_and_candidate_and_current_index: CachedDbItem::new(db.clone(), PRUNING_POINT_KEY),
        }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.pruning_point_and_candidate_and_current_index.write_batch(batch, &(pruning_point, candidate, index))
    }
}

impl PruningStoreReader for DbPruningStore {
    fn pruning_point(&self) -> StoreResult<Hash> {
        Ok(self.pruning_point_and_candidate_and_current_index.read()?.0)
    }

    fn pruning_point_candidate(&self) -> StoreResult<Hash> {
        Ok(self.pruning_point_and_candidate_and_current_index.read()?.1)
    }

    fn pruning_point_index(&self) -> StoreResult<u64> {
        Ok(self.pruning_point_and_candidate_and_current_index.read()?.2)
    }
}

impl PruningStore for DbPruningStore {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.pruning_point_and_candidate_and_current_index.write(&(pruning_point, candidate, index))
    }
}
