use crate::processes::reachability::interval::Interval;
use kaspa_consensus_core::{
    blockhash::{self, BlockHashes},
    BlockHashMap, BlockHashSet, BlockHasher, BlockLevel, HashMapCustomHasher,
};
use kaspa_database::{
    prelude::{
        BatchDbWriter, Cache, CachePolicy, CachedDbAccess, CachedDbItem, DbKey, DbSetAccess, DbWriter, DirectDbWriter, StoreError, DB,
    },
    registry::{DatabaseStorePrefixes, SEPARATOR},
};
use kaspa_hashes::Hash;

use itertools::Itertools;
use kaspa_utils::mem_size::MemSizeEstimator;
use parking_lot::{RwLockUpgradableReadGuard, RwLockWriteGuard};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::Entry::{Occupied, Vacant},
    iter::once,
    sync::Arc,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ReachabilityData {
    pub parent: Hash,
    pub interval: Interval,
    pub height: u64,
}

impl MemSizeEstimator for ReachabilityData {}

impl ReachabilityData {
    pub fn new(parent: Hash, interval: Interval, height: u64) -> Self {
        Self { parent, interval, height }
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
    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<(), StoreError>;
    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError>;
    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError>;
    fn replace_child(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError>;
    fn replace_future_covering_item(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError>;
    fn delete(&mut self, hash: Hash) -> Result<(), StoreError>;
    fn get_height(&self, hash: Hash) -> Result<u64, StoreError>;
    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError>;
    fn get_reindex_root(&self) -> Result<Hash, StoreError>;
}

/// DB cached ordered `Set` access (manages a set per entry with cache and ordering).
/// Used both for the tree children set and for the future covering set (per block)
#[derive(Clone)]
struct DbReachabilitySet {
    access: DbSetAccess<Hash, Hash>,
    cache: Cache<Hash, BlockHashes>,
}

impl DbReachabilitySet {
    fn new(set_access: DbSetAccess<Hash, Hash>, set_cache: Cache<Hash, BlockHashes>) -> Self {
        Self { access: set_access, cache: set_cache }
    }

    fn append(&mut self, writer: impl DbWriter, hash: Hash, element: Hash) -> Result<(), StoreError> {
        if let Some(mut entry) = self.cache.get(&hash) {
            Arc::make_mut(&mut entry).push(element);
            self.cache.insert(hash, entry);
        }
        self.access.write(writer, hash, element)?;
        Ok(())
    }

    fn insert(&mut self, writer: impl DbWriter, hash: Hash, element: Hash, insertion_index: usize) -> Result<(), StoreError> {
        if let Some(mut entry) = self.cache.get(&hash) {
            Arc::make_mut(&mut entry).insert(insertion_index, element);
            self.cache.insert(hash, entry);
        }
        self.access.write(writer, hash, element)?;
        Ok(())
    }

    fn replace(
        &mut self,
        mut writer: impl DbWriter,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        if let Some(mut entry) = self.cache.get(&hash) {
            {
                let removed_elements =
                    Arc::make_mut(&mut entry).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
                debug_assert_eq!(replaced_hash, removed_elements.exactly_one().unwrap());
            }
            self.cache.insert(hash, entry);
        }
        self.access.delete(&mut writer, hash, replaced_hash)?;
        for added_element in replace_with.iter().copied() {
            self.access.write(&mut writer, hash, added_element)?;
        }
        Ok(())
    }

    fn commit_staging_entry(&mut self, mut writer: impl DbWriter, hash: Hash, entry: StagingSetEntry) -> Result<(), StoreError> {
        self.cache.insert(hash, entry.set);
        for removed_element in entry.deletions {
            self.access.delete(&mut writer, hash, removed_element)?;
        }
        for added_element in entry.additions {
            self.access.write(&mut writer, hash, added_element)?;
        }
        Ok(())
    }

    fn delete(&mut self, writer: impl DbWriter, hash: Hash) -> Result<(), StoreError> {
        self.cache.remove(&hash);
        self.access.delete_bucket(writer, hash)
    }

    fn read<K, F>(&self, hash: Hash, f: F) -> Result<BlockHashes, StoreError>
    where
        F: FnMut(&Hash) -> K,
        K: Ord,
    {
        if let Some(entry) = self.cache.get(&hash) {
            return Ok(entry);
        }

        let mut set: Vec<Hash> = self.access.bucket_iterator(hash).collect::<Result<_, _>>()?;
        // Apply the ordering rule before caching
        set.sort_by_cached_key(f);
        let set = BlockHashes::new(set);
        self.cache.insert(hash, set.clone());

        Ok(set)
    }
}

/// A DB + cache implementation of `ReachabilityStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbReachabilityStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, ReachabilityData, BlockHasher>, // Main access
    children_access: DbReachabilitySet,                          // Tree children
    fcs_access: DbReachabilitySet,                               // Future Covering Set
    reindex_root: CachedDbItem<Hash>,
    prefix_end: u8,
}

impl DbReachabilityStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, sets_cache_policy: CachePolicy) -> Self {
        Self::with_prefix_end(db, cache_policy, sets_cache_policy, DatabaseStorePrefixes::Separator.into())
    }

