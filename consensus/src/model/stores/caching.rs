use super::{errors::StoreError, DB};
use moka::sync::Cache;
use rocksdb::WriteBatch;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{Arc, RwLock};

const SEP: u8 = b'/';

struct DbKey {
    path: Vec<u8>,
}

impl DbKey {
    fn new<TKey: Copy + AsRef<[u8]>>(prefix: &[u8], key: TKey) -> Self {
        Self {
            path: prefix
                .iter()
                .chain(std::iter::once(&SEP))
                .chain(key.as_ref().iter())
                .copied()
                .collect(),
        }
    }
}

impl AsRef<[u8]> for DbKey {
    fn as_ref(&self) -> &[u8] {
        &self.path
    }
}

/// A concurrent DB store with typed caching.
#[derive(Clone)]
pub struct CachedDbAccess<TKey, TData>
where
    TKey: std::hash::Hash + Eq + Send + Sync + 'static,
    TData: Clone + Send + Sync + 'static,
{
    db: Arc<DB>,
    // The moka cache type supports shallow cloning and manages
    // ref counting internally, so no need for Arc
    cache: Cache<TKey, Arc<TData>>,

    // DB bucket/path (TODO: eventually this must become dynamic in
    // order to support `active/inactive` consensus instances)
    prefix: &'static [u8],
}

impl<TKey, TData> CachedDbAccess<TKey, TData>
where
    TKey: std::hash::Hash + Eq + Send + Sync + 'static,
    TData: Clone + Send + Sync + 'static,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: &'static [u8]) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&key)
            || self
                .db
                .get_pinned(DbKey::new(self.prefix, key))?
                .is_some())
    }

    pub fn read(&self, key: TKey) -> Result<Arc<TData>, StoreError>
    where
        TKey: Copy + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else if let Some(slice) = self.db.get_pinned(DbKey::new(self.prefix, key))? {
            let data: Arc<TData> = Arc::new(bincode::deserialize(&slice)?);
            self.cache.insert(key, Arc::clone(&data));
            Ok(data)
        } else {
            Err(StoreError::KeyNotFound(key.to_string()))
        }
    }

    pub fn write(&self, key: TKey, data: &Arc<TData>) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, Arc::clone(data));
        let bin_data = bincode::serialize(data.as_ref())?;
        self.db
            .put(DbKey::new(self.prefix, key), bin_data)?;
        Ok(())
    }

    pub fn write_batch(&self, batch: &mut WriteBatch, key: TKey, data: &Arc<TData>) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, Arc::clone(data));
        let bin_data = bincode::serialize(data.as_ref())?;
        batch.put(DbKey::new(self.prefix, key), bin_data);
        Ok(())
    }
}

/// A concurrent DB store with typed caching for `Copy` types.
/// TODO: try and generalize under `CachedDbAccess`
#[derive(Clone)]
pub struct CachedDbAccessForCopy<TKey, TData>
where
    TKey: std::hash::Hash + Eq + Send + Sync + 'static,
    TData: Clone + Copy + Send + Sync + 'static,
{
    db: Arc<DB>,
    // The moka cache type supports shallow cloning and manages
    // ref counting internally, so no need for Arc
    cache: Cache<TKey, TData>,

    // DB bucket/path (TODO: eventually this must become dynamic in
    // order to support `active/inactive` consensus instances)
    prefix: &'static [u8],
}

impl<TKey, TData> CachedDbAccessForCopy<TKey, TData>
where
    TKey: std::hash::Hash + Eq + Send + Sync + 'static,
    TData: Clone + Copy + Send + Sync + 'static,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: &'static [u8]) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&key)
            || self
                .db
                .get_pinned(DbKey::new(self.prefix, key))?
                .is_some())
    }

    pub fn read(&self, key: TKey) -> Result<TData, StoreError>
    where
        TKey: Copy + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else if let Some(slice) = self.db.get_pinned(DbKey::new(self.prefix, key))? {
            let data: TData = bincode::deserialize(&slice)?;
            self.cache.insert(key, data);
            Ok(data)
        } else {
            Err(StoreError::KeyNotFound(key.to_string()))
        }
    }

    pub fn write(&self, key: TKey, data: TData) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, data);
        let bin_data = bincode::serialize(&data)?;
        self.db
            .put(DbKey::new(self.prefix, key), bin_data)?;
        Ok(())
    }

    pub fn write_batch(&self, batch: &mut WriteBatch, key: TKey, data: TData) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, data);
        let bin_data = bincode::serialize(&data)?;
        batch.put(DbKey::new(self.prefix, key), bin_data);
        Ok(())
    }
}

/// A cached DB item with concurrency support
#[derive(Clone)]
pub struct CachedDbItem<T> {
    db: Arc<DB>,
    key: &'static [u8],
    cached_item: Arc<RwLock<Option<T>>>,
}

impl<T> CachedDbItem<T> {
    pub fn new(db: Arc<DB>, key: &'static [u8]) -> Self {
        assert!(String::from_utf8(Vec::from(key)).is_ok());
        Self { db, key, cached_item: Arc::new(RwLock::new(None)) }
    }

    pub fn read(&self) -> Result<T, StoreError>
    where
        T: Copy + DeserializeOwned,
    {
        if let Some(root) = *self.cached_item.read().unwrap() {
            Ok(root)
        } else if let Some(slice) = self.db.get_pinned(self.key)? {
            let item: T = bincode::deserialize(&slice)?;
            *self.cached_item.write().unwrap() = Some(item);
            Ok(item)
        } else {
            Err(StoreError::KeyNotFound(String::from_utf8(Vec::from(self.key)).unwrap()))
        }
    }

    pub fn write(&mut self, item: &T) -> Result<(), StoreError>
    where
        T: Copy + Serialize, // Copy can be relaxed to Clone if needed by new usages
    {
        *self.cached_item.write().unwrap() = Some(*item);
        let bin_data = bincode::serialize(&item)?;
        self.db.put(self.key, bin_data)?;
        Ok(())
    }

    pub fn write_batch(&mut self, batch: &mut WriteBatch, item: &T) -> Result<(), StoreError>
    where
        T: Copy + Serialize,
    {
        *self.cached_item.write().unwrap() = Some(*item);
        let bin_data = bincode::serialize(&item)?;
        batch.put(self.key, bin_data);
        Ok(())
    }
}
