use crate::processes::reachability::interval::Interval;
use kaspa_consensus_core::{
    blockhash::{self, BlockHashes},
    BlockHashMap, BlockHashSet, BlockHasher, BlockLevel, HashMapCustomHasher,
};
use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbAccess, CachedDbItem, DbKey, DirectDbWriter, StoreError, DB},
    registry::{DatabaseStorePrefixes, SEPARATOR},
};
use kaspa_hashes::Hash;

use itertools::Itertools;
use parking_lot::{RwLockUpgradableReadGuard, RwLockWriteGuard};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{collections::hash_map::Entry::Vacant, iter::once, sync::Arc};

#[derive(Clone, Serialize, Deserialize)]
pub struct ReachabilityData {
    pub children: BlockHashes,
    pub parent: Hash,
    pub interval: Interval,
    pub height: u64,
    pub future_covering_set: BlockHashes,
}

impl ReachabilityData {
    pub fn new(parent: Hash, interval: Interval, height: u64) -> Self {
        Self { children: Arc::new(vec![]), parent, interval, height, future_covering_set: Arc::new(vec![]) }
    }
}

/// Reader API for `ReachabilityStore`.
pub trait ReachabilityStoreReader {
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError>;
    /// Returns the reachability *tree* parent of `hash`
    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError>;
    /// Returns the reachability *tree* children of `hash`
    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    /// Returns the counts of entries in the store. To be used for tests only
    fn count(&self) -> Result<usize, StoreError>;
}

/// Write API for `ReachabilityStore`. All write functions are deliberately `mut`
/// since reachability writes are not append-only and thus need to be guarded.
pub trait ReachabilityStore: ReachabilityStoreReader {
    fn init(&mut self, origin: Hash, capacity: Interval) -> Result<(), StoreError>;
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError>;
    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError>;
    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError>;
    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError>;
    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError>;
    fn replace_child(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError>;
    fn replace_future_covering_item(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError>;
    fn delete(&mut self, hash: Hash) -> Result<(), StoreError>;
    fn get_height(&self, hash: Hash) -> Result<u64, StoreError>;
    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError>;
    fn get_reindex_root(&self) -> Result<Hash, StoreError>;
}

/// A DB + cache implementation of `ReachabilityStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbReachabilityStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, Arc<ReachabilityData>, BlockHasher>,
    reindex_root: CachedDbItem<Hash>,
    prefix_end: u8,
}

impl DbReachabilityStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self::with_prefix_end(db, cache_size, DatabaseStorePrefixes::Separator.into())
    }

    pub fn with_block_level(db: Arc<DB>, cache_size: u64, level: BlockLevel) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        Self::with_prefix_end(db, cache_size, level)
    }

    fn with_prefix_end(db: Arc<DB>, cache_size: u64, prefix_end: u8) -> Self {
        let store_prefix = DatabaseStorePrefixes::Reachability.into_iter().chain(once(prefix_end)).collect_vec();
        let reindex_root_prefix = DatabaseStorePrefixes::ReachabilityReindexRoot.into_iter().chain(once(prefix_end)).collect_vec();
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(Arc::clone(&db), cache_size, store_prefix),
            reindex_root: CachedDbItem::new(db, reindex_root_prefix),
            prefix_end,
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::with_prefix_end(Arc::clone(&self.db), cache_size, self.prefix_end)
    }
}

impl ReachabilityStore for DbReachabilityStore {
    fn init(&mut self, origin: Hash, capacity: Interval) -> Result<(), StoreError> {
        debug_assert!(!self.access.has(origin)?);

        let data = Arc::new(ReachabilityData::new(blockhash::NONE, capacity, 0));
        let mut batch = WriteBatch::default();
        self.access.write(BatchDbWriter::new(&mut batch), origin, data)?;
        self.reindex_root.write(BatchDbWriter::new(&mut batch), &origin)?;
        self.db.write(batch)?;

        Ok(())
    }

    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        let data = Arc::new(ReachabilityData::new(parent, interval, height));
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        Arc::make_mut(&mut data).interval = interval;
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        let mut data = self.access.read(hash)?;
        let height = data.height;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.children).push(child);
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.future_covering_set).insert(insertion_index, fci);
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        Arc::make_mut(&mut data).parent = new_parent;
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn replace_child(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.children).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn replace_future_covering_item(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.future_covering_set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn delete(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.access.delete(DirectDbWriter::new(&self.db), hash)
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        Ok(self.access.read(hash)?.height)
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        self.reindex_root.write(DirectDbWriter::new(&self.db), &root)
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        self.reindex_root.read()
    }
}

