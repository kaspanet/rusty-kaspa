use super::{caching::CachedDbAccess, errors::StoreError, DB};
use crate::model::api::hash::{Hash, HashArray};
use misc::uint256::Uint256;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};

pub type HashU8Map = Arc<HashMap<Hash, u8>>;

#[derive(Clone, Serialize, Deserialize)]
pub struct GhostdagData {
    pub blue_score: u64,
    pub blue_work: Uint256,
    pub selected_parent: Hash,
    pub mergeset_blues: HashArray,
    pub mergeset_reds: HashArray,
    pub blues_anticone_sizes: HashU8Map,
}

impl GhostdagData {
    pub fn new(
        blue_score: u64, blue_work: Uint256, selected_parent: Hash, mergeset_blues: HashArray,
        mergeset_reds: HashArray, blues_anticone_sizes: HashU8Map,
    ) -> Self {
        Self { blue_score, blue_work, selected_parent, mergeset_blues, mergeset_reds, blues_anticone_sizes }
    }

    pub fn with_selected_parent(selected_parent: Hash, k: u8) -> Self {
        let mut mergeset_blues: Vec<Hash> = Vec::with_capacity((k + 1) as usize);
        let mut blues_anticone_sizes: HashMap<Hash, u8> = HashMap::with_capacity(k as usize);
        mergeset_blues.push(selected_parent);
        blues_anticone_sizes.insert(selected_parent, 0);

        Self {
            blue_score: Default::default(),
            blue_work: Default::default(),
            selected_parent,
            mergeset_blues: HashArray::new(mergeset_blues),
            mergeset_reds: Default::default(),
            blues_anticone_sizes: HashU8Map::new(blues_anticone_sizes),
        }
    }
}

pub trait GhostdagStoreReader {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError>;
    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError>;
    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError>;
    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashU8Map, StoreError>;

    /// Returns full block data for the requested hash
    fn get_data(&self, hash: Hash, is_trusted_data: bool) -> Result<Arc<GhostdagData>, StoreError>;

    /// Check if the store contains data for the requested hash
    fn has(&self, hash: Hash, is_trusted_data: bool) -> Result<bool, StoreError>;
}

pub trait GhostdagStore: GhostdagStoreReader {
    /// Insert GHOSTDAG data for block `hash` into the store. Note that GHOSTDAG data
    /// is added once and never modified, so no need for specific setters for each element
    fn insert(&mut self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"block-ghostdag-data"; // TODO: use fixed value constants for for store prefixes

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

    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.mergeset_blues))
    }

    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.mergeset_reds))
    }

    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashU8Map, StoreError> {
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
    fn insert(&mut self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_access.write(hash, &data)?;
        Ok(())
    }
}

pub struct MemoryGhostdagStore {
    blue_score_map: HashMap<Hash, u64>,
    blue_work_map: HashMap<Hash, Uint256>,
    selected_parent_map: HashMap<Hash, Hash>,
    mergeset_blues_map: HashMap<Hash, HashArray>,
    mergeset_reds_map: HashMap<Hash, HashArray>,
    blues_anticone_sizes_map: HashMap<Hash, HashU8Map>,
}

impl MemoryGhostdagStore {
    pub fn new() -> Self {
        Self {
            blue_score_map: HashMap::new(),
            blue_work_map: HashMap::new(),
            selected_parent_map: HashMap::new(),
            mergeset_blues_map: HashMap::new(),
            mergeset_reds_map: HashMap::new(),
            blues_anticone_sizes_map: HashMap::new(),
        }
    }
}

impl Default for MemoryGhostdagStore {
    fn default() -> Self {
        Self::new()
    }
}

impl GhostdagStore for MemoryGhostdagStore {
    fn insert(&mut self, hash: Hash, data: Arc<GhostdagData>) -> Result<(), StoreError> {
        if self.has(hash, false)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.blue_score_map.insert(hash, data.blue_score);
        self.blue_work_map.insert(hash, data.blue_work);
        self.selected_parent_map
            .insert(hash, data.selected_parent);
        self.mergeset_blues_map
            .insert(hash, data.mergeset_blues.clone());
        self.mergeset_reds_map
            .insert(hash, data.mergeset_reds.clone());
        self.blues_anticone_sizes_map
            .insert(hash, data.blues_anticone_sizes.clone());
        Ok(())
    }
}

impl GhostdagStoreReader for MemoryGhostdagStore {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError> {
        match self.blue_score_map.get(&hash) {
            Some(blue_score) => Ok(*blue_score),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError> {
        match self.blue_work_map.get(&hash) {
            Some(blue_work) => Ok(*blue_work),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError> {
        match self.selected_parent_map.get(&hash) {
            Some(selected_parent) => Ok(*selected_parent),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError> {
        match self.mergeset_blues_map.get(&hash) {
            Some(mergeset_blues) => Ok(HashArray::clone(mergeset_blues)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError> {
        match self.mergeset_reds_map.get(&hash) {
            Some(mergeset_reds) => Ok(HashArray::clone(mergeset_reds)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashU8Map, StoreError> {
        match self.blues_anticone_sizes_map.get(&hash) {
            Some(sizes) => Ok(HashU8Map::clone(sizes)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_data(&self, hash: Hash, is_trusted_data: bool) -> Result<Arc<GhostdagData>, StoreError> {
        if !self.has(hash, is_trusted_data)? {
            return Err(StoreError::KeyNotFound(hash.to_string()));
        }
        Ok(Arc::new(GhostdagData::new(
            self.blue_score_map[&hash],
            self.blue_work_map[&hash],
            self.selected_parent_map[&hash],
            self.mergeset_blues_map[&hash].clone(),
            self.mergeset_reds_map[&hash].clone(),
            self.blues_anticone_sizes_map[&hash].clone(),
        )))
    }

    fn has(&self, hash: Hash, is_trusted_data: bool) -> Result<bool, StoreError> {
        Ok(self.blue_score_map.contains_key(&hash))
    }
}
