use consensus_core::blockstatus::BlockStatus;
use consensus_core::ChainPath;
use parking_lot::RwLockWriteGuard;
use rocksdb::WriteBatch;

use std::sync::Arc;

use database::prelude::{BatchDbWriter, CachedDbAccess};
use database::prelude::{CachedDbItem, DB};
use database::prelude::{StoreError, StoreResult};
use hashes::Hash;

use super::U64Key;

/// Reader API for `SelectedChainStore`.
pub trait SelectedChainStoreReader {
    fn get_by_hash(&self, hash: Hash) -> StoreResult<u64>;
    fn get_by_index(&self, index: u64) -> StoreResult<Hash>;
}

/// Write API for `SelectedChainStore`. The set function is deliberately `mut`
/// since chain index is not append-only and thus needs to be guarded.
pub trait SelectedChainStore: SelectedChainStoreReader {
    fn apply_changes(&mut self, batch: &mut WriteBatch, changes: ChainPath) -> StoreResult<()>;
    fn init_with_pruning_point(&mut self, batch: &mut WriteBatch, block: Hash) -> StoreResult<()>;
}

const STORE_PREFIX_HASH_BY_INDEX: &[u8] = b"selected-chain-hash-by-index";
const STORE_PREFIX_INDEX_BY_HASH: &[u8] = b"selected-chain-index-by-hash";
const STORE_PREFIX_HIGHEST_INDEX: &[u8] = b"selected-chain-highest-index";

/// A DB + cache implementation of `SelectedChainStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbSelectedChainStore {
    db: Arc<DB>,
    access_hash_by_index: CachedDbAccess<U64Key, Hash>,
    access_index_by_hash: CachedDbAccess<Hash, u64>,
    access_highest_index: CachedDbItem<u64>,
}

impl DbSelectedChainStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            db: Arc::clone(&db),
            access_hash_by_index: CachedDbAccess::new(db.clone(), cache_size, STORE_PREFIX_HASH_BY_INDEX.to_vec()),
            access_index_by_hash: CachedDbAccess::new(db.clone(), cache_size, STORE_PREFIX_INDEX_BY_HASH.to_vec()),
            access_highest_index: CachedDbItem::new(db, STORE_PREFIX_HIGHEST_INDEX.to_vec()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
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
}

impl SelectedChainStore for DbSelectedChainStore {
    fn apply_changes(&mut self, batch: &mut WriteBatch, changes: ChainPath) -> StoreResult<()> {
        let added_len = changes.added.len() as u64;
        let current_highest_index = self.access_highest_index.read().unwrap();
        let split_index = current_highest_index - changes.removed.len() as u64;
        let new_highest_index = added_len + split_index;

        for to_remove in changes.removed {
            let index = self.access_index_by_hash.read(to_remove).unwrap();
            self.access_index_by_hash.delete(BatchDbWriter::new(batch), to_remove).unwrap();
            self.access_hash_by_index.delete(BatchDbWriter::new(batch), index.into()).unwrap();
        }

        for (i, to_add) in changes.added.into_iter().enumerate() {
            self.access_index_by_hash.write(BatchDbWriter::new(batch), to_add, i as u64 + split_index + 1).unwrap();
            self.access_hash_by_index.write(BatchDbWriter::new(batch), (i as u64 + split_index + 1).into(), to_add).unwrap();
        }

        self.access_highest_index.write(BatchDbWriter::new(batch), &new_highest_index).unwrap();
        Ok(())
    }

    fn init_with_pruning_point(&mut self, batch: &mut WriteBatch, block: Hash) -> StoreResult<()> {
        self.access_index_by_hash.write(BatchDbWriter::new(batch), block, 0)?;
        self.access_hash_by_index.write(BatchDbWriter::new(batch), 0.into(), block)?;
        self.access_highest_index.write(BatchDbWriter::new(batch), &0).unwrap();
        Ok(())
    }
}
