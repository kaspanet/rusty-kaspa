use super::{caching::CachedDbAccess, errors::StoreError, DB};
use crate::model::api::hash::{Hash, HashArray};
use std::{collections::HashMap, sync::Arc};

pub trait RelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<HashArray, StoreError>;
    fn set_parents(&mut self, hash: Hash, parents: HashArray) -> Result<(), StoreError>;
}

const STORE_PREFIX: &[u8] = b"block-relations";

#[derive(Clone)]
pub struct DbRelationsStore {
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    cached_access: CachedDbAccess<Hash, Vec<Hash>>,
}

impl DbRelationsStore {
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

impl RelationsStore for DbRelationsStore {
    fn get_parents(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?))
    }

    fn set_parents(&mut self, hash: Hash, parents: HashArray) -> Result<(), StoreError> {
        self.cached_access.write(hash, &parents)?;
        Ok(())
    }
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
    fn get_parents(&self, hash: Hash) -> Result<HashArray, StoreError> {
        match self.map.get(&hash) {
            Some(parents) => Ok(HashArray::clone(parents)),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn set_parents(&mut self, hash: Hash, parents: HashArray) -> Result<(), StoreError> {
        self.map.insert(hash, parents);
        Ok(())
    }
}
