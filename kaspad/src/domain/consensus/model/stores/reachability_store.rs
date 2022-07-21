use std::collections::HashMap;

use super::errors::StoreError;
use crate::domain::consensus::{model::api::hash::DomainHash, processes::reachability::interval::Interval};

#[derive(Clone)]
pub struct ReachabilityData {
    pub children: Vec<DomainHash>,
    pub parent: DomainHash,
    pub interval: Interval,
    pub future_covering_set: Vec<DomainHash>,
}

impl ReachabilityData {
    pub fn new(parent: &DomainHash, interval: Interval) -> Self {
        Self { children: vec![], parent: *parent, interval, future_covering_set: vec![] }
    }
}

pub trait ReachabilityStore {
    fn init(&mut self, hash: &DomainHash, parent: &DomainHash, interval: Interval) -> Result<(), StoreError>;

    fn set_interval(&mut self, hash: &DomainHash, interval: Interval) -> Result<(), StoreError>;
    fn append_child(&mut self, hash: &DomainHash, child: &DomainHash) -> Result<(), StoreError>;
    fn insert_future_covering_item(
        &mut self, hash: &DomainHash, fci: &DomainHash, insertion_index: usize,
    ) -> Result<(), StoreError>;

    fn has(&self, hash: &DomainHash) -> Result<bool, StoreError>;

    fn get_interval(&self, hash: &DomainHash) -> Result<Interval, StoreError>;
    fn get_parent(&self, hash: &DomainHash) -> Result<&DomainHash, StoreError>;
    fn get_children(&self, hash: &DomainHash) -> Result<&[DomainHash], StoreError>;
    fn get_future_covering_set(&self, hash: &DomainHash) -> Result<&[DomainHash], StoreError>;
}

pub struct MemoryReachabilityStore {
    map: HashMap<DomainHash, ReachabilityData>,
}

impl MemoryReachabilityStore {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    fn get_data_mut(&mut self, hash: &DomainHash) -> Result<&mut ReachabilityData, StoreError> {
        match self.map.get_mut(hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound),
        }
    }

    fn get_data(&self, hash: &DomainHash) -> Result<&ReachabilityData, StoreError> {
        match self.map.get(hash) {
            Some(data) => Ok(data),
            None => Err(StoreError::KeyNotFound),
        }
    }
}

impl ReachabilityStore for MemoryReachabilityStore {
    fn init(&mut self, hash: &DomainHash, parent: &DomainHash, interval: Interval) -> Result<(), StoreError> {
        if self.map.contains_key(hash) {
            Err(StoreError::KeyAlreadyExists)
        } else {
            self.map
                .insert(*hash, ReachabilityData::new(parent, interval));
            Ok(())
        }
    }

    fn set_interval(&mut self, hash: &DomainHash, interval: Interval) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        data.interval = interval;
        Ok(())
    }

    fn append_child(&mut self, hash: &DomainHash, child: &DomainHash) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        data.children.push(*child);
        Ok(())
    }

    fn insert_future_covering_item(
        &mut self, hash: &DomainHash, fci: &DomainHash, insertion_index: usize,
    ) -> Result<(), StoreError> {
        let data = self.get_data_mut(hash)?;
        data.future_covering_set
            .insert(insertion_index, *fci);
        Ok(())
    }

    fn has(&self, hash: &DomainHash) -> Result<bool, StoreError> {
        Ok(self.map.contains_key(hash))
    }

    fn get_interval(&self, hash: &DomainHash) -> Result<Interval, StoreError> {
        Ok(self.get_data(hash)?.interval)
    }

    fn get_parent(&self, hash: &DomainHash) -> Result<&DomainHash, StoreError> {
        Ok(&self.get_data(hash)?.parent)
    }

    fn get_children(&self, hash: &DomainHash) -> Result<&[DomainHash], StoreError> {
        Ok(self.get_data(hash)?.children.as_slice())
    }

    fn get_future_covering_set(&self, hash: &DomainHash) -> Result<&[DomainHash], StoreError> {
        Ok(self
            .get_data(hash)?
            .future_covering_set
            .as_slice())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_basics() {
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
        let (hash, parent) = (DomainHash::from_u64(7), DomainHash::from_u64(15));
        let interval = Interval::maximal();
        store.init(&hash, &parent, interval).unwrap();
        // let 
    }
}
