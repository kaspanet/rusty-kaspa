use super::errors::StoreError;
use crate::model::api::hash::{Hash, HashArray};
use std::collections::HashMap;

pub trait RelationsStore {
    fn get_parents(&self, hash: &Hash) -> Result<HashArray, StoreError>;
    fn set_parents(&mut self, hash: Hash, parents: HashArray);
}

pub struct MemoryRelationsStore {
    map: HashMap<Hash, HashArray>,
}

impl MemoryRelationsStore {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }
}

impl Default for MemoryRelationsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl RelationsStore for MemoryRelationsStore {
    fn get_parents(&self, hash: &Hash) -> Result<HashArray, StoreError> {
        match self.map.get(hash) {
            Some(parents) => Ok(HashArray::clone(parents)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn set_parents(&mut self, hash: Hash, parents: HashArray) {
        self.map.insert(hash, parents);
    }
}