    pub fn with_block_level(db: Arc<DB>, cache_policy: CachePolicy, sets_cache_policy: CachePolicy, level: BlockLevel) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        Self::with_prefix_end(db, cache_policy, sets_cache_policy, level)
    }

    fn with_prefix_end(db: Arc<DB>, cache_policy: CachePolicy, sets_cache_policy: CachePolicy, prefix_end: u8) -> Self {
        let store_prefix = DatabaseStorePrefixes::Reachability.into_iter().chain(once(prefix_end)).collect_vec();
        let children_prefix = DatabaseStorePrefixes::ReachabilityTreeChildren.into_iter().chain(once(prefix_end)).collect_vec();
        let fcs_prefix = DatabaseStorePrefixes::ReachabilityFutureCoveringSet.into_iter().chain(once(prefix_end)).collect_vec();
        let reindex_root_prefix = DatabaseStorePrefixes::ReachabilityReindexRoot.into_iter().chain(once(prefix_end)).collect_vec();
        let access = CachedDbAccess::new(db.clone(), cache_policy, store_prefix);
        Self {
            db: db.clone(),
            access,
            children_access: DbReachabilitySet::new(DbSetAccess::new(db.clone(), children_prefix), Cache::new(sets_cache_policy)),
            fcs_access: DbReachabilitySet::new(DbSetAccess::new(db.clone(), fcs_prefix), Cache::new(sets_cache_policy)),
            reindex_root: CachedDbItem::new(db, reindex_root_prefix),
            prefix_end,
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy, sets_cache_policy: CachePolicy) -> Self {
        Self::with_prefix_end(Arc::clone(&self.db), cache_policy, sets_cache_policy, self.prefix_end)
    }
}

impl ReachabilityStore for DbReachabilityStore {
    fn init(&mut self, origin: Hash, capacity: Interval) -> Result<(), StoreError> {
        assert!(!self.access.has(origin)?);

        let data = ReachabilityData::new(blockhash::NONE, capacity, 0);
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
        let data = ReachabilityData::new(parent, interval, height);
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        data.interval = interval;
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<(), StoreError> {
        self.children_access.append(DirectDbWriter::new(&self.db), hash, child)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        self.fcs_access.insert(DirectDbWriter::new(&self.db), hash, fci, insertion_index)
    }

    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError> {
        let mut data = self.access.read(hash)?;
        data.parent = new_parent;
        self.access.write(DirectDbWriter::new(&self.db), hash, data)?;
        Ok(())
    }

    fn replace_child(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        self.children_access.replace(DirectDbWriter::new(&self.db), hash, replaced_hash, replaced_index, replace_with)
    }

    fn replace_future_covering_item(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        self.fcs_access.replace(DirectDbWriter::new(&self.db), hash, replaced_hash, replaced_index, replace_with)
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
        // Cached reachability sets are assumed to be ordered by interval in order to allow binary search over them
        self.children_access.read(hash, |&h| self.access.read(h).unwrap().interval)
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        // Cached reachability sets are assumed to be ordered by interval in order to allow binary search over them
        self.fcs_access.read(hash, |&h| self.access.read(h).unwrap().interval)
    }

    fn count(&self) -> Result<usize, StoreError> {
        Ok(self.access.iterator().count())
    }
}

/// Represents a staging set entry which was modified. The set can be either the tree children set or
/// the future covering set of a block. This struct saves the full cached updated set, as well as tracks the exact
/// changes that were made to it (additions/deletions). When committing the entry to the underlying DB store
/// these changes are used in order to efficiently update the DB only about the actual changes (thus avoiding quadratic disk writes).
/// Note that the cached set is still fully copied when reading/committing (in order to preserve order semantics). This too can be
/// optimized but for now these mem-copies don't seem to be a bottleneck so we favor the simplicity
struct StagingSetEntry {
    set: BlockHashes,        // The full cached (ordered) set
    additions: BlockHashSet, // additions diff
    deletions: BlockHashSet, // deletions diff
}

impl StagingSetEntry {
    fn new(cached_set: BlockHashes) -> Self {
        Self { set: cached_set, additions: Default::default(), deletions: Default::default() }
    }

