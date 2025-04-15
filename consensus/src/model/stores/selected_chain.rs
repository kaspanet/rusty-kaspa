use kaspa_consensus_core::blockstatus::BlockStatus;
use kaspa_consensus_core::ChainPath;
use kaspa_database::registry::DatabaseStorePrefixes;
use parking_lot::RwLockWriteGuard;
use rocksdb::WriteBatch;

use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DbWriter};
use kaspa_database::prelude::{CachedDbItem, DB};
use kaspa_database::prelude::{StoreError, StoreResult};
use kaspa_hashes::Hash;

use super::U64Key;

/// Reader API for `SelectedChainStore`.
pub trait SelectedChainStoreReader {
    fn get_by_hash(&self, hash: Hash) -> StoreResult<u64>;
    fn get_by_index(&self, index: u64) -> StoreResult<Hash>;
    fn get_tip(&self) -> StoreResult<(u64, Hash)>;
}

/// Write API for `SelectedChainStore`. The set function is deliberately `mut`
/// since chain index is not append-only and thus needs to be guarded.
pub trait SelectedChainStore: SelectedChainStoreReader {
    fn apply_changes(&mut self, batch: &mut WriteBatch, changes: &ChainPath) -> StoreResult<()>;
    fn prune_below_point(&mut self, writer: impl DbWriter, block: Hash) -> StoreResult<()>;
    fn init_with_pruning_point(&mut self, batch: &mut WriteBatch, block: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `SelectedChainStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbSelectedChainStore {
    db: Arc<DB>,
    access_hash_by_index: CachedDbAccess<U64Key, Hash>,
    access_index_by_hash: CachedDbAccess<Hash, u64>,
    access_highest_index: CachedDbItem<u64>,
}

impl DbSelectedChainStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access_hash_by_index: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::ChainHashByIndex.into()),
            access_index_by_hash: CachedDbAccess::new(db.clone(), cache_policy, DatabaseStorePrefixes::ChainIndexByHash.into()),
            access_highest_index: CachedDbItem::new(db, DatabaseStorePrefixes::ChainHighestIndex.into()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), cache_policy)
    }
}

pub trait SelectedChainStoreBatchExtensions {
    fn apply_changes(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbSelectedChainStore>, StoreError>;
}

impl SelectedChainStoreReader for DbSelectedChainStore {
    fn get_by_hash(&self, hash: Hash) -> StoreResult<u64> {
        self.access_index_by_hash.read(hash)
    }

    fn get_by_index(&self, index: u64) -> StoreResult<Hash> {
        self.access_hash_by_index.read(index.into())
    }

    fn get_tip(&self) -> StoreResult<(u64, Hash)> {
        let idx = self.access_highest_index.read()?;
        let hash = self.access_hash_by_index.read(idx.into())?;
        Ok((idx, hash))
    }
}

impl SelectedChainStore for DbSelectedChainStore {
    fn apply_changes(&mut self, batch: &mut WriteBatch, changes: &ChainPath) -> StoreResult<()> {
        let added_len = changes.added.len() as u64;
        let current_highest_index = self.access_highest_index.read().unwrap();
        let split_index = current_highest_index - changes.removed.len() as u64;
        let new_highest_index = added_len + split_index;

        for to_remove in changes.removed.iter().copied() {
            let index = self.access_index_by_hash.read(to_remove).unwrap();
            self.access_index_by_hash.delete(BatchDbWriter::new(batch), to_remove).unwrap();
            self.access_hash_by_index.delete(BatchDbWriter::new(batch), index.into()).unwrap();
        }

        for (i, to_add) in changes.added.iter().copied().enumerate() {
            self.access_index_by_hash.write(BatchDbWriter::new(batch), to_add, i as u64 + split_index + 1).unwrap();
            self.access_hash_by_index.write(BatchDbWriter::new(batch), (i as u64 + split_index + 1).into(), to_add).unwrap();
        }

        self.access_highest_index.write(BatchDbWriter::new(batch), &new_highest_index).unwrap();
        Ok(())
    }

    fn prune_below_point(&mut self, mut writer: impl DbWriter, block: Hash) -> StoreResult<()> {
        let mut index = self.access_index_by_hash.read(block)?;
        while index > 0 {
            index -= 1;
            match self.access_hash_by_index.read(index.into()) {
                Ok(hash) => {
                    self.access_hash_by_index.delete(&mut writer, index.into())?;
                    self.access_index_by_hash.delete(&mut writer, hash)?;
                }
                Err(StoreError::KeyNotFound(_)) => break, // This signals that data below this point has already been pruned
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }

    fn init_with_pruning_point(&mut self, batch: &mut WriteBatch, block: Hash) -> StoreResult<()> {
        self.access_index_by_hash.write(BatchDbWriter::new(batch), block, 0)?;
        self.access_hash_by_index.write(BatchDbWriter::new(batch), 0.into(), block)?;
        self.access_highest_index.write(BatchDbWriter::new(batch), &0).unwrap();
        Ok(())
    }
}