impl ReachabilityStoreReader for DbReachabilityStore {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        Ok(self.access.read(hash)?.interval)
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.access.read(hash)?.parent)
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.access.read(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.access.read(hash)?.future_covering_set))
    }

    fn count(&self) -> Result<usize, StoreError> {
        Ok(self.access.iterator().count())
    }
}

pub struct StagingReachabilityStore<'a> {
    store_read: RwLockUpgradableReadGuard<'a, DbReachabilityStore>,
    staging_writes: BlockHashMap<ReachabilityData>,
    staging_deletions: BlockHashSet,
    staging_reindex_root: Option<Hash>,
}

impl<'a> StagingReachabilityStore<'a> {
    pub fn new(store_read: RwLockUpgradableReadGuard<'a, DbReachabilityStore>) -> Self {
        Self { store_read, staging_writes: BlockHashMap::new(), staging_deletions: Default::default(), staging_reindex_root: None }
    }

    pub fn commit(self, batch: &mut WriteBatch) -> Result<RwLockWriteGuard<'a, DbReachabilityStore>, StoreError> {
        let mut store_write = RwLockUpgradableReadGuard::upgrade(self.store_read);
        for (k, v) in self.staging_writes {
            let data = Arc::new(v);
            store_write.access.write(BatchDbWriter::new(batch), k, data)?
        }
        // Deletions always come after mutations
        store_write.access.delete_many(BatchDbWriter::new(batch), &mut self.staging_deletions.iter().copied())?;
        if let Some(root) = self.staging_reindex_root {
            store_write.reindex_root.write(BatchDbWriter::new(batch), &root)?;
        }
        Ok(store_write)
    }

    fn check_not_in_deletions(&self, hash: Hash) -> Result<(), StoreError> {
        if self.staging_deletions.contains(&hash) {
            Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Reachability.as_ref(), hash)))
        } else {
            Ok(())
        }
    }
}

impl ReachabilityStore for StagingReachabilityStore<'_> {
    fn init(&mut self, origin: Hash, capacity: Interval) -> Result<(), StoreError> {
        self.insert(origin, blockhash::NONE, capacity, 0)?;
        self.set_reindex_root(origin)?;
        Ok(())
    }

    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        // Note: We never delete and re-insert an item (deletion is part of pruning; new items are inserted
        // for new blocks only), hence we can avoid verifying that the new block is not in `staging_deletions`

        if self.store_read.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        if let Vacant(e) = self.staging_writes.entry(hash) {
            e.insert(ReachabilityData::new(parent, interval, height));
            Ok(())
        } else {
            Err(StoreError::HashAlreadyExists(hash))
        }
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            data.interval = interval;
            return Ok(());
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        data.interval = interval;
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.children).push(child);
            return Ok(data.height);
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        let height = data.height;
        Arc::make_mut(&mut data.children).push(child);
        self.staging_writes.insert(hash, data);

        Ok(height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
            return Ok(());
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        Arc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            data.parent = new_parent;
            return Ok(());
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        data.parent = new_parent;
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn replace_child(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.children).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
            return Ok(());
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        Arc::make_mut(&mut data.children).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn replace_future_covering_item(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.future_covering_set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
            return Ok(());
        }

        let mut data = (*self.store_read.access.read(hash)?).clone();
        Arc::make_mut(&mut data.future_covering_set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn delete(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.staging_writes.remove(&hash);
        self.staging_deletions.insert(hash);
        Ok(())
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.height)
        } else {
            Ok(self.store_read.access.read(hash)?.height)
        }
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        self.staging_reindex_root = Some(root);
        Ok(())
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        if let Some(root) = self.staging_reindex_root {
            Ok(root)
        } else {
            Ok(self.store_read.get_reindex_root()?)
        }
    }
}

