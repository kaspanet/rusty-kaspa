use crate::processes::ghostdag::ordering::SortableBlock;
use kaspa_consensus_core::trusted::ExternalGhostdagData;
use kaspa_consensus_core::{blockhash::BlockHashes, BlueWorkType};
use kaspa_consensus_core::{BlockHashMap, BlockHasher, BlockLevel, HashMapCustomHasher};
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbAccess, DbKey};
use kaspa_database::prelude::{CachePolicy, StoreError};
use kaspa_database::registry::{DatabaseStorePrefixes, SEPARATOR};
use kaspa_hashes::Hash;

use itertools::EitherOrBoth::{Both, Left, Right};
use itertools::Itertools;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::iter::once;
use std::mem::size_of;
use std::{cell::RefCell, sync::Arc};

/// Re-export for convenience
pub use kaspa_consensus_core::{HashKTypeMap, KType};

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct GhostdagData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: Hash,
    pub mergeset_blues: BlockHashes,
    pub mergeset_reds: BlockHashes,
    pub blues_anticone_sizes: HashKTypeMap,
}

#[derive(Clone, Serialize, Deserialize, Copy)]
pub struct CompactGhostdagData {
    pub blue_score: u64,
    pub blue_work: BlueWorkType,
    pub selected_parent: Hash,
}

impl MemSizeEstimator for GhostdagData {
    fn estimate_mem_bytes(&self) -> usize {
        let mut bytes = size_of::<Self>();
        bytes += (self.mergeset_blues.len() + self.mergeset_reds.len()) * size_of::<Hash>();
        bytes += self.blues_anticone_sizes.len() * size_of::<(Hash, KType)>();
        bytes
    }
}

impl MemSizeEstimator for CompactGhostdagData {}

impl From<&GhostdagData> for CompactGhostdagData {
    fn from(value: &GhostdagData) -> Self {
        Self { blue_score: value.blue_score, blue_work: value.blue_work, selected_parent: value.selected_parent }
    }
}

impl From<ExternalGhostdagData> for GhostdagData {
    fn from(value: ExternalGhostdagData) -> Self {
        Self {
            blue_score: value.blue_score,
            blue_work: value.blue_work,
            selected_parent: value.selected_parent,
            mergeset_blues: Arc::new(value.mergeset_blues),
            mergeset_reds: Arc::new(value.mergeset_reds),
            blues_anticone_sizes: Arc::new(value.blues_anticone_sizes),
        }
    }
}

impl From<&GhostdagData> for ExternalGhostdagData {
    fn from(value: &GhostdagData) -> Self {
        Self {
            blue_score: value.blue_score,
            blue_work: value.blue_work,
            selected_parent: value.selected_parent,
            mergeset_blues: (*value.mergeset_blues).clone(),
            mergeset_reds: (*value.mergeset_reds).clone(),
            blues_anticone_sizes: (*value.blues_anticone_sizes).clone(),
        }
    }
}

impl GhostdagData {
    pub fn new(
        blue_score: u64,
        blue_work: BlueWorkType,
        selected_parent: Hash,
        mergeset_blues: BlockHashes,
        mergeset_reds: BlockHashes,
        blues_anticone_sizes: HashKTypeMap,
    ) -> Self {
        Self { blue_score, blue_work, selected_parent, mergeset_blues, mergeset_reds, blues_anticone_sizes }
    }

    pub fn new_with_selected_parent(selected_parent: Hash, k: KType) -> Self {
        let mut mergeset_blues: Vec<Hash> = Vec::with_capacity((k + 1) as usize);
        let mut blues_anticone_sizes: BlockHashMap<KType> = BlockHashMap::with_capacity(k as usize);
        mergeset_blues.push(selected_parent);
        blues_anticone_sizes.insert(selected_parent, 0);

        Self {
            blue_score: Default::default(),
            blue_work: Default::default(),
            selected_parent,
            mergeset_blues: BlockHashes::new(mergeset_blues),
            mergeset_reds: Default::default(),
            blues_anticone_sizes: HashKTypeMap::new(blues_anticone_sizes),
        }
    }

    pub fn mergeset_size(&self) -> usize {
        self.mergeset_blues.len() + self.mergeset_reds.len()
    }

