use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_database::{
    prelude::{CachedDbItem, DbWriter, DirectDbWriter, StoreResult, StoreResultExt, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PruningData {
    retention_root: Hash,
    retention_root_blue_score: u64,
    next_to_prune_blue_score: u64,
}

impl MemSizeEstimator for PruningData {}

pub trait PruningSyncStoreReader {
    fn get_retention_root_blue_score(&self) -> StoreResult<Option<u64>>;
    fn get_retention_root(&self) -> StoreResult<Option<Hash>>;
    fn get_next_to_prune_blue_score(&self) -> StoreResult<Option<u64>>;
}

pub trait PruningSyncStore: PruningSyncStoreReader {
    fn set_new_next_to_prune_blue_score(&mut self, writer: &mut impl DbWriter, blue_score: u64) -> StoreResult<()>;
    fn set_new_retention_root(
        &mut self,
        writer: &mut impl DbWriter,
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

    fn get_pruning_data(&self) -> StoreResult<Option<PruningData>> {
        self.access.read().optional()
    }
}

impl PruningSyncStoreReader for DbPruningSyncStore {

    fn get_retention_root_blue_score(&self) -> StoreResult<Option<u64>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.retention_root_blue_score))
    }

    fn get_retention_root(&self) -> StoreResult<Option<Hash>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.retention_root))
    }

    fn get_next_to_prune_blue_score(&self) -> StoreResult<Option<u64>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.next_to_prune_blue_score))
    }
}

impl PruningSyncStore for DbPruningSyncStore {
    fn set_new_next_to_prune_blue_score(&mut self, writer: &mut impl DbWriter, new_blue_score: u64) -> StoreResult<()> {
        self.access.update(writer, |mut data| {
            data.next_to_prune_blue_score = new_blue_score;
            data
        })?;
        Ok(())
    }

    fn set_new_retention_root(
        &mut self,
        writer: &mut impl DbWriter,
        retention_root: Hash,
        retention_root_blue_score: u64,
    ) -> StoreResult<()> {
        if self.access.read().optional()?.is_none() {
            let pruning_data = PruningData { retention_root, retention_root_blue_score, next_to_prune_blue_score: 0 };
            self.access.write(writer, &pruning_data)?;
            return Ok(());
        }
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
            PruningData { retention_root: retention_root1, retention_root_blue_score: 100, next_to_prune_blue_score: 50 };

        // Initially empty
        assert!(matches!(store.get_pruning_data(), Ok(None)));

        // Initialize pruning data
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store
            .set_new_retention_root(&mut writer, pruning_data1.clone().retention_root, pruning_data1.clone().retention_root_blue_score)
            .unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.retention_root, retention_root1);
        assert_eq!(data.retention_root_blue_score, 100);
        assert_eq!(data.next_to_prune_blue_score, 50);
        // Update next to prune blue score
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_next_to_prune_blue_score(&mut writer, 75).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.next_to_prune_blue_score, 75);

        // Update retention root
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_retention_root(&mut writer, retention_root2, 200).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.retention_root, retention_root2);
        assert_eq!(data.retention_root_blue_score, 200);

        // Remove pruning data
        store.remove_pruning_data().unwrap();
        assert!(matches!(store.get_pruning_data(), Ok(None)));
    }
}
