use super::errors::StoreError;
use crate::model::api::hash::{Hash, HashArray};
use misc::uint256::Uint256;
use std::{collections::HashMap, rc::Rc};

pub trait GhostdagStore {
    fn set_blue_score(&mut self, hash: Hash, blue_score: u64) -> Result<(), StoreError>;
    fn set_blue_work(&mut self, hash: Hash, blue_work: Uint256) -> Result<(), StoreError>;
    fn set_selected_parent(&mut self, hash: Hash, selected_parent: Hash) -> Result<(), StoreError>;
    fn set_mergeset_blues(&mut self, hash: Hash, blues: HashArray) -> Result<(), StoreError>;
    fn set_mergeset_reds(&mut self, hash: Hash, reds: HashArray) -> Result<(), StoreError>;
    fn set_blues_anticone_sizes(&mut self, hash: Hash, sizes: Rc<HashMap<Hash, u8>>) -> Result<(), StoreError>;

    fn get_blue_score(&self, hash: Hash, is_trusted_data: bool) -> Result<u64, StoreError>;
    fn get_blue_work(&self, hash: Hash, is_trusted_data: bool) -> Result<Uint256, StoreError>;
    fn get_selected_parent(&self, hash: Hash, is_trusted_data: bool) -> Result<Hash, StoreError>;
    fn get_mergeset_blues(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError>;
    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<Rc<HashMap<Hash, u8>>, StoreError>;
}

pub struct MemoryGhostdagStore {
    blue_score_map: HashMap<Hash, u64>,
    blue_work_map: HashMap<Hash, Uint256>,
    selected_parent_map: HashMap<Hash, Hash>,
    mergeset_blues_map: HashMap<Hash, HashArray>,
    mergeset_reds_map: HashMap<Hash, HashArray>,
    blues_anticone_sizes_map: HashMap<Hash, Rc<HashMap<Hash, u8>>>,
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
    fn set_blue_score(&mut self, hash: Hash, blue_score: u64) -> Result<(), StoreError> {
        self.blue_score_map.insert(hash, blue_score);
        Ok(())
    }

    fn set_blue_work(&mut self, hash: Hash, blue_work: Uint256) -> Result<(), StoreError> {
        self.blue_work_map.insert(hash, blue_work);
        Ok(())
    }

    fn set_selected_parent(&mut self, hash: Hash, selected_parent: Hash) -> Result<(), StoreError> {
        self.selected_parent_map
            .insert(hash, selected_parent);
        Ok(())
    }

    fn set_mergeset_blues(&mut self, hash: Hash, blues: HashArray) -> Result<(), StoreError> {
        self.mergeset_blues_map.insert(hash, blues);
        Ok(())
    }

    fn set_mergeset_reds(&mut self, hash: Hash, reds: HashArray) -> Result<(), StoreError> {
        self.mergeset_reds_map.insert(hash, reds);
        Ok(())
    }

    fn set_blues_anticone_sizes(&mut self, hash: Hash, sizes: Rc<HashMap<Hash, u8>>) -> Result<(), StoreError> {
        self.blues_anticone_sizes_map.insert(hash, sizes);
        Ok(())
    }

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
            Some(mergeset_blues) => Ok(Rc::clone(mergeset_blues)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_mergeset_reds(&self, hash: Hash, is_trusted_data: bool) -> Result<HashArray, StoreError> {
        match self.mergeset_reds_map.get(&hash) {
            Some(mergeset_reds) => Ok(Rc::clone(mergeset_reds)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_blues_anticone_sizes(&self, hash: Hash, is_trusted_data: bool) -> Result<Rc<HashMap<Hash, u8>>, StoreError> {
        match self.blues_anticone_sizes_map.get(&hash) {
            Some(sizes) => Ok(Rc::clone(sizes)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }
}