    /// Returns an iterator to the mergeset in ascending blue work order (tie-breaking by hash)
    pub fn ascending_mergeset_without_selected_parent<'a>(
        &'a self,
        store: &'a (impl GhostdagStoreReader + ?Sized),
    ) -> impl Iterator<Item = SortableBlock> + '_ {
        self.mergeset_blues
            .iter()
            .skip(1) // Skip the selected parent
            .cloned()
            .map(|h| SortableBlock::new(h, store.get_blue_work(h).unwrap()))
            .merge_join_by(
                self.mergeset_reds
                    .iter()
                    .cloned()
                    .map(|h| SortableBlock::new(h, store.get_blue_work(h).unwrap())),
                |a, b| a.cmp(b),
            )
            .map(|r| match r {
                Left(b) | Right(b) => b,
                Both(_, _) => panic!("distinct blocks are never equal"),
            })
    }

    /// Returns an iterator to the mergeset in descending blue work order (tie-breaking by hash)
    pub fn descending_mergeset_without_selected_parent<'a>(
        &'a self,
        store: &'a (impl GhostdagStoreReader + ?Sized),
    ) -> impl Iterator<Item = SortableBlock> + '_ {
        self.mergeset_blues
                .iter()
                .skip(1) // Skip the selected parent
                .rev()   // Reverse since blues and reds are stored with ascending blue work order
                .cloned()
                .map(|h| SortableBlock::new(h, store.get_blue_work(h).unwrap()))
                .merge_join_by(
                    self.mergeset_reds
                        .iter()
                        .rev() // Reverse
                        .cloned()
                        .map(|h| SortableBlock::new(h, store.get_blue_work(h).unwrap())),
                    |a, b| b.cmp(a), // Reverse
                )
                .map(|r| match r {
                    Left(b) | Right(b) => b,
                    Both(_, _) => panic!("distinct blocks are never equal"),
                })
    }

    /// Returns an iterator to the mergeset with no specified order (excluding the selected parent)
    pub fn unordered_mergeset_without_selected_parent(&self) -> impl Iterator<Item = Hash> + '_ {
        self.mergeset_blues
            .iter()
            .skip(1) // Skip the selected parent
            .cloned()
            .chain(self.mergeset_reds.iter().cloned())
    }

    /// Returns an iterator to the mergeset in topological consensus order -- starting with the selected parent,
    /// and adding the mergeset in increasing blue work order. Note that this is a topological order even though
    /// the selected parent has highest blue work by def -- since the mergeset is in its anticone.
    pub fn consensus_ordered_mergeset<'a>(
        &'a self,
        store: &'a (impl GhostdagStoreReader + ?Sized),
    ) -> impl Iterator<Item = Hash> + '_ {
        once(self.selected_parent).chain(self.ascending_mergeset_without_selected_parent(store).map(|s| s.hash))
    }

    /// Returns an iterator to the mergeset in topological consensus order without the selected parent
    pub fn consensus_ordered_mergeset_without_selected_parent<'a>(
        &'a self,
        store: &'a (impl GhostdagStoreReader + ?Sized),
    ) -> impl Iterator<Item = Hash> + '_ {
        self.ascending_mergeset_without_selected_parent(store).map(|s| s.hash)
    }

    /// Returns an iterator to the mergeset with no specified order (including the selected parent)
    pub fn unordered_mergeset(&self) -> impl Iterator<Item = Hash> + '_ {
        self.mergeset_blues.iter().cloned().chain(self.mergeset_reds.iter().cloned())
    }

    pub fn to_compact(&self) -> CompactGhostdagData {
        self.into()
    }

    pub fn add_blue(&mut self, block: Hash, blue_anticone_size: KType, block_blues_anticone_sizes: &BlockHashMap<KType>) {
        // Add the new blue block to mergeset blues
        BlockHashes::make_mut(&mut self.mergeset_blues).push(block);

        // Get a mut ref to internal anticone size map
        let blues_anticone_sizes = HashKTypeMap::make_mut(&mut self.blues_anticone_sizes);

        // Insert the new blue block with its blue anticone size to the map
        blues_anticone_sizes.insert(block, blue_anticone_size);

        // Insert/update map entries for blocks affected by this insertion
        for (blue, size) in block_blues_anticone_sizes {
            blues_anticone_sizes.insert(*blue, size + 1);
        }
    }

    pub fn add_red(&mut self, block: Hash) {
        // Add the new red block to mergeset reds
        BlockHashes::make_mut(&mut self.mergeset_reds).push(block);
    }

    pub fn finalize_score_and_work(&mut self, blue_score: u64, blue_work: BlueWorkType) {
        self.blue_score = blue_score;
        self.blue_work = blue_work;
    }
}
pub trait GhostdagStoreReader {
    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_blue_work(&self, hash: Hash) -> Result<BlueWorkType, StoreError>;
    fn get_selected_parent(&self, hash: Hash) -> Result<Hash, StoreError>;
    fn get_mergeset_blues(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_mergeset_reds(&self, hash: Hash) -> Result<BlockHashes, StoreError>;
    fn get_blues_anticone_sizes(&self, hash: Hash) -> Result<HashKTypeMap, StoreError>;

    /// Returns full block data for the requested hash
    fn get_data(&self, hash: Hash) -> Result<Arc<GhostdagData>, StoreError>;

    fn get_compact_data(&self, hash: Hash) -> Result<CompactGhostdagData, StoreError>;

    /// Check if the store contains data for the requested hash
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
}

pub trait GhostdagStore: GhostdagStoreReader {
    /// Insert GHOSTDAG data for block `hash` into the store. Note that GHOSTDAG data
    /// is added once and never modified, so no need for specific setters for each element.
    /// Additionally, this means writes are semantically "append-only", which is why
    /// we can keep the `insert` method non-mutable on self. See "Parallel Processing.md" for an overview.
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError>;
    fn delete(&self, hash: Hash) -> Result<(), StoreError>;
}

/// A DB + cache implementation of `GhostdagStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbGhostdagStore {
    db: Arc<DB>,
    level: BlockLevel,
    access: CachedDbAccess<Hash, Arc<GhostdagData>, BlockHasher>,
    compact_access: CachedDbAccess<Hash, CompactGhostdagData, BlockHasher>,
}