    fn append(&mut self, element: Hash) {
        Arc::make_mut(&mut self.set).push(element);
        self.mark_addition(element);
    }

    fn insert(&mut self, element: Hash, insertion_index: usize) {
        Arc::make_mut(&mut self.set).insert(insertion_index, element);
        self.mark_addition(element);
    }

    fn replace(&mut self, replaced_hash: Hash, replaced_index: usize, replace_with: &[Hash]) {
        Arc::make_mut(&mut self.set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        self.mark_deletion(replaced_hash);
        for added_element in replace_with.iter().copied() {
            self.mark_addition(added_element);
        }
    }

    fn mark_addition(&mut self, addition: Hash) {
        if !self.deletions.remove(&addition) {
            self.additions.insert(addition);
        }
    }

    fn mark_deletion(&mut self, deletion: Hash) {
        if !self.additions.remove(&deletion) {
            self.deletions.insert(deletion);
        }
    }
}

pub struct StagingReachabilityStore<'a> {
    store_read: RwLockUpgradableReadGuard<'a, DbReachabilityStore>,
    staging_writes: BlockHashMap<ReachabilityData>,
    staging_children: BlockHashMap<StagingSetEntry>,
    staging_fcs: BlockHashMap<StagingSetEntry>,
    staging_deletions: BlockHashSet,
    staging_reindex_root: Option<Hash>,
}

impl<'a> StagingReachabilityStore<'a> {
    pub fn new(store_read: RwLockUpgradableReadGuard<'a, DbReachabilityStore>) -> Self {
        Self {
            store_read,
            staging_writes: Default::default(),
            staging_children: Default::default(),
            staging_fcs: Default::default(),
            staging_deletions: Default::default(),
            staging_reindex_root: None,
        }
    }

    pub fn commit(self, batch: &mut WriteBatch) -> Result<RwLockWriteGuard<'a, DbReachabilityStore>, StoreError> {
        let mut store_write = RwLockUpgradableReadGuard::upgrade(self.store_read);
        let mut writer = BatchDbWriter::new(batch);

        for (k, v) in self.staging_writes {
            store_write.access.write(&mut writer, k, v)?
        }

        for (k, v) in self.staging_children {
            store_write.children_access.commit_staging_entry(&mut writer, k, v)?;
        }

        for (k, v) in self.staging_fcs {
            store_write.fcs_access.commit_staging_entry(&mut writer, k, v)?;
        }

        // Deletions always come after mutations
        store_write.access.delete_many(&mut writer, &mut self.staging_deletions.iter().copied())?;

        for fully_deleted in self.staging_deletions {
            store_write.children_access.delete(&mut writer, fully_deleted)?;
            store_write.fcs_access.delete(&mut writer, fully_deleted)?;
        }

