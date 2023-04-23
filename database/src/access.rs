use crate::{db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use itertools::Itertools;
use rocksdb::{Direction, IteratorMode, ReadOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::hash_map::RandomState, error::Error, hash::BuildHasher, sync::Arc};

/// A concurrent DB store access with typed caching.
#[derive(Clone)]
pub struct CachedDbAccess<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, TData, S>,

    // DB bucket/path
    prefix: Vec<u8>,
}

impl<TKey, TData, S> CachedDbAccess<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: Vec<u8>) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<TData>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        self.cache.get(&key)
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&key) || self.db.get_pinned(DbKey::new(&self.prefix, key))?.is_some())
    }

    pub fn read(&self, key: TKey) -> Result<TData, StoreError>
    where
        TKey: Clone + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let db_key = DbKey::new(&self.prefix, key.clone());
            if let Some(slice) = self.db.get_pinned(&db_key)? {
                let data: TData = bincode::deserialize(&slice)?;
                self.cache.insert(key, data.clone());
                Ok(data)
            } else {
                Err(StoreError::KeyNotFound(db_key))
            }
        }
    }

    pub fn iterator(&self) -> impl Iterator<Item = Result<(Box<[u8]>, TData), Box<dyn Error>>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        let db_key = DbKey::prefix_only(&self.prefix);
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(db_key.as_ref()));
        self.db.iterator_opt(IteratorMode::From(db_key.as_ref(), Direction::Forward), read_opts).map(|iter_result| match iter_result {
            Ok((key, data_bytes)) => match bincode::deserialize(&data_bytes) {
                Ok(data) => Ok((key[self.prefix.len() + 1..].into(), data)),
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e.into()),
        })
    }

    pub fn write(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        let bin_data = bincode::serialize(&data)?;
        self.cache.insert(key.clone(), data);
        writer.put(DbKey::new(&self.prefix, key), bin_data)?;
        Ok(())
    }

    pub fn write_many(
        &self,
        mut writer: impl DbWriter,
        iter: &mut (impl Iterator<Item = (TKey, TData)> + Clone),
    ) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        let iter_clone = iter.clone();
        self.cache.insert_many(iter);
        for (key, data) in iter_clone {
            let bin_data = bincode::serialize(&data)?;
            writer.put(DbKey::new(&self.prefix, key.clone()), bin_data)?;
        }
        Ok(())
    }

    /// Write directly from an iterator and do not cache any data. NOTE: this action also clears the cache
    pub fn write_many_without_cache(
        &self,
        mut writer: impl DbWriter,
        iter: &mut impl Iterator<Item = (TKey, TData)>,
    ) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
        TData: Serialize,
    {
        for (key, data) in iter {
            let bin_data = bincode::serialize(&data)?;
            writer.put(DbKey::new(&self.prefix, key), bin_data)?;
        }
        // We must clear the cache in order to avoid invalidated entries
        self.cache.remove_all();
        Ok(())
    }

    pub fn delete(&self, mut writer: impl DbWriter, key: TKey) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.cache.remove(&key);
        writer.delete(DbKey::new(&self.prefix, key))?;
        Ok(())
    }

    pub fn delete_many(&self, mut writer: impl DbWriter, key_iter: &mut (impl Iterator<Item = TKey> + Clone)) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        let key_iter_clone = key_iter.clone();
        self.cache.remove_many(key_iter);
        for key in key_iter_clone {
            writer.delete(DbKey::new(&self.prefix, key.clone()))?;
        }
        Ok(())
    }

    pub fn delete_all(&self, mut writer: impl DbWriter) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.cache.remove_all();
        //TODO: Consider using column families to make it faster
        let db_key = DbKey::prefix_only(&self.prefix);
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(db_key.as_ref()));
        let keys = self
            .db
            .iterator_opt(IteratorMode::From(db_key.as_ref(), Direction::Forward), read_opts)
            .map(|iter_result| match iter_result {
                Ok((key, _)) => Ok::<_, rocksdb::Error>(key),
                Err(e) => Err(e),
            })
            .collect_vec();
        for key in keys {
            writer.delete(key.unwrap())?;
        }
        Ok(())
    }

    /// A dynamic iterator that can iterate through a specific prefix / bucket, or from a certain start point.
    //TODO: loop and chain iterators for multi-prefix / bucket iterator.
    pub fn seek_iterator(
        &self,
        bucket: Option<&[u8]>,   // iter self.prefix if None, else append bytes to self.prefix.
        seek_from: Option<TKey>, // iter whole range if None
        limit: usize,            // amount to take.
        skip_first: bool,        // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = Result<(Box<[u8]>, TData), Box<dyn Error>>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        let db_key = bucket.map_or(DbKey::prefix_only(&self.prefix), move |bucket| {
            let mut key = DbKey::prefix_only(&self.prefix);
            key.add_bucket(bucket);
            key
        });

        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(db_key.as_ref()));

        let mut db_iterator = match seek_from {
            Some(seek_key) => {
                self.db.iterator_opt(IteratorMode::From(DbKey::new(&self.prefix, seek_key).as_ref(), Direction::Forward), read_opts)
            }
            None => self.db.iterator_opt(IteratorMode::Start, read_opts),
        };

        if skip_first {
            db_iterator.next();
        }

        db_iterator.take(limit).map(move |item| match item {
            Ok((key_bytes, value_bytes)) => match bincode::deserialize::<TData>(value_bytes.as_ref()) {
                Ok(value) => Ok((key_bytes[db_key.prefix_len()..].into(), value)),
                Err(err) => Err(err.into()),
            },
            Err(err) => Err(err.into()),
        })
    }
}
