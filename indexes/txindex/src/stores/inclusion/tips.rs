use std::sync::Arc;

use kaspa_database::{
    prelude::{CachedDbItem, DbWriter, DirectDbWriter, StoreError, StoreResult, StoreResultExt, DB},
    registry::DatabaseStorePrefixes,
};

use kaspa_consensus_core::BlockHashSet;
use kaspa_hashes::Hash;

// This is required to keep block added / included transactions in sync.

/// Reader API for `TxIndexTipsStore`.
pub trait TxIndexTipsStoreReader {
    fn get_tips(&self) -> StoreResult<Option<Arc<BlockHashSet>>>;
}

pub trait TxIndexTipsStore: TxIndexTipsStoreReader {
    fn init_tips(&mut self, writer: &mut impl DbWriter, tips: BlockHashSet) -> StoreResult<()>;
    fn set_tip(&mut self, writer: &mut impl DbWriter, new_tip: Hash) -> StoreResult<()>;
    fn set_tips(&mut self, writer: &mut impl DbWriter, new_tips: BlockHashSet) -> StoreResult<()>;
    fn remove_tips(&mut self, writer: &mut impl DbWriter, removed_tips: Vec<Hash>) -> StoreResult<()>;
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
    fn get_tips(&self) -> StoreResult<Option<Arc<BlockHashSet>>> {
        self.access.read().optional()
    }
}

impl TxIndexTipsStore for DbTxIndexTipsStore {
    fn init_tips(&mut self, writer: &mut impl DbWriter, tips: BlockHashSet) -> StoreResult<()> {
        self.access.write(writer, &Arc::new(tips))
    }

    fn set_tip(&mut self, writer: &mut impl DbWriter, new_tip: Hash) -> StoreResult<()> {
        self.access.update(writer, |mut tips| {
            Arc::make_mut(&mut tips).insert(new_tip);
            tips
        })?;
        Ok(())
    }
    fn set_tips(&mut self, writer: &mut impl DbWriter, new_tips: BlockHashSet) -> StoreResult<()> {
        self.access.update(writer, |mut tips| {
            *Arc::make_mut(&mut tips) = Arc::make_mut(&mut tips).union(&new_tips).cloned().collect::<BlockHashSet>();
            tips
        })?;
        Ok(())
    }
    fn remove_tips(&mut self, writer: &mut impl DbWriter, removed_tips: Vec<Hash>) -> StoreResult<()> {
        self.access.update(writer, move |mut tips| {
            for tip in &removed_tips {
                Arc::make_mut(&mut tips).remove(tip);
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
    use kaspa_database::prelude::BatchDbWriter;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::Hash;
    use rocksdb::WriteBatch;

    #[test]
    fn test_txindex_tips_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTxIndexTipsStore::new(txindex_db.clone());

        // Initially empty
        assert!(store.get_tips().is_ok_and(|v| v.is_none()));

        // Initialize tips
        let initial_tips: BlockHashSet = [Hash::from_slice(&[0u8; 32])].iter().cloned().collect();
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.init_tips(&mut writer, initial_tips.clone()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get_tips().unwrap();
        assert_eq!(tips.as_ref().unwrap().len(), 1);
        assert!(tips.as_ref().unwrap().contains(&Hash::from_slice(&[0u8; 32])));

        // Set a tip
        let tip1 = Hash::from_slice(&[1u8; 32]);
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_tip(&mut writer, tip1).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get_tips().unwrap();
        assert_eq!(tips.as_ref().unwrap().len(), 2);
        assert!(tips.as_ref().unwrap().contains(&tip1));
        assert!(tips.as_ref().unwrap().contains(&initial_tips.iter().next().unwrap()));

        // Set 2 tips
        let tip2 = Hash::from_slice(&[2u8; 32]);
        let tip3 = Hash::from_slice(&[3u8; 32]);
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.set_tips(&mut writer, [tip2, tip1, tip3].iter().cloned().collect()).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get_tips().unwrap();
        assert_eq!(tips.as_ref().unwrap().len(), 4);
        assert!(tips.as_ref().unwrap().contains(&tip1));
        assert!(tips.as_ref().unwrap().contains(&tip2));
        assert!(tips.as_ref().unwrap().contains(&tip3));
        assert!(tips.as_ref().unwrap().contains(&initial_tips.iter().next().unwrap()));

        // Remove 2 tips
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.remove_tips(&mut writer, vec![tip1, tip2]).unwrap();
        txindex_db.write(write_batch).unwrap();
        let tips = store.get_tips().unwrap();
        assert_eq!(tips.as_ref().unwrap().len(), 2);
        assert!(tips.as_ref().unwrap().contains(&tip3));
        assert!(tips.as_ref().unwrap().contains(&initial_tips.iter().next().unwrap()));

        // Delete all tips
        store.delete_all().unwrap();
        assert!(store.get_tips().unwrap().is_none());
    }
}