impl DbGhostdagStore {
    pub fn new(db: Arc<DB>, level: BlockLevel, cache_policy: CachePolicy, compact_cache_policy: CachePolicy) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let prefix = DatabaseStorePrefixes::Ghostdag.into_iter().chain(lvl_bytes).collect_vec();
        let compact_prefix = DatabaseStorePrefixes::GhostdagCompact.into_iter().chain(lvl_bytes).collect_vec();
        Self {
            db: Arc::clone(&db),
            level,
            access: CachedDbAccess::new(db.clone(), cache_policy, prefix),
            compact_access: CachedDbAccess::new(db, compact_cache_policy, compact_prefix),
        }
    }

    pub fn new_temp(
        db: Arc<DB>,
        level: BlockLevel,
        cache_policy: CachePolicy,
        compact_cache_policy: CachePolicy,
        temp_index: u8,
    ) -> Self {
        assert_ne!(SEPARATOR, level, "level {} is reserved for the separator", level);
        let lvl_bytes = level.to_le_bytes();
        let temp_index_bytes = temp_index.to_le_bytes();
        let prefix = DatabaseStorePrefixes::TempGhostdag.into_iter().chain(lvl_bytes).chain(temp_index_bytes).collect_vec();
        let compact_prefix =
            DatabaseStorePrefixes::TempGhostdagCompact.into_iter().chain(lvl_bytes).chain(temp_index_bytes).collect_vec();
        Self {
            db: Arc::clone(&db),
            level,
            access: CachedDbAccess::new(db.clone(), cache_policy, prefix),
            compact_access: CachedDbAccess::new(db, compact_cache_policy, compact_prefix),
        }
    }

    pub fn clone_with_new_cache(&self, cache_policy: CachePolicy, compact_cache_policy: CachePolicy) -> Self {
        Self::new(Arc::clone(&self.db), self.level, cache_policy, compact_cache_policy)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, data: &Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.access.write(BatchDbWriter::new(batch), hash, data.clone())?;
        self.compact_access.write(BatchDbWriter::new(batch), hash, data.to_compact())?;
        Ok(())
    }

    pub fn update_batch(&self, batch: &mut WriteBatch, hash: Hash, data: &Arc<GhostdagData>) -> Result<(), StoreError> {
        self.access.write(BatchDbWriter::new(batch), hash, data.clone())?;
        self.compact_access.write(BatchDbWriter::new(batch), hash, data.to_compact())?;
        Ok(())
    }

    pub fn delete_batch(&self, batch: &mut WriteBatch, hash: Hash) -> Result<(), StoreError> {
        self.compact_access.delete(BatchDbWriter::new(batch), hash)?;
        self.access.delete(BatchDbWriter::new(batch), hash)
    }
}