impl ReachabilityStoreReader for StagingReachabilityStore<'_> {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.staging_deletions.contains(&hash) {
            return Ok(false);
        }
        Ok(self.staging_writes.contains_key(&hash) || self.store_read.access.has(hash)?)
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.interval)
        } else {
            Ok(self.store_read.access.read(hash)?.interval)
        }
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.parent)
        } else {
            Ok(self.store_read.access.read(hash)?.parent)
        }
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(BlockHashes::clone(&data.children))
        } else {
            self.store_read.get_children(hash)
        }
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(BlockHashes::clone(&data.future_covering_set))
        } else {
            self.store_read.get_future_covering_set(hash)
        }
    }

    fn count(&self) -> Result<usize, StoreError> {
        Ok(self
            .store_read
            .access
            .iterator()
            .map(|r| r.unwrap().0)
            .map(|k| <[u8; kaspa_hashes::HASH_SIZE]>::try_from(&k[..]).unwrap())
            .map(Hash::from_bytes)
            .chain(self.staging_writes.keys().copied())
            .collect::<BlockHashSet>()
            .difference(&self.staging_deletions)
            .count())
    }
}

pub struct MemoryReachabilityStore {
    map: BlockHashMap<ReachabilityData>,
    reindex_root: Option<Hash>,
}

impl Default for MemoryReachabilityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryReachabilityStore {
    pub fn new() -> Self {
        Self { map: BlockHashMap::new(), reindex_root: None }
    }

    fn get_data_mut(&mut self, hash: Hash) -> Result<&mut ReachabilityData, StoreError> {
        match self.map.get_mut(&hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Reachability.as_ref(), hash))),
        }
    }

    fn get_data(&self, hash: Hash) -> Result<&ReachabilityData, StoreError> {
        match self.map.get(&hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Reachability.as_ref(), hash))),
        }
    }
}

impl ReachabilityStore for MemoryReachabilityStore {
    fn init(&mut self, origin: Hash, capacity: Interval) -> Result<(), StoreError> {
        self.insert(origin, blockhash::NONE, capacity, 0)?;
        self.set_reindex_root(origin)?;
        Ok(())
    }

    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if let Vacant(e) = self.map.entry(hash) {
            e.insert(ReachabilityData::new(parent, interval, height));
            Ok(())
        } else {
            Err(StoreError::HashAlreadyExists(hash))
        }
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        data.interval = interval;
        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        let data = self.get_data_mut(hash)?;
        Arc::make_mut(&mut data.children).push(child);
        Ok(data.height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        Arc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
        Ok(())
    }

    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        data.parent = new_parent;
        Ok(())
    }

    fn replace_child(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        Arc::make_mut(&mut data.children).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        Ok(())
    }

    fn replace_future_covering_item(&mut self, hash: Hash, replaced_index: usize, replace_with: &[Hash]) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        Arc::make_mut(&mut data.future_covering_set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        Ok(())
    }

    fn delete(&mut self, hash: Hash) -> Result<(), StoreError> {
        self.map.remove(&hash);
        Ok(())
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        Ok(self.get_data(hash)?.height)
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        self.reindex_root = Some(root);
        Ok(())
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        match self.reindex_root {
            Some(root) => Ok(root),
            None => Err(StoreError::KeyNotFound(DbKey::prefix_only(DatabaseStorePrefixes::ReachabilityReindexRoot.as_ref()))),
        }
    }
}

impl ReachabilityStoreReader for MemoryReachabilityStore {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.map.contains_key(&hash))
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        Ok(self.get_data(hash)?.interval)
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.get_data(hash)?.parent)
    }

    fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.get_data(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.get_data(hash)?.future_covering_set))
    }

    fn count(&self) -> Result<usize, StoreError> {
        Ok(self.map.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_basics() {
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
        let (hash, parent) = (7.into(), 15.into());
        let interval = Interval::maximal();
        store.insert(hash, parent, interval, 5).unwrap();
        let height = store.append_child(hash, 31.into()).unwrap();
        assert_eq!(height, 5);
        let children = store.get_children(hash).unwrap();
        println!("{children:?}");
        store.get_interval(7.into()).unwrap();
        println!("{children:?}");
    }
}
