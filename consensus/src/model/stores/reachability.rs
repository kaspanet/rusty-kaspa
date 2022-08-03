use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::{caching::CachedDbAccess, caching::CachedDbItem, errors::StoreError, DB};
use crate::{
    model::api::hash::{Hash, HashArray},
    processes::reachability::interval::Interval,
};

#[derive(Clone, Serialize, Deserialize)]
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

pub trait ReachabilityStoreReader {
    fn has(&self, hash: Hash) -> Result<bool, StoreError>;
    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError>;
    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError>;
    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError>;
    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError>;
}
pub trait ReachabilityStore: ReachabilityStoreReader {
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError>;
    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError>;
    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError>;
    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError>;
    fn get_height(&self, hash: Hash) -> Result<u64, StoreError>;
    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError>;
    fn get_reindex_root(&self) -> Result<Hash, StoreError>;
}

const REINDEX_ROOT_KEY: &[u8] = b"reindex_root";

#[derive(Clone)]
pub struct DbReachabilityStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbAccess<Hash, ReachabilityData>, // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    reindex_root: CachedDbItem<Hash>,
    staged: Arc<AtomicBool>, // Used as a poor man mechanism to verify that reachability is never staged concurrently
}

impl DbReachabilityStore {
    pub fn new(db_path: &str, cache_size: u64) -> Self {
        let db = Arc::new(DB::open_default(db_path).unwrap());
        Self {
            raw_db: Arc::clone(&db),
            cached_access: CachedDbAccess::new(Arc::clone(&db), cache_size),
            reindex_root: CachedDbItem::new(db, REINDEX_ROOT_KEY),
            staged: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&self.raw_db),
            cached_access: CachedDbAccess::new(Arc::clone(&self.raw_db), cache_size),
            reindex_root: CachedDbItem::new(Arc::clone(&self.raw_db), REINDEX_ROOT_KEY),
            staged: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn new_staging(&self) -> StagingReachabilityStore {
        if self
            .staged
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            != Ok(false)
        {
            panic!("only a single reachability staging is allowed")
        }
        StagingReachabilityStore::new(self)
    }

    pub(self) fn release_staging(&self) {
        if self
            .staged
            .compare_exchange(true, false, Ordering::Relaxed, Ordering::Relaxed)
            != Ok(true)
        {
            panic!("expected staged to be true")
        }
    }
}

impl ReachabilityStore for DbReachabilityStore {
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if self.cached_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        let data = Arc::new(ReachabilityData::new(parent, interval, height));
        self.cached_access.write(hash, &data)?;
        Ok(())
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        let mut data = self.cached_access.read(hash)?;
        Arc::make_mut(&mut data).interval = interval;
        self.cached_access.write(hash, &data)?;
        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        let mut data = self.cached_access.read(hash)?;
        let height = data.height;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.children).push(child);
        self.cached_access.write(hash, &data)?;
        Ok(height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        let mut data = self.cached_access.read(hash)?;
        let height = data.height;
        let mut_data = Arc::make_mut(&mut data);
        Arc::make_mut(&mut mut_data.future_covering_set).insert(insertion_index, fci);
        self.cached_access.write(hash, &data)?;
        Ok(())
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        Ok(self.cached_access.read(hash)?.height)
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        self.reindex_root.write(&root)
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        self.reindex_root.read()
    }
}

impl ReachabilityStoreReader for DbReachabilityStore {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        self.cached_access.has(hash)
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        Ok(self.cached_access.read(hash)?.interval)
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        Ok(self.cached_access.read(hash)?.parent)
    }

    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.children))
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError> {
        Ok(Arc::clone(&self.cached_access.read(hash)?.future_covering_set))
    }
}

pub struct StagingReachabilityStore<'a> {
    inner_store: &'a DbReachabilityStore,
    staging_writes: HashMap<Hash, ReachabilityData>,
    staging_reindex_root: Option<Hash>,
}

impl<'a> StagingReachabilityStore<'a> {
    pub fn new(inner_store: &'a DbReachabilityStore) -> Self {
        Self { inner_store, staging_writes: HashMap::new(), staging_reindex_root: None }
    }

