use crate::{db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use parking_lot::{RwLock, RwLockReadGuard};
use rocksdb::{IteratorMode, ReadOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{hash_map::RandomState, HashSet},
    error::Error,
    fmt::Debug,
    hash::BuildHasher,
    sync::Arc,
};

/// A concurrent DB store for **set** access with typed caching.
#[derive(Clone)]
pub struct CachedDbSetAccess<TKey, TData, S = RandomState, W = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
    W: Send + Sync,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, Arc<RwLock<HashSet<TData, W>>>, S>,

    // DB bucket/path
    prefix: Vec<u8>,
}

/// A read-only lock. Essentially a wrapper to [`parking_lot::RwLock`] which allows only reading.
#[derive(Default, Debug)]
pub struct ReadLock<T>(Arc<RwLock<T>>);

impl<T> ReadLock<T> {
    pub fn new(rwlock: Arc<RwLock<T>>) -> Self {
        Self(rwlock)
    }

    pub fn read(&self) -> RwLockReadGuard<T> {
        self.0.read()
    }
}

impl<T> From<T> for ReadLock<T> {
    fn from(value: T) -> Self {
        Self::new(Arc::new(RwLock::new(value)))
    }
}

impl<TKey, TData, S, W> CachedDbSetAccess<TKey, TData, S, W>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync + AsRef<[u8]>,
    TData: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
    S: BuildHasher + Default,
    W: BuildHasher + Default + Send + Sync,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: Vec<u8>) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<ReadLock<HashSet<TData, W>>> {
        self.cache.get(&key).map(ReadLock::new)
    }

    /// Returns the set entry wrapped with a read-write lock. If the entry is not cached then it is read from the DB and cached.
    fn read_locked_entry(&self, key: TKey) -> Result<Arc<RwLock<HashSet<TData, W>>>, StoreError> {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let data: HashSet<TData, _> = self.bucket_iterator(key.clone()).map(|x| x.unwrap()).collect();
            let data = Arc::new(RwLock::new(data));
            self.cache.insert(key, data.clone());
            Ok(data)
        }
    }

    pub fn read(&self, key: TKey) -> Result<ReadLock<HashSet<TData, W>>, StoreError> {
        Ok(ReadLock::new(self.read_locked_entry(key)?))
    }

    pub fn write(&self, writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        // We cache the new item only if the set entry already exists in the cache
        if let Some(locked_entry) = self.cache.get(&key) {
            locked_entry.write().insert(data.clone());
        }
        self.write_to_db(writer, key, &data)
    }

    fn write_to_db(&self, mut writer: impl DbWriter, key: TKey, data: &TData) -> Result<(), StoreError> {
        writer.put(self.get_db_key(&key, data)?, [])?;
        Ok(())
    }

    fn get_db_key(&self, key: &TKey, data: &TData) -> Result<DbKey, StoreError> {
        let bin_data = bincode::serialize(&data)?;
        Ok(DbKey::new_with_bucket(&self.prefix, key, bin_data))
    }

    pub fn delete_bucket(&self, mut writer: impl DbWriter, key: TKey) -> Result<(), StoreError> {
        let locked_entry = self.read_locked_entry(key.clone())?;
        // TODO: check if DB supports delete by prefix
        for data in locked_entry.read().iter() {
            writer.delete(self.get_db_key(&key, data)?)?;
        }
        self.cache.remove(&key);
        Ok(())
    }

    pub fn delete(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        // We remove the item from cache only if the full set entry already exists in the cache
        if let Some(locked_entry) = self.cache.get(&key) {
            locked_entry.write().remove(&data);
        }
        writer.delete(self.get_db_key(&key, &data)?)?;
        Ok(())
    }

    fn seek_iterator(
        &self,
        key: TKey,
        limit: usize,     // amount to take.
        skip_first: bool, // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = Result<Box<[u8]>, Box<dyn Error>>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        let db_key = {
            let mut db_key = DbKey::prefix_only(&self.prefix);
            db_key.add_bucket(&key);
            db_key
        };

        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(db_key.as_ref()));

        let mut db_iterator = self.db.iterator_opt(IteratorMode::Start, read_opts);

        if skip_first {
            db_iterator.next();
        }

        db_iterator.take(limit).map(move |item| match item {
            Ok((key_bytes, _)) => Ok(key_bytes[db_key.prefix_len()..].into()),
            Err(err) => Err(err.into()),
        })
    }

    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }

    fn bucket_iterator(&self, key: TKey) -> impl Iterator<Item = Result<TData, Box<dyn Error>>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        self.seek_iterator(key, usize::MAX, false).map(|res| {
            let data = res.unwrap();
            Ok(bincode::deserialize(&data)?)
        })
    }
}
