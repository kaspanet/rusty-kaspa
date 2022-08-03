use super::{errors::StoreError, DB};
use moka::sync::Cache;
use rocksdb::WriteBatch;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

/// A general purpose cached DB accessor
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
    // TODO: manage DB bucket/path
}

impl<TKey, TData> CachedDbAccess<TKey, TData>
where
    TKey: std::hash::Hash + Eq + Send + Sync + 'static,
    TData: Clone + Send + Sync + 'static,
{
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db, cache: Cache::new(cache_size) }
    }

    pub fn has(&self, hash: TKey) -> Result<bool, StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&hash) || self.db.get_pinned(hash)?.is_some())
    }

    pub fn read(&self, hash: TKey) -> Result<Arc<TData>, StoreError>
    where
        TKey: Copy + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&hash) {
            Ok(data)
        } else if let Some(slice) = self.db.get_pinned(hash)? {
            let data: Arc<TData> = Arc::new(bincode::deserialize(&slice)?);
            self.cache.insert(hash, Arc::clone(&data));
            Ok(data)
        } else {
            Err(StoreError::KeyNotFound(hash.to_string()))
        }
    }

    pub fn write(&self, hash: TKey, data: &Arc<TData>) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(hash, Arc::clone(data));
        let bin_data = bincode::serialize(data.as_ref())?;
        self.db.put(hash, bin_data)?;
        Ok(())
    }

    pub fn write_batch(&self, batch: &mut WriteBatch, hash: TKey, data: &Arc<TData>) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(hash, Arc::clone(data));
        let bin_data = bincode::serialize(data.as_ref())?;
        batch.put(hash, bin_data);
        Ok(())
    }
}