    pub fn commit(&mut self) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        for (k, v) in self.staging_writes.drain() {
            let data = Arc::new(v);
            self.inner_store
                .cached_access
                .write_batch(&mut batch, k, &data)?
        }
        if let Some(root) = self.staging_reindex_root {
            self.inner_store
                .reindex_root
                .write_batch(&mut batch, &root)?;
            self.staging_reindex_root = None; // Cleanup
        }
        self.inner_store.raw_db.write(batch)?;
        Ok(())
    }
}

impl Drop for StagingReachabilityStore<'_> {
    fn drop(&mut self) {
        self.inner_store.release_staging()
    }
}

impl ReachabilityStore for StagingReachabilityStore<'_> {
    fn insert(&mut self, hash: Hash, parent: Hash, interval: Interval, height: u64) -> Result<(), StoreError> {
        if self.inner_store.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        if let Vacant(e) = self.staging_writes.entry(hash) {
            e.insert(ReachabilityData::new(parent, interval, height));
            Ok(())
        } else {
            Err(StoreError::KeyAlreadyExists(hash.to_string()))
        }
    }

    fn set_interval(&mut self, hash: Hash, interval: Interval) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            data.interval = interval;
            return Ok(());
        }

        let mut data = (*self.inner_store.cached_access.read(hash)?).clone();
        data.interval = interval;
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn append_child(&mut self, hash: Hash, child: Hash) -> Result<u64, StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.children).push(child);
            return Ok(data.height);
        }

        let mut data = (*self.inner_store.cached_access.read(hash)?).clone();
        let height = data.height;
        Arc::make_mut(&mut data.children).push(child);
        self.staging_writes.insert(hash, data);

        Ok(height)
    }

    fn insert_future_covering_item(&mut self, hash: Hash, fci: Hash, insertion_index: usize) -> Result<(), StoreError> {
        if let Some(data) = self.staging_writes.get_mut(&hash) {
            Arc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
            return Ok(());
        }

        let mut data = (*self.inner_store.cached_access.read(hash)?).clone();
        Arc::make_mut(&mut data.future_covering_set).insert(insertion_index, fci);
        self.staging_writes.insert(hash, data);

        Ok(())
    }

    fn get_height(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.height)
        } else {
            Ok(self.inner_store.cached_access.read(hash)?.height)
        }
    }

    fn set_reindex_root(&mut self, root: Hash) -> Result<(), StoreError> {
        self.staging_reindex_root = Some(root);
        Ok(())
    }

    fn get_reindex_root(&self) -> Result<Hash, StoreError> {
        if let Some(root) = self.staging_reindex_root {
            Ok(root)
        } else {
            Ok(self.inner_store.get_reindex_root()?)
        }
    }
}

impl ReachabilityStoreReader for StagingReachabilityStore<'_> {
    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.staging_writes.contains_key(&hash) || self.inner_store.cached_access.has(hash)?)
    }

    fn get_interval(&self, hash: Hash) -> Result<Interval, StoreError> {
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.interval)
        } else {
            Ok(self
                .inner_store
                .cached_access
                .read(hash)?
                .interval)
        }
    }

    fn get_parent(&self, hash: Hash) -> Result<Hash, StoreError> {
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(data.parent)
        } else {
            Ok(self.inner_store.cached_access.read(hash)?.parent)
        }
    }

    fn get_children(&self, hash: Hash) -> Result<HashArray, StoreError> {
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(HashArray::clone(&data.children))
        } else {
            Ok(HashArray::clone(
                &self
                    .inner_store
                    .cached_access
                    .read(hash)?
                    .children,
            ))
        }
    }

    fn get_future_covering_set(&self, hash: Hash) -> Result<HashArray, StoreError> {
        if let Some(data) = self.staging_writes.get(&hash) {
            Ok(HashArray::clone(&data.future_covering_set))
        } else {
            Ok(HashArray::clone(
                &self
                    .inner_store
                    .cached_access
                    .read(hash)?
                    .future_covering_set,
            ))
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

impl ReachabilityStoreReader for MemoryReachabilityStore {
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
