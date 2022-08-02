use moka::sync::Cache;
use rocksdb::{DBWithThreadMode, MultiThreaded};
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    sync::Arc,
    sync::RwLock,
};

type DB = DBWithThreadMode<MultiThreaded>;

use super::errors::StoreError;
use crate::{model::api::hash::Hash, processes::reachability::interval::Interval};

type HashArray = Arc<Vec<Hash>>;

#[derive(Clone)]
pub struct ReachabilityData {
    pub children: HashArray,
    pub parent: Hash,
    pub interval: Interval,
    pub height: u64,
    pub future_covering_set: HashArray,
}

impl ReachabilityData {
    pub fn new(parent: Hash, interval: Interval, height: u64) -> Self {
        Self { children: Arc::new(vec![]), parent, interval, height, future_covering_set: Arc::new(vec![]) }
    }
}

impl From<SerializableReachabilityData> for ReachabilityData {
    fn from(srd: SerializableReachabilityData) -> Self {
        Self {
            children: Arc::new(srd.children),
            parent: srd.parent,
            interval: srd.interval,
            height: srd.height,
            future_covering_set: Arc::new(srd.future_covering_set),
        }
    }
}

impl From<&ReachabilityData> for SerializableReachabilityData {
    fn from(srd: &ReachabilityData) -> Self {
        Self {
            children: (*srd.children).clone(), // TODO: `Arc::unwrap_or_clone` would be perfect here (and below for FCS) but is unstable yet
            parent: srd.parent,
            interval: srd.interval,
            height: srd.height,
            future_covering_set: (*srd.future_covering_set).clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SerializableReachabilityData {
    pub children: Vec<Hash>,
    pub parent: Hash,
    pub interval: Interval,
    pub height: u64,
    pub future_covering_set: Vec<Hash>,
}

pub trait ReachabilityStore {
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError>;
    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError>;
    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError>;
    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError>;
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError>;
    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError>;
    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError>;
    fn get_height(&self, hash: Hash) -> Result<u64, StoreError>;
    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError>;
    fn get_reindex_root(&self) -> Result<Hash, StoreError>;
}

#[derive(Clone)]
pub struct DbReachabilityStore {
    db: Arc<DB>,
    // The cache type supports shallow cloning and manages ref counting internally, so no need for Arc
    cache: Cache<Hash, Arc<ReachabilityData>>,
    reindex_root: Arc<RwLock<Option<Hash>>>,
}

impl DbReachabilityStore {
    pub fn new(db_path: &str, cache_size: u64) -> Self {
        Self {
            db: Arc::new(DB::open_default(db_path).unwrap()),
            cache: Cache::new(cache_size),
            reindex_root: Arc::new(RwLock::new(None)),
        }
    }

    fn read(&self, hash: Hash, should_cache: bool) -> Result<Arc<ReachabilityData>, StoreError> {
        if let Some(data) = self.cache.get(&hash) {
            Ok(data)
        } else if let Some(slice) = self.db.get_pinned(hash)? {
            let deserial_data: SerializableReachabilityData = bincode::deserialize(&slice)?;
            let data: Arc<ReachabilityData> = Arc::new(deserial_data.into());
            if should_cache {
                self.cache.insert(hash, Arc::clone(&data));
            }
            Ok(data)
        } else {
            Err(StoreError::KeyNotFound(hash.to_string()))
        }
    }

    fn read_and_cache(&self, hash: Hash) -> Result<Arc<ReachabilityData>, StoreError> {
        self.read(hash, true)
    }

    fn read_for_write(&self, hash: Hash) -> Result<Arc<ReachabilityData>, StoreError> {
        self.read(hash, false)
    }

    fn write(&self, hash: Hash, data: Arc<ReachabilityData>) -> Result<(), StoreError> {
        let cache = &self.cache; //.clone(); // See TODO above
        cache.insert(hash, Arc::clone(&data));
        let serial_data: SerializableReachabilityData = data.as_ref().into();
        let bin_data = bincode::serialize(&serial_data)?;
        self.db.put(hash, bin_data)?;
        Ok(())
    }
}

impl ReachabilityStore for DbReachabilityStore {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.cache.contains_key(&hash) || self.db.get_pinned(hash)?.is_some())
    }

    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if self.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        let data = Arc::new(ReachabilityData::new(parent, interval, height));
        self.write(hash, data)?;
        Ok(())
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        let mut data = self.read_for_write(hash)?;
        Arc::make_mut(&mut data).interval = interval;
        self.write(hash, data)?;
        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        let mut data = self.read_for_write(hash)?;
        let height = data.height;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.children).push(child);
        self.write(hash, data)?;
        Ok(height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        let mut data = self.read_for_write(hash)?;
        let height = data.height;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.children).insert(insertion_index, fci);
        self.write(hash, data)?;
        Ok(())
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        Ok(self.read_and_cache(hash)?.interval)
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.read_and_cache(hash)?.parent)
    }

    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.read_and_cache(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.read_and_cache(hash)?.future_covering_set))
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        Ok(self.read_and_cache(hash)?.height)
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        *self.reindex_root.write().unwrap() = Some(root);
        let bin_data = bincode::serialize(&root)?;
        self.db.put(b"reindex_root", bin_data)?;
        Ok(())
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        if let Some(root) = *self.reindex_root.read().unwrap() {
            Ok(root)
        } else if let Some(slice) = self.db.get_pinned(b"reindex_root")? {
            let root: Hash = bincode::deserialize(&slice)?;
            *self.reindex_root.write().unwrap() = Some(root);
            Ok(root)
        } else {
            Err(StoreError::KeyNotFound("reindex_root".to_string()))
        }
    }
}

pub struct MemoryReachabilityStore {
    map: HashMap<Hash, ReachabilityData>,
    reindex_root: Option<Hash>,
}

impl Default for MemoryReachabilityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryReachabilityStore {
    pub fn new() -> Self {
        Self { map: HashMap::new(), reindex_root: None }
    }

    fn get_data_mut(&mut self, hash: Hash) -> Result<&mut ReachabilityData, StoreError> {
        match self.map.get_mut(&hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }

    fn get_data(&self, hash: Hash) -> Result<&ReachabilityData, StoreError> {
        match self.map.get(&hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound(hash.to_string())),
        }
    }
}

impl ReachabilityStore for MemoryReachabilityStore {
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if let Vacant(e) = self.map.entry(hash) {
            e.insert(ReachabilityData::new(parent, interval, height));
            Ok(())
        } else {
            Err(StoreError::KeyAlreadyExists(hash.to_string()))
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

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.map.contains_key(&hash))
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        Ok(self.get_data(hash)?.interval)
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.get_data(hash)?.parent)
    }

    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.get_data(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.get_data(hash)?.future_covering_set))
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
            None => Err(StoreError::KeyNotFound("reindex root".to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_basics() {
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
        let (hash, parent) = (Hash::from_u64(7), Hash::from_u64(15));
        let interval = Interval::maximal();
        store.insert(hash, parent, interval, 5).unwrap();
        let height = store
            .append_child(hash, Hash::from_u64(31))
            .unwrap();
        assert_eq!(height, 5);
        let children = store.get_children(hash).unwrap();
        println!("{:?}", children);
        store.get_interval(Hash::from_u64(7)).unwrap();
        println!("{:?}", children);
    }
}
