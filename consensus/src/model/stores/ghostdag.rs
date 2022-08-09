use super::{caching::CachedDbAccess, errors::StoreError, DB};
use consensus_core::blockhash::BlockHashes;
use hashes::Hash;
use misc::uint256::Uint256;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

pub type KType = u8; // This type must be increased to u16 if we ever set GHOSTDAG K > 255
pub type HashKTypeMap = Arc<HashMap<Hash, KType>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct GhostdagData {
    pub blue_score: u64,
    pub blue_work: Uint256,
    pub selected_parent: Hash,
    pub mergeset_blues: BlockHashes,
    pub mergeset_reds: BlockHashes,
    pub blues_anticone_sizes: HashKTypeMap,
}

impl GhostdagData {
    pub fn new(
        blue_score: u64, blue_work: Uint256, selected_parent: Hash, mergeset_blues: BlockHashes,
        mergeset_reds: BlockHashes, blues_anticone_sizes: HashKTypeMap,
    ) -> Self {
        Self { blue_score, blue_work, selected_parent, mergeset_blues, mergeset_reds, blues_anticone_sizes }
    }

    pub fn new_with_selected_parent(selected_parent: Hash, k: KType) -> Self {
        let mut mergeset_blues: Vec<Hash> = Vec::with_capacity((k + 1) as usize);
        let mut blues_anticone_sizes: HashMap<Hash, KType> = HashMap::with_capacity(k as usize);
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
}

impl GhostdagData {
    pub fn add_blue(
        self: &mut Arc<Self>, block: Hash, blue_anticone_size: KType, block_blues_anticone_sizes: &HashMap<Hash, KType>,
    ) {
        // Extract mutable data
        let data = Arc::make_mut(self);

        // Add the new blue block to mergeset blues
        BlockHashes::make_mut(&mut data.mergeset_blues).push(block);

        // Get a mut ref to internal anticone size map
        let blues_anticone_sizes = HashKTypeMap::make_mut(&mut data.blues_anticone_sizes);

        // Insert the new blue block with its blue anticone size to the map
        blues_anticone_sizes.insert(block, blue_anticone_size);

        // Insert/update map entries for blocks affected by this insertion
        for (blue, size) in block_blues_anticone_sizes {
            blues_anticone_sizes.insert(*blue, size + 1);
        }
    }

    pub fn add_red(self: &mut Arc<Self>, block: Hash) {
        // Extract mutable data
        let data = Arc::make_mut(self);

        // Add the new red block to mergeset reds
        BlockHashes::make_mut(&mut data.mergeset_reds).push(block);
    }

    pub fn finalize_score_and_work(self: &mut Arc<Self>, blue_score: u64, blue_work: Uint256) {
        let data = Arc::make_mut(self);
        data.blue_score = blue_score;
        data.blue_work = blue_work;
    }
}

pub trait GhostdagStoreReader {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError>;
    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError>;
    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError>;
    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError>;
    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError>;
    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashKTypeMap, StoreError>;

    /// Returns full block data for the requested hash
    fn get_data(&self, hash: Hash, is_trusted_data: bool) -> Result<Arc<GhostdagData>, StoreError>;

    /// Check if the store contains data for the requested hash
    fn has(&self, hash: Hash, is_trusted_data: bool) -> Result<bool, StoreError>;
}

pub trait GhostdagStore: GhostdagStoreReader {
    /// Insert GHOSTDAG data for block `hash` into the store. Note that GHOSTDAG data
    /// is added once and never modified, so no need for specific setters for each element
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"block-ghostdag-data";

#[derive(Clone)]
pub struct DbGhostdagStore {
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    cached_access: CachedDbAccess<Hash, GhostdagData>,
}

impl DbGhostdagStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&self.raw_db),
            cached_access: CachedDbAccess::new(Arc::clone(&self.raw_db), cache_size, STORE_PREFIX),
        }
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access
            .write_batch(batch, hash, &data)?;
        Ok(())
    }
}

