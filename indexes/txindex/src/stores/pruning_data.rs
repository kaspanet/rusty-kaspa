use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_database::{
    prelude::{CachedDbItem, DirectDbWriter, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PruningData {
    retention_root: Hash,
    retention_root_blue_score: u64,
    last_pruned_blue_score: u64,
}

impl MemSizeEstimator for PruningData {}

trait PruningDataStoreReader {
    fn get_pruning_data(&self) -> StoreResult<PruningData>;
}

trait PruningDataStore: PruningDataStoreReader {
    fn set_new_last_pruned_blue_score(&mut self, blue_score: u64) -> StoreResult<()>;
    fn set_new_retention_root(&mut self, retention_root: Hash, retention_root_blue_score: u64) -> StoreResult<()>;
}

// --- implementations ---
pub struct DbPruningDataStore {
    db: Arc<DB>,
    access: CachedDbItem<PruningData>,
}

impl DbPruningDataStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningData.into()) }
    }
}

impl PruningDataStoreReader for DbPruningDataStore {
    fn get_pruning_data(&self) -> StoreResult<PruningData> {
        self.access.read()
    }
}

impl PruningDataStore for DbPruningDataStore {
    fn set_new_last_pruned_blue_score(&mut self, new_blue_score: u64) -> StoreResult<()> {
        self.access.update(DirectDbWriter::new(&self.db), |mut data| {
            data.last_pruned_blue_score = new_blue_score;
            data
        })?;

        Ok(())
    }

    fn set_new_retention_root(&mut self, retention_root: Hash, retention_root_blue_score: u64) -> StoreResult<()> {
        self.access.update(DirectDbWriter::new(&self.db), |mut data| {
            data.retention_root = retention_root;
            data.retention_root_blue_score = retention_root_blue_score;
            data
        })?;

        Ok(())
    }
}
