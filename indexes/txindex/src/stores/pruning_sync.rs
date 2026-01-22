use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreResult, DB},
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

trait PruningSyncStoreReader {
    fn get_pruning_data(&self) -> StoreResult<PruningData>;
}

trait PruningSyncStore: PruningSyncStoreReader {
    fn init_pruning_data(&mut self, writer: BatchDbWriter, data: PruningData) -> StoreResult<()>;
    fn set_new_last_pruned_blue_score(&mut self, writer: BatchDbWriter, blue_score: u64) -> StoreResult<()>;
    fn set_new_retention_root(
        &mut self,
        writer: BatchDbWriter,
        retention_root: Hash,
        retention_root_blue_score: u64,
    ) -> StoreResult<()>;
    fn remove_pruning_data(&mut self) -> StoreResult<()>;
}

// --- implementations ---
#[derive(Clone)]
pub struct DbPruningSyncStore {
    db: Arc<DB>,
    access: CachedDbItem<PruningData>,
}

impl DbPruningSyncStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningData.into()) }
    }
}

impl PruningSyncStoreReader for DbPruningSyncStore {
    fn get_pruning_data(&self) -> StoreResult<PruningData> {
        self.access.read()
    }
}

impl PruningSyncStore for DbPruningSyncStore {
    fn init_pruning_data(&mut self, mut writer: BatchDbWriter, data: PruningData) -> StoreResult<()> {
        self.access.write(&mut writer, &data)
    }

    fn set_new_last_pruned_blue_score(&mut self, mut writer: BatchDbWriter, new_blue_score: u64) -> StoreResult<()> {
        self.access.update(&mut writer, |mut data| {
            data.last_pruned_blue_score = new_blue_score;
            data
        })?;
        Ok(())
    }

    fn set_new_retention_root(
        &mut self,
        mut writer: BatchDbWriter,
        retention_root: Hash,
        retention_root_blue_score: u64,
    ) -> StoreResult<()> {
        self.access.update(&mut writer, |mut data| {
            data.retention_root = retention_root;
            data.retention_root_blue_score = retention_root_blue_score;
            data
        })?;
        Ok(())
    }

    fn remove_pruning_data(&mut self) -> StoreResult<()> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}

// --- tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, StoreError, WriteBatch, DB},
    };
    use kaspa_hashes::Hash;
    use std::sync::Arc;

    #[test]
    fn test_pruning_sync_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPruningSyncStore::new(txindex_db.clone());
        let retention_root1 = Hash::from_slice(&[1u8; 32]);
        let retention_root2 = Hash::from_slice(&[2u8; 32]);
        let pruning_data1 =
            PruningData { retention_root: retention_root1, retention_root_blue_score: 100, last_pruned_blue_score: 50 };

        // Initially empty
        assert!(matches!(store.get_pruning_data().unwrap_err(), StoreError::KeyNotFound(_)));

        // Initialize pruning data
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.init_pruning_data(writer, pruning_data1.clone()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap();
        assert_eq!(data.retention_root, retention_root1);
        assert_eq!(data.retention_root_blue_score, 100);
        assert_eq!(data.last_pruned_blue_score, 50);
        // Update last pruned blue score
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_last_pruned_blue_score(writer, 75).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap();
        assert_eq!(data.last_pruned_blue_score, 75);

        // Update retention root
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_retention_root(writer, retention_root2, 200).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap();
        assert_eq!(data.retention_root, retention_root2);
        assert_eq!(data.retention_root_blue_score, 200);

        // Remove pruning data
        store.remove_pruning_data().unwrap();
        assert!(matches!(store.get_pruning_data().unwrap_err(), StoreError::KeyNotFound(_)));
    }
}
