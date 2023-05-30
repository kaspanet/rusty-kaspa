use std::sync::Arc;

use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct PruningPointInfo {
    pub pruning_point: Hash,
    pub candidate: Hash,
    pub index: u64,
}

impl PruningPointInfo {
    pub fn new(pruning_point: Hash, candidate: Hash, index: u64) -> Self {
        Self { pruning_point, candidate, index }
    }

    pub fn from_genesis(genesis_hash: Hash) -> Self {
        Self { pruning_point: genesis_hash, candidate: genesis_hash, index: 0 }
    }

    pub fn decompose(self) -> (Hash, Hash, u64) {
        (self.pruning_point, self.candidate, self.index)
    }
}

/// Reader API for `PruningStore`.
pub trait PruningStoreReader {
    fn pruning_point(&self) -> StoreResult<Hash>;
    fn pruning_point_candidate(&self) -> StoreResult<Hash>;
    fn pruning_point_index(&self) -> StoreResult<u64>;

    /// Returns full pruning point info, including its index and the next pruning point candidate
    fn get(&self) -> StoreResult<PruningPointInfo>;

    /// Represent the point at which data prior to it was successfully pruned. This is usually the
    /// pruning point itself, though it might lag a bit behind (and for archival nodes it will remain
    /// the initial syncing point or the last pruning point before turning to an archive)
    fn data_pruned_point(&self) -> StoreResult<Hash>;
}

pub trait PruningStore: PruningStoreReader {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()>;
}

/// A DB + cache implementation of `PruningStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningStore {
    db: Arc<DB>,
    access: CachedDbItem<PruningPointInfo>,
    data_pruned_point_access: CachedDbItem<Hash>,
}

const PRUNING_POINT_KEY: &[u8] = b"pruning-point";
const DATA_PRUNED_POINT_KEY: &[u8] = b"data-pruned-point";

impl DbPruningStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbItem::new(db.clone(), PRUNING_POINT_KEY.to_vec()),
            data_pruned_point_access: CachedDbItem::new(db.clone(), DATA_PRUNED_POINT_KEY.to_vec()),
        }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &PruningPointInfo { pruning_point, candidate, index })
    }

    pub fn set_data_pruned_point(&mut self, batch: &mut WriteBatch, data_pruned_point: Hash) -> StoreResult<()> {
        self.data_pruned_point_access.write(BatchDbWriter::new(batch), &data_pruned_point)
    }
}

impl PruningStoreReader for DbPruningStore {
    fn pruning_point(&self) -> StoreResult<Hash> {
        Ok(self.access.read()?.pruning_point)
    }

    fn pruning_point_candidate(&self) -> StoreResult<Hash> {
        Ok(self.access.read()?.candidate)
    }

    fn pruning_point_index(&self) -> StoreResult<u64> {
        Ok(self.access.read()?.index)
    }

    fn get(&self) -> StoreResult<PruningPointInfo> {
        self.access.read()
    }

    fn data_pruned_point(&self) -> StoreResult<Hash> {
        self.data_pruned_point_access.read()
    }
}

impl PruningStore for DbPruningStore {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &PruningPointInfo::new(pruning_point, candidate, index))
    }
}