impl GhostdagStoreReader for DbGhostdagStore {
    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(ghostdag_data) = self.access.read_from_cache(hash) {
            return Ok(ghostdag_data.blue_score);
        }
        Ok(self.compact_access.read(hash)?.blue_score)
    }

    fn get_blue_work(&self, hash: Hash) -> Result<BlueWorkType, StoreError> {
        if let Some(ghostdag_data) = self.access.read_from_cache(hash) {
            return Ok(ghostdag_data.blue_work);
        }
        Ok(self.compact_access.read(hash)?.blue_work)
    }

    fn get_selected_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        if let Some(ghostdag_data) = self.access.read_from_cache(hash) {
            return Ok(ghostdag_data.selected_parent);
        }
        Ok(self.compact_access.read(hash)?.selected_parent)
    }

    fn get_mergeset_blues(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.access.read(hash)?.mergeset_blues))
    }

    fn get_mergeset_reds(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.access.read(hash)?.mergeset_reds))
    }

    fn get_blues_anticone_sizes(&self, hash: Hash) -> Result<HashKTypeMap, StoreError> {
        Ok(Arc::clone(&self.access.read(hash)?.blues_anticone_sizes))
    }

    fn get_data(&self, hash: Hash) -> Result<Arc<GhostdagData>, StoreError> {
        self.access.read(hash)
    }

    fn get_compact_data(&self, hash: Hash) -> Result<CompactGhostdagData, StoreError> {
        self.compact_access.read(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.access.has(hash)
    }
}

impl GhostdagStore for DbGhostdagStore {
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.access.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        if self.compact_access.has(hash)? {
            return Err(StoreError::DataInconsistency(format!("store has compact data for {} but is missing full data", hash)));
        }
        let mut batch = WriteBatch::default();
        self.access.write(BatchDbWriter::new(&mut batch), hash, data.clone())?;
        self.compact_access.write(BatchDbWriter::new(&mut batch), hash, data.to_compact())?;
        self.db.write(batch)?;
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        self.compact_access.delete(BatchDbWriter::new(&mut batch), hash)?;
        self.access.delete(BatchDbWriter::new(&mut batch), hash)?;
        self.db.write(batch)?;
        Ok(())
    }
}

/// An in-memory implementation of `GhostdagStore` trait to be used for tests.
/// Uses `RefCell` for interior mutability in order to workaround `insert`
/// being non-mutable.
pub struct MemoryGhostdagStore {
    blue_score_map: RefCell<BlockHashMap<u64>>,
    blue_work_map: RefCell<BlockHashMap<BlueWorkType>>,
    selected_parent_map: RefCell<BlockHashMap<Hash>>,
    mergeset_blues_map: RefCell<BlockHashMap<BlockHashes>>,
    mergeset_reds_map: RefCell<BlockHashMap<BlockHashes>>,
    blues_anticone_sizes_map: RefCell<BlockHashMap<HashKTypeMap>>,
}

impl MemoryGhostdagStore {
    pub fn new() -> Self {
        Self {
            blue_score_map: RefCell::new(BlockHashMap::new()),
            blue_work_map: RefCell::new(BlockHashMap::new()),
            selected_parent_map: RefCell::new(BlockHashMap::new()),
            mergeset_blues_map: RefCell::new(BlockHashMap::new()),
            mergeset_reds_map: RefCell::new(BlockHashMap::new()),
            blues_anticone_sizes_map: RefCell::new(BlockHashMap::new()),
        }
    }

    pub fn key_not_found_error(hash: Hash) -> StoreError {
        StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Ghostdag.as_ref(), hash))
    }
}

impl Default for MemoryGhostdagStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GhostdagStore for MemoryGhostdagStore {
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }
        self.blue_score_map.borrow_mut().insert(hash, data.blue_score);
        self.blue_work_map.borrow_mut().insert(hash, data.blue_work);
        self.selected_parent_map.borrow_mut().insert(hash, data.selected_parent);
        self.mergeset_blues_map.borrow_mut().insert(hash, data.mergeset_blues.clone());
        self.mergeset_reds_map.borrow_mut().insert(hash, data.mergeset_reds.clone());
        self.blues_anticone_sizes_map.borrow_mut().insert(hash, data.blues_anticone_sizes.clone());
        Ok(())
    }

    fn delete(&self, hash: Hash) -> Result<(), StoreError> {
        self.blue_score_map.borrow_mut().remove(&hash);
        self.blue_work_map.borrow_mut().remove(&hash);
        self.selected_parent_map.borrow_mut().remove(&hash);
        self.mergeset_blues_map.borrow_mut().remove(&hash);
        self.mergeset_reds_map.borrow_mut().remove(&hash);
        self.blues_anticone_sizes_map.borrow_mut().remove(&hash);
        Ok(())
    }
}

