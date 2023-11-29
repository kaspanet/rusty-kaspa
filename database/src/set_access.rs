use crate::{db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use parking_lot::{RwLock, RwLockReadGuard};
use rocksdb::{IteratorMode, ReadOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{hash_map::RandomState, HashSet},
    error::Error,
    hash::BuildHasher,
    sync::Arc,
};

/// A concurrent DB store access with typed caching.
#[derive(Clone)]
pub struct CachedDbSetAccess<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, Arc<RwLock<HashSet<TData>>>, S>,

    // DB bucket/path
    prefix: Vec<u8>,
}

pub struct ReadLock<T>(Arc<RwLock<T>>);

impl<T> ReadLock<T> {
    pub fn new(rwlock: Arc<RwLock<T>>) -> Self {
        Self(rwlock)
    }

    pub fn read(&self) -> RwLockReadGuard<T> {
        self.0.read()
    }
}

impl<TKey, TData, S> CachedDbSetAccess<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync + AsRef<[u8]>,
    TData: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: Vec<u8>) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<ReadLock<HashSet<TData>>> {
        let set = self.cache.get(&key)?.clone();
        Some(ReadLock::new(set))
    }

    fn get_locked_data(&self, key: TKey) -> Result<Arc<RwLock<HashSet<TData>>>, StoreError> {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let data: HashSet<TData> = self.bucket_iterator(key.clone()).map(|x| x.unwrap()).collect();
            let data = Arc::new(RwLock::new(data));
            self.cache.insert(key, data.clone());
            Ok(data)
        }
    }

    pub fn read(&self, key: TKey) -> Result<ReadLock<HashSet<TData>>, StoreError> {
        Ok(ReadLock::new(self.get_locked_data(key)?))
    }

    pub fn write(&self, writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        let locked_data = self.get_locked_data(key.clone())?;
        let mut write_guard = locked_data.write();
        write_guard.insert(data.clone());
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
        let locked_data = self.get_locked_data(key.clone())?;
        let mut write_guard = locked_data.write();
        for data in write_guard.iter() {
            writer.delete(self.get_db_key(&key, data)?)?;
        }
        *write_guard = Default::default();
        Ok(())
    }

    pub fn delete(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        let locked_data = self.get_locked_data(key.clone())?;
        let mut write_guard = locked_data.write();
        write_guard.remove(&data);
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

        db_iterator.take(limit).map(move |item: Result<(Box<[u8]>, Box<[u8]>), rocksdb::Error>| match item {
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
