use std::sync::Arc;

use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
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

    /// Represent the point after which data is fully held (i.e., history is consecutive from this point and up to virtual).
    /// This is usually a pruning point that is at or below the retention period requirement (and for archival
    /// nodes it will remain the initial syncing point or the last pruning point before turning to an archive).
    /// At every pruning point movement, this is adjusted to the next pruning point sample that satisfies the required
    /// retention period.
    fn retention_period_root(&self) -> StoreResult<Hash>;

    // During pruning, this is a reference to the retention root before the pruning point move.
    // After pruning, this is updated to point to the retention period root.
    // This checkpoint is used to determine if pruning has successfully completed.
    fn retention_checkpoint(&self) -> StoreResult<Hash>;
}

pub trait PruningStore: PruningStoreReader {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()>;
}

/// A DB + cache implementation of `PruningStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbPruningStore {
    db: Arc<DB>,
    access: CachedDbItem<PruningPointInfo>,
    retention_checkpoint_access: CachedDbItem<Hash>,
    retention_period_root_access: CachedDbItem<Hash>,
}

impl DbPruningStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningPoint.into()),
            retention_checkpoint_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::RetentionCheckpoint.into()),
            retention_period_root_access: CachedDbItem::new(db, DatabaseStorePrefixes::RetentionPeriodRoot.into()),
        }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &PruningPointInfo { pruning_point, candidate, index })
    }

    pub fn set_retention_checkpoint(&mut self, batch: &mut WriteBatch, retention_checkpoint: Hash) -> StoreResult<()> {
        self.retention_checkpoint_access.write(BatchDbWriter::new(batch), &retention_checkpoint)
    }

    pub fn set_retention_period_root(&mut self, batch: &mut WriteBatch, retention_period_root: Hash) -> StoreResult<()> {
        self.retention_period_root_access.write(BatchDbWriter::new(batch), &retention_period_root)
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

    fn retention_checkpoint(&self) -> StoreResult<Hash> {
        self.retention_checkpoint_access.read()
    }

    fn retention_period_root(&self) -> StoreResult<Hash> {
        self.retention_period_root_access.read()
    }
}

impl PruningStore for DbPruningStore {
    fn set(&mut self, pruning_point: Hash, candidate: Hash, index: u64) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &PruningPointInfo::new(pruning_point, candidate, index))
    }
}
