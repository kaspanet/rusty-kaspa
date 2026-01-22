use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreError, StoreResult, WriteBatch, DB},
    registry::DatabaseStorePrefixes,
};

use kaspa_consensus_core::BlockHashSet;
use kaspa_hashes::Hash;

// This is required to keep block added / included transactions in sync.

/// Reader API for `TxIndexTipsStore`.
pub trait TxIndexTipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait TxIndexTipsStore: TxIndexTipsStoreReader {
    fn init_tips(&mut self, writer: BatchDbWriter, tips: BlockHashSet) -> StoreResult<()>;
    fn set_tip(&mut self, writer: BatchDbWriter, new_tip: Hash) -> StoreResult<()>;
    fn set_tips(&mut self, writer: BatchDbWriter, new_tips: BlockHashSet) -> StoreResult<()>;
    fn remove_tips(&mut self, writer: BatchDbWriter, removed_tips: Vec<Hash>) -> StoreResult<()>;
    fn delete_all(&mut self) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `TxIndexTipsStore` trait
#[derive(Clone)]
pub struct DbTxIndexTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbTxIndexTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexTips.into()) }
    }
}

impl TxIndexTipsStoreReader for DbTxIndexTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.access.read()
    }
}

impl TxIndexTipsStore for DbTxIndexTipsStore {
    fn init_tips(&mut self, mut writer: BatchDbWriter, tips: BlockHashSet) -> StoreResult<()> {
        self.access.write(&mut writer, &Arc::new(tips))
    }

    fn set_tip(&mut self, mut writer: BatchDbWriter, new_tip: Hash) -> StoreResult<()> {
        self.access.update(&mut writer, |mut tips| {
            Arc::make_mut(&mut tips).insert(new_tip);
            tips
        })?;
        Ok(())
    }
    fn set_tips(&mut self, mut writer: BatchDbWriter, new_tips: BlockHashSet) -> StoreResult<()> {
        self.access.update(&mut writer, |mut tips| {
            *Arc::make_mut(&mut tips) = Arc::make_mut(&mut tips).union(&new_tips).cloned().collect::<BlockHashSet>();
            tips
        })?;
        Ok(())
    }
    fn remove_tips(&mut self, mut writer: BatchDbWriter, removed_tips: Vec<Hash>) -> StoreResult<()> {
        self.access.update(&mut writer, move |mut tips| {
            for tip in &removed_tips {
                Arc::make_mut(&mut tips).remove(&tip);
            }
            tips
        })?;
        Ok(())
    }
    fn delete_all(&mut self) -> Result<(), StoreError> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::Hash;

    #[test]
    fn test_txindex_tips_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTxIndexTipsStore::new(txindex_db.clone());

        // Initially empty
        assert!(matches!(store.get().unwrap_err(), StoreError::KeyNotFound(_)));

        // Initialize tips
        let initial_tips: BlockHashSet = [Hash::from_slice(&[0u8; 32])].iter().cloned().collect();
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.init_tips(writer, initial_tips.clone()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get().unwrap();
        assert_eq!(tips.len(), 1);
        assert!(tips.contains(&Hash::from_slice(&[0u8; 32])));

        // Set a tip
        let tip1 = Hash::from_slice(&[1u8; 32]);
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_tip(writer, tip1).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get().unwrap();
        assert_eq!(tips.len(), 2);
        assert!(tips.contains(&tip1));
        assert!(tips.contains(&initial_tips.iter().next().unwrap()));

        // Set 2 tips
        let tip2 = Hash::from_slice(&[2u8; 32]);
        let tip3 = Hash::from_slice(&[3u8; 32]);
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_tips(writer, [tip2, tip1, tip3].iter().cloned().collect()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get().unwrap();
        assert_eq!(tips.len(), 4);
        assert!(tips.contains(&tip1));
        assert!(tips.contains(&tip2));
        assert!(tips.contains(&tip3));
        assert!(tips.contains(&initial_tips.iter().next().unwrap()));

        // Remove 2 tips
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.remove_tips(writer, vec![tip1, tip2]).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get().unwrap();
        assert_eq!(tips.len(), 2);
        assert!(tips.contains(&tip3));
        assert!(tips.contains(&initial_tips.iter().next().unwrap()));

        // Delete all tips
        store.delete_all().unwrap();
        assert!(matches!(store.get().unwrap_err(), StoreError::KeyNotFound(_)));
    }
}