impl GhostdagStoreReader for DbGhostdagStore {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError> {
        Ok(self.cached_access.read(hash)?.blue_score)
    }

    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError> {
        Ok(self.cached_access.read(hash)?.blue_work)
    }

    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError> {
        Ok(self.cached_access.read(hash)?.selected_parent)
    }

    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.mergeset_blues))
    }

    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.mergeset_reds))
    }

    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashKTypeMap, StoreError> {
        Ok(Arc::clone(
            &self
                .cached_access
                .read(hash)?
                .blues_anticone_sizes,
        ))
    }

    fn get_data(&self, hash: Hash, is_trusted_data: bool) -> Result<Arc<GhostdagData>, StoreError> {
        self.cached_access.read(hash)
    }

    fn has(&self, hash: Hash, is_trusted_data: bool) -> Result<bool, StoreError> {
        self.cached_access.has(hash)
    }
}

impl GhostdagStore for DbGhostdagStore {
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(hash, &data)?;
        Ok(())
    }
}

pub struct MemoryGhostdagStore {
    blue_score_map: RefCell<HashMap<Hash, u64>>,
    blue_work_map: RefCell<HashMap<Hash, Uint256>>,
    selected_parent_map: RefCell<HashMap<Hash, Hash>>,
    mergeset_blues_map: RefCell<HashMap<Hash, BlockHashes>>,
    mergeset_reds_map: RefCell<HashMap<Hash, BlockHashes>>,
    blues_anticone_sizes_map: RefCell<HashMap<Hash, HashKTypeMap>>,
}

impl MemoryGhostdagStore {
    pub fn new() -> Self {
        Self {
            blue_score_map: RefCell::new(HashMap::new()),
            blue_work_map: RefCell::new(HashMap::new()),
            selected_parent_map: RefCell::new(HashMap::new()),
            mergeset_blues_map: RefCell::new(HashMap::new()),
            mergeset_reds_map: RefCell::new(HashMap::new()),
            blues_anticone_sizes_map: RefCell::new(HashMap::new()),
        }
    }
}

impl Default for MemoryGhostdagStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GhostdagStore for MemoryGhostdagStore {
    fn insert(&self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.has(hash, false)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.blue_score_map
            .borrow_mut()
            .insert(hash, data.blue_score);
        self.blue_work_map
            .borrow_mut()
            .insert(hash, data.blue_work);
        self.selected_parent_map
            .borrow_mut()
            .insert(hash, data.selected_parent);
        self.mergeset_blues_map
            .borrow_mut()
            .insert(hash, data.mergeset_blues.clone());
        self.mergeset_reds_map
            .borrow_mut()
            .insert(hash, data.mergeset_reds.clone());
        self.blues_anticone_sizes_map
            .borrow_mut()
            .insert(hash, data.blues_anticone_sizes.clone());
        Ok(())
    }
}

impl GhostdagStoreReader for MemoryGhostdagStore {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError> {
        match self.blue_score_map.borrow().get(&hash) {
            Some(blue_score) => Ok(*blue_score),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError> {
        match self.blue_work_map.borrow().get(&hash) {
            Some(blue_work) => Ok(*blue_work),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError> {
        match self.selected_parent_map.borrow().get(&hash) {
            Some(selected_parent) => Ok(*selected_parent),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError> {
        match self.mergeset_blues_map.borrow().get(&hash) {
            Some(mergeset_blues) => Ok(BlockHashes::clone(mergeset_blues)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<BlockHashes, StoreError> {
        match self.mergeset_reds_map.borrow().get(&hash) {
            Some(mergeset_reds) => Ok(BlockHashes::clone(mergeset_reds)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashKTypeMap, StoreError> {
        match self.blues_anticone_sizes_map.borrow().get(&hash) {
            Some(sizes) => Ok(HashKTypeMap::clone(sizes)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_data(&self, hash: Hash, is_trusted_data: bool) -> Result<Arc<GhostdagData>, StoreError> {
        if !self.has(hash, is_trusted_data)? {
            return Err(StoreError::KeyNotFound(hash.to_string()));
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

    fn has(&self, hash: Hash, is_trusted_data: bool) -> Result<bool, StoreError> {
        Ok(self.blue_score_map.borrow().contains_key(&hash))
    }
}