impl GhostdagStoreReader for MemoryGhostdagStore {
    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError> {
        match self.blue_score_map.borrow().get(&hash) {
            Some(blue_score) => Ok(*blue_score),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_blue_work(&self, hash: Hash) -> Result<BlueWorkType, StoreError> {
        match self.blue_work_map.borrow().get(&hash) {
            Some(blue_work) => Ok(*blue_work),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_selected_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        match self.selected_parent_map.borrow().get(&hash) {
            Some(selected_parent) => Ok(*selected_parent),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_mergeset_blues(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.mergeset_blues_map.borrow().get(&hash) {
            Some(mergeset_blues) => Ok(BlockHashes::clone(mergeset_blues)),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_mergeset_reds(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        match self.mergeset_reds_map.borrow().get(&hash) {
            Some(mergeset_reds) => Ok(BlockHashes::clone(mergeset_reds)),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_blues_anticone_sizes(&self, hash: Hash) -> Result<HashKTypeMap, StoreError> {
        match self.blues_anticone_sizes_map.borrow().get(&hash) {
            Some(sizes) => Ok(HashKTypeMap::clone(sizes)),
            None => Err(Self::key_not_found_error(hash)),
        }
    }

    fn get_data(&self, hash: Hash) -> Result<Arc<GhostdagData>, StoreError> {
        if !self.has(hash)? {
            return Err(Self::key_not_found_error(hash));
        }
        Ok(Arc::new(GhostdagData::new(
            self.blue_score_map.borrow()[&hash],
            self.blue_work_map.borrow()[&hash],
            self.selected_parent_map.borrow()[&hash],
            self.mergeset_blues_map.borrow()[&hash].clone(),
            self.mergeset_reds_map.borrow()[&hash].clone(),
            self.blues_anticone_sizes_map.borrow()[&hash].clone(),
        )))
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.blue_score_map.borrow().contains_key(&hash))
    }

    fn get_compact_data(&self, hash: Hash) -> Result<CompactGhostdagData, StoreError> {
        Ok(self.get_data(hash)?.to_compact())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlockHashSet;

    #[test]
    fn test_mergeset_iterators() {
        let store = MemoryGhostdagStore::new();

        let factory = |w: u64| {
            Arc::new(GhostdagData {
                blue_score: Default::default(),
                blue_work: w.into(),
                selected_parent: Default::default(),
                mergeset_blues: Default::default(),
                mergeset_reds: Default::default(),
                blues_anticone_sizes: Default::default(),
            })
        };

        // Blues
        store.insert(1.into(), factory(2)).unwrap();
        store.insert(2.into(), factory(7)).unwrap();
        store.insert(3.into(), factory(11)).unwrap();

        // Reds
        store.insert(4.into(), factory(4)).unwrap();
        store.insert(5.into(), factory(9)).unwrap();
        store.insert(6.into(), factory(11)).unwrap(); // Tie-breaking case

        let mut data = GhostdagData::new_with_selected_parent(1.into(), 5);
        data.add_blue(2.into(), Default::default(), &Default::default());
        data.add_blue(3.into(), Default::default(), &Default::default());

        data.add_red(4.into());
        data.add_red(5.into());
        data.add_red(6.into());

        let mut expected: Vec<Hash> = vec![4.into(), 2.into(), 5.into(), 3.into(), 6.into()];
        assert_eq!(expected, data.ascending_mergeset_without_selected_parent(&store).map(|b| b.hash).collect::<Vec<Hash>>());

        itertools::assert_equal(once(1.into()).chain(expected.iter().cloned()), data.consensus_ordered_mergeset(&store));

        expected.reverse();
        assert_eq!(expected, data.descending_mergeset_without_selected_parent(&store).map(|b| b.hash).collect::<Vec<Hash>>());

        // Use sets since the below functions have no order guarantee
        let expected = BlockHashSet::from_iter([4.into(), 2.into(), 5.into(), 3.into(), 6.into()]);
        assert_eq!(expected, data.unordered_mergeset_without_selected_parent().collect::<BlockHashSet>());

        let expected = BlockHashSet::from_iter([1.into(), 4.into(), 2.into(), 5.into(), 3.into(), 6.into()]);
        assert_eq!(expected, data.unordered_mergeset().collect::<BlockHashSet>());
    }
}