        if let Some(root) = self.staging_reindex_root {
            store_write.reindex_root.write(&mut writer, &root)?;
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

        let mut data = self.store_read.access.read(hash)?;
        data.interval = interval;
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<(), StoreError> {
        match self.staging_children.entry(hash) {
            Occupied(mut e) => {
                e.get_mut().append(child);
            }
            Vacant(e) => {
                let mut set = StagingSetEntry::new(self.store_read.get_children(hash)?);
                set.append(child);
                e.insert(set);
            }
        }

        Ok(())
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        match self.staging_fcs.entry(hash) {
            Occupied(mut e) => {
                e.get_mut().insert(fci, insertion_index);
            }
            Vacant(e) => {
                let mut set = StagingSetEntry::new(self.store_read.get_future_covering_set(hash)?);
                set.insert(fci, insertion_index);
                e.insert(set);
            }
        }

        Ok(())
    }

    fn set_parent(&mut self, hash: Hash, new_parent: Hash) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            data.parent = new_parent;
            return Ok(());
        }

        let mut data = self.store_read.access.read(hash)?;
        data.parent = new_parent;
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn replace_child(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        match self.staging_children.entry(hash) {
            Occupied(mut e) => {
                e.get_mut().replace(replaced_hash, replaced_index, replace_with);
            }
            Vacant(e) => {
                let mut set = StagingSetEntry::new(self.store_read.get_children(hash)?);
                set.replace(replaced_hash, replaced_index, replace_with);
                e.insert(set);
            }
        }

        Ok(())
    }

    fn replace_future_covering_item(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        match self.staging_fcs.entry(hash) {
            Occupied(mut e) => {
                e.get_mut().replace(replaced_hash, replaced_index, replace_with);
            }
            Vacant(e) => {
                let mut set = StagingSetEntry::new(self.store_read.get_future_covering_set(hash)?);
                set.replace(replaced_hash, replaced_index, replace_with);
                e.insert(set);
            }
        }

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

        if let Some(e) = self.staging_children.get(&hash) {
            Ok(BlockHashes::clone(&e.set))
        } else {
            self.store_read.get_children(hash)
        }
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.check_not_in_deletions(hash)?;

        if let Some(e) = self.staging_fcs.get(&hash) {
            Ok(BlockHashes::clone(&e.set))
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

/// Used only by the (test-intended) memory store. Groups all reachability data including
/// tree children and the future covering set unlike the DB store where they are decomposed
#[derive(Clone, Serialize, Deserialize)]
struct MemoryReachabilityData {
    pub children: BlockHashes,
    pub parent: Hash,
    pub interval: Interval,
    pub height: u64,
    pub future_covering_set: BlockHashes,
}

impl MemoryReachabilityData {
    pub fn new(parent: Hash, interval: Interval, height: u64) -> Self {
        Self { children: Arc::new(vec![]), parent, interval, height, future_covering_set: Arc::new(vec![]) }
    }
}

pub struct MemoryReachabilityStore {
    map: BlockHashMap<MemoryReachabilityData>,
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

    fn get_data_mut(&mut self, hash: Hash) -> Result<&mut MemoryReachabilityData, StoreError> {
        match self.map.get_mut(&hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Reachability.as_ref(), hash))),
        }
    }

    fn get_data(&self, hash: Hash) -> Result<&MemoryReachabilityData, StoreError> {
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
            e.insert(MemoryReachabilityData::new(parent, interval, height));
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

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        Arc::make_mut(&mut data.children).push(child);
        Ok(())
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

    fn replace_child(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        let removed_hash = Arc::make_mut(&mut data.children).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        debug_assert_eq!(replaced_hash, removed_hash.exactly_one().unwrap());
        Ok(())
    }

    fn replace_future_covering_item(
        &mut self,
        hash: Hash,
        replaced_hash: Hash,
        replaced_index: usize,
        replace_with: &[Hash],
    ) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        let removed_hash =
            Arc::make_mut(&mut data.future_covering_set).splice(replaced_index..replaced_index + 1, replace_with.iter().copied());
        debug_assert_eq!(replaced_hash, removed_hash.exactly_one().unwrap());
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
        store.append_child(hash, 31.into()).unwrap();
        let height = store.get_height(hash).unwrap();
        assert_eq!(height, 5);
        let children = store.get_children(hash).unwrap();
        println!("{children:?}");
        store.get_interval(7.into()).unwrap();
        println!("{children:?}");
    }
}
