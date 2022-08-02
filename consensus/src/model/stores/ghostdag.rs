use super::errors::StoreError;
use crate::model::api::hash::{Hash, HashArray};
use misc::uint256::Uint256;
use std::{collections::HashMap, sync::Arc};

pub type HashU8Map = Arc<HashMap<Hash, u8>>;

pub trait GhostdagStoreReader {
    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError>;
    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError>;
    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError>;
    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<HashU8Map, StoreError>;
}

pub trait GhostdagStore: GhostdagStoreReader {
    /// Insert GHOSTDAG data for block `hash` into the store. Note that GHOSTDAG data
    /// is added once and never modified, so no need for specific setters for each element
    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self, hash: Hash, blue_score: u64, blue_work: Uint256, selected_parent: Hash, mergeset_blues: HashArray,
        mergeset_reds: HashArray, blues_anticone_sizes: HashU8Map,
    ) -> Result<(), StoreError>;
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
    fn insert(
        &mut self, hash: Hash, blue_score: u64, blue_work: Uint256, selected_parent: Hash, mergeset_blues: HashArray,
        mergeset_reds: HashArray, blues_anticone_sizes: HashU8Map,
    ) -> Result<(), StoreError> {
        if self.blue_score_map.contains_key(&hash) {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.blue_score_map.insert(hash, blue_score);
        self.blue_work_map.insert(hash, blue_work);
        self.selected_parent_map
            .insert(hash, selected_parent);
        self.mergeset_blues_map
            .insert(hash, mergeset_blues);
        self.mergeset_reds_map.insert(hash, mergeset_reds);
        self.blues_anticone_sizes_map
            .insert(hash, blues_anticone_sizes);
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
}
