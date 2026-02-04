use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_database::{
    prelude::{CachedDbItem, DB, DbWriter, DirectDbWriter, StoreResult, StoreResultExt},
    registry::DatabaseStorePrefixes,
};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};

#[repr(u8)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ToPruneStore {
    AcceptanceData = 0,
    InclusionData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PruningData {
    retention_root: Hash,
    retention_root_blue_score: u64,
    retention_root_daa_score: u64,
    next_to_prune_blue_score: u64,
    next_to_prune_daa_score: u64,
    next_to_prune_store: ToPruneStore,
}

impl PruningData {
    pub fn new(
        retention_root: Hash,
        retention_root_blue_score: u64,
        retention_root_daa_score: u64,
        next_to_prune_blue_score: u64,
        next_to_prune_daa_score: u64,
        next_to_prune_store: ToPruneStore,
    ) -> Self {
        Self {
            retention_root,
            retention_root_blue_score,
            retention_root_daa_score,
            next_to_prune_blue_score,
            next_to_prune_daa_score,
            next_to_prune_store,
        }
    }
}

impl MemSizeEstimator for PruningData {}

pub trait PruningSyncStoreReader {
    fn get_retention_root_blue_score(&self) -> StoreResult<Option<u64>>;
    fn get_retention_root_daa_score(&self) -> StoreResult<Option<u64>>;
    fn get_retention_root(&self) -> StoreResult<Option<Hash>>;

    fn get_next_to_prune_blue_score(&self) -> StoreResult<Option<u64>>;
    fn get_next_to_prune_daa_score(&self) -> StoreResult<Option<u64>>;

    fn get_next_to_prune_store(&self) -> StoreResult<Option<ToPruneStore>>;

    fn is_acceptance_pruning_done(&self) -> StoreResult<bool>;
    fn is_inclusion_pruning_done(&self) -> StoreResult<bool>;
}

pub trait PruningSyncStore: PruningSyncStoreReader {
    fn set_new_next_to_prune_blue_score(&mut self, writer: &mut impl DbWriter, blue_score: u64) -> StoreResult<()>;
    fn set_new_next_to_prune_daa_score(&mut self, writer: &mut impl DbWriter, daa_score: u64) -> StoreResult<()>;

    fn set_new_pruning_data(&mut self, writer: &mut impl DbWriter, pruning_data: PruningData) -> StoreResult<()>;
    fn update_to_new_retention_root(
        &mut self,
        writer: &mut impl DbWriter,
        retention_root: Hash,
        retention_root_blue_score: u64,
        retention_root_daa_score: u64,
    ) -> StoreResult<()>;

    fn set_next_to_prune_store(&mut self, writer: &mut impl DbWriter, to_prune_store: ToPruneStore) -> StoreResult<()>;

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

    fn get_retention_root_daa_score(&self) -> StoreResult<Option<u64>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.retention_root_daa_score))
    }

    fn get_retention_root(&self) -> StoreResult<Option<Hash>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.retention_root))
    }

    fn get_next_to_prune_blue_score(&self) -> StoreResult<Option<u64>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.next_to_prune_blue_score))
    }

    fn get_next_to_prune_daa_score(&self) -> StoreResult<Option<u64>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.next_to_prune_daa_score))
    }

    fn get_next_to_prune_store(&self) -> StoreResult<Option<ToPruneStore>> {
        let data = self.get_pruning_data()?;
        Ok(data.map(|data| data.next_to_prune_store))
    }

    fn is_acceptance_pruning_done(&self) -> StoreResult<bool> {
        let data = self.get_pruning_data()?;
        if let Some(data) = data { Ok(data.next_to_prune_blue_score >= data.retention_root_blue_score) } else { Ok(false) }
    }

    fn is_inclusion_pruning_done(&self) -> StoreResult<bool> {
        let data = self.get_pruning_data()?;
        if let Some(data) = data { Ok(data.next_to_prune_daa_score >= data.retention_root_daa_score) } else { Ok(false) }
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

    fn set_new_next_to_prune_daa_score(&mut self, writer: &mut impl DbWriter, daa_score: u64) -> StoreResult<()> {
        self.access.update(writer, |mut data| {
            data.next_to_prune_daa_score = daa_score;
            data
        })?;
        Ok(())
    }

    fn set_next_to_prune_store(&mut self, writer: &mut impl DbWriter, to_prune_store: ToPruneStore) -> StoreResult<()> {
        self.access.update(writer, |mut data| {
            data.next_to_prune_store = to_prune_store.clone();
            data
        })?;
        Ok(())
    }

    fn set_new_pruning_data(&mut self, writer: &mut impl DbWriter, pruning_data: PruningData) -> StoreResult<()> {
        self.access.write(writer, &pruning_data)?;
        Ok(())
    }

    fn update_to_new_retention_root(
        &mut self,
        writer: &mut impl DbWriter,
        retention_root: Hash,
        retention_root_blue_score: u64,
        retention_root_daa_score: u64,
    ) -> StoreResult<()> {
        self.access.update(writer, |mut data| {
            data.retention_root = retention_root;
            data.retention_root_blue_score = retention_root_blue_score;
            data.retention_root_daa_score = retention_root_daa_score;
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
        prelude::{BatchDbWriter, ConnBuilder},
    };
    use kaspa_hashes::Hash;
    use rocksdb::WriteBatch;

    #[test]
    fn test_pruning_sync_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbPruningSyncStore::new(txindex_db.clone());
        let retention_root1 = Hash::from_slice(&[1u8; 32]);
        let retention_root2 = Hash::from_slice(&[2u8; 32]);
        let pruning_data1 = PruningData {
            retention_root: retention_root1,
            retention_root_blue_score: 100,
            retention_root_daa_score: 1000,
            next_to_prune_blue_score: 50,
            next_to_prune_daa_score: 500,
            next_to_prune_store: ToPruneStore::AcceptanceData,
        };

        // Initially empty
        assert!(matches!(store.get_pruning_data(), Ok(None)));

        // Initialize pruning data
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_pruning_data(&mut writer, pruning_data1.clone()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.retention_root, retention_root1);
        assert_eq!(data.retention_root_blue_score, 100);
        assert_eq!(data.retention_root_daa_score, 1000);
        assert_eq!(data.next_to_prune_blue_score, 50);
        assert_eq!(data.next_to_prune_daa_score, 500);
        assert!(matches!(data.next_to_prune_store, ToPruneStore::AcceptanceData));

        // Update next to prune blue score
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_next_to_prune_blue_score(&mut writer, 75).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.next_to_prune_blue_score, 75);

        // Update next to prune daa score
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_new_next_to_prune_daa_score(&mut writer, 750).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.next_to_prune_daa_score, 750);

        // Update next to prune store
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_next_to_prune_store(&mut writer, ToPruneStore::InclusionData).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert!(matches!(data.next_to_prune_store, ToPruneStore::InclusionData));

        // Update retention root and daa score
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.update_to_new_retention_root(&mut writer, retention_root2, 200, 2000).unwrap();
        txindex_db.write(write_batch).unwrap();
        let data = store.get_pruning_data().unwrap().unwrap();
        assert_eq!(data.retention_root, retention_root2);
        assert_eq!(data.retention_root_blue_score, 200);
        assert_eq!(data.retention_root_daa_score, 2000);

        // Remove pruning data
        store.remove_pruning_data().unwrap();
        assert!(matches!(store.get_pruning_data(), Ok(None)));
    }
}
