use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    rc::Rc,
};

use super::errors::StoreError;
use crate::{
    model::api::hash::{Hash, HashArray},
    processes::reachability::interval::Interval,
};

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
        Self { children: Rc::new(vec![]), parent, interval, height, future_covering_set: Rc::new(vec![]) }
    }
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
        Rc::make_mut(&mut data.children).push(child);
        Ok(data.height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        Rc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
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
        Ok(Rc::clone(&self.get_data(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Rc::clone(&self.get_data(hash)?.future_covering_set))
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
