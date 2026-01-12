use crate::{
    cache::CachePolicy,
    db::DB,
    errors::{StoreError, StoreResult},
};

use super::prelude::{Cache, DbKey, DbWriter};
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::{DBIteratorWithThreadMode, DBWithThreadMode, Direction, IterateBounds, IteratorMode, MultiThreaded, ReadOptions};
use self_cell::self_cell;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;
use std::{collections::hash_map::RandomState, hash::BuildHasher, sync::Arc};

/// A concurrent DB store access with typed caching.
#[derive(Clone)]
pub struct CachedDbAccess<TKey, TData, S = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, TData, S>,

    // DB bucket/path
    prefix: Vec<u8>,
}

type KeyDataResult<TKey, TData> = StoreResult<(TKey, TData)>;

impl<TKey, TData, S> CachedDbAccess<TKey, TData, S>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync + MemSizeEstimator,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>) -> Self {
        Self { db, cache: Cache::new(cache_policy), prefix }
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

    pub fn has_with_fallback(&self, fallback_prefix: &[u8], key: TKey) -> Result<bool, StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        if self.cache.contains_key(&key) {
            Ok(true)
        } else {
            let db_key = DbKey::new(&self.prefix, key.clone());
            if self.db.get_pinned(&db_key)?.is_some() {
                Ok(true)
            } else {
                let db_key = DbKey::new(fallback_prefix, key.clone());
                Ok(self.db.get_pinned(&db_key)?.is_some())
            }
        }
    }

    pub fn read_with_fallback<TFallbackDeser>(&self, fallback_prefix: &[u8], key: TKey) -> Result<TData, StoreError>
    where
        TKey: Clone + AsRef<[u8]> + ToString,
        TData: DeserializeOwned,
        TFallbackDeser: DeserializeOwned + Into<TData>,
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
                let db_key = DbKey::new(fallback_prefix, key.clone());
                if let Some(slice) = self.db.get_pinned(&db_key)? {
                    let data: TFallbackDeser = bincode::deserialize(&slice)?;
                    let data: TData = data.into();
                    self.cache.insert(key, data.clone());
                    Ok(data)
                } else {
                    Err(StoreError::KeyNotFound(db_key))
                }
            }
        }
    }

    pub fn iterator(&self) -> impl Iterator<Item = KeyDataResult<TKey, TData>> + '_
    where
        TKey: Clone + AsRef<[u8]> + for<'a> TryFrom<&'a [u8]>,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        let prefix_key = DbKey::prefix_only(&self.prefix);
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(prefix_key.as_ref()));
        self.db.iterator_opt(IteratorMode::From(prefix_key.as_ref(), Direction::Forward), read_opts).map(move |iter_result| {
            match iter_result {
                Ok((key_bytes, data_bytes)) => match TKey::try_from(&key_bytes[self.prefix.len()..]) {
                    Ok(key) => match bincode::deserialize(&data_bytes) {
                        Ok(data) => Ok((key, data)),
                        Err(e) => Err(StoreError::DeserializationError(e)),
                    },
                    Err(_) => Err(StoreError::ConversionError(format!("Failed to deserialize key: {:?}", key_bytes))),
                },
                Err(e) => Err(StoreError::DbError(e)),
            }
        })
    }

    /// A dynamic iterator that can iterate through a specific prefix / bucket, or from a certain start point.
    pub fn seek_iterator(
        &self,
        bucket: Option<&[u8]>,   // iter self.prefix if None, else append bytes to self.prefix.
        seek_from: Option<TKey>, // iter whole range if None
        limit: usize,            // amount to take.
        skip_first: bool,        // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = KeyDataResult<TKey, TData>> + '_
    where
        TKey: Clone + AsRef<[u8]> + for<'a> TryFrom<&'a [u8]>,
        TData: DeserializeOwned,
    {
        let db_key = bucket.map_or_else(
            move || DbKey::prefix_only(&self.prefix),
            move |bucket| {
                let mut key = DbKey::prefix_only(&self.prefix);
                key.add_bucket(bucket);
                key
            },
        );

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
            Ok((ref key_bytes, ref data_bytes)) => match bincode::deserialize::<TData>(data_bytes.as_ref()) {
                Ok(data) => Ok((
                    TKey::try_from(&key_bytes[self.prefix.len()..])
                        .map_err(|_e| StoreError::ConversionError(format!("Failed to deserialize key: {:?}", &key_bytes)))?,
                    data,
                )),

                Err(e) => Err(StoreError::DeserializationError(e)),
            },
            Err(e) => Err(StoreError::DbError(e)),
        })
    }

    pub fn iterator_owned(&self) -> impl Iterator<Item = KeyDataResult<TKey, TData>> + 'static
    where
        TKey: Clone + AsRef<[u8]> + for<'a> TryFrom<&'a [u8]> + 'static,
        TData: DeserializeOwned + 'static,
    {
        let db = self.db.clone();
        let prefix_key = DbKey::prefix_only(&self.prefix);
        let prefix_len = self.prefix.len();
        OwnedIter {
            db_iter_cell: DbIterCell::new(db, |db| {
                // TODO: Configure read options:
                // this iterator is used to sync the utxoindex utxo set from the consensus db.
                // Perhaps tailor the read options for this specific use-case.
                // Although a few have been tried and no significant difference was observed.
                // The option combinations were:
                // async io - true false
                // readahead - 4mb, 256 kb, 32kb
                // Default options.
                // As such staying with default options for now.
                let mut read_ops = ReadOptions::default();
                read_ops.set_iterate_range(rocksdb::PrefixRange(prefix_key.as_ref()));
                db.iterator_opt(IteratorMode::From(prefix_key.as_ref(), Direction::Forward), read_ops)
            }),
            prefix_len,
            data: Default::default(),
        }
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

    /// Deletes all entries in the store using the underlying rocksdb `delete_range` operation
    pub fn delete_all(&self, mut writer: impl DbWriter) -> Result<(), StoreError>
    where
        TKey: Clone + AsRef<[u8]>,
    {
        self.cache.remove_all();
        let db_key = DbKey::prefix_only(&self.prefix);
        let (from, to) = rocksdb::PrefixRange(db_key.as_ref()).into_bounds();
        writer.delete_range(from.unwrap(), to.unwrap())?;
        Ok(())
    }

    pub fn prefix(&self) -> &[u8] {
        &self.prefix
    }
}

type DbIterator<'a> = DBIteratorWithThreadMode<'a, DBWithThreadMode<MultiThreaded>>;

self_cell!(
    struct DbIterCell {
        owner: Arc<DB>,

        #[covariant]
        dependent: DbIterator,
    }
);

pub struct OwnedIter<TKey, TData> {
    db_iter_cell: DbIterCell,
    prefix_len: usize,
    data: PhantomData<fn() -> (TKey, TData)>,
}

impl<TKey, TData> Iterator for OwnedIter<TKey, TData>
where
    TKey: for<'a> TryFrom<&'a [u8]>,
    TData: DeserializeOwned,
{
    type Item = KeyDataResult<TKey, TData>;

    fn next(&mut self) -> Option<Self::Item> {
        self.db_iter_cell.with_dependent_mut(|_, db_iterator| match db_iterator.next() {
            Some(item) => match item {
                Ok((key_bytes, data_bytes)) => match TKey::try_from(&key_bytes[self.prefix_len..]) {
                    Ok(key) => match bincode::deserialize::<TData>(data_bytes.as_ref()) {
                        Ok(data) => Some(Ok((key, data))),
                        Err(e) => Some(Err(StoreError::DeserializationError(e))),
                    },
                    Err(_) => Some(Err(StoreError::ConversionError(format!("Failed to deserialize key: {:?}", key_bytes)))),
                },
                Err(err) => Some(Err(err.into())),
            },
            None => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, DirectDbWriter},
    };
    use kaspa_hashes::Hash;
    use rocksdb::WriteBatch;

    #[test]
    fn test_delete_all() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(2), vec![1, 2]);

        access.write_many(DirectDbWriter::new(&db), &mut (0..16).map(|i| (i.into(), 2))).unwrap();
        assert_eq!(16, access.iterator().count());
        access.delete_all(DirectDbWriter::new(&db)).unwrap();
        assert_eq!(0, access.iterator().count());

        access.write_many(DirectDbWriter::new(&db), &mut (0..16).map(|i| (i.into(), 2))).unwrap();
        assert_eq!(16, access.iterator().count());
        let mut batch = WriteBatch::default();
        access.delete_all(BatchDbWriter::new(&mut batch)).unwrap();
        assert_eq!(16, access.iterator().count());
        db.write(batch).unwrap();
        assert_eq!(0, access.iterator().count());
    }

    #[test]
    fn test_read_with_fallback() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let primary_prefix = vec![1];
        let fallback_prefix = vec![2];
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), primary_prefix);
        let fallback_access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), fallback_prefix.clone());

        let key: Hash = 1.into();
        let value = 100;

        // Write to fallback
        fallback_access.write(DirectDbWriter::new(&db), key, value).unwrap();

        // Read with fallback, should succeed
        let result = access.read_with_fallback::<u64>(&fallback_prefix, key).unwrap();
        assert_eq!(result, value);

        // Key should now be in the primary cache
        assert_eq!(access.read_from_cache(key).unwrap(), value);
    }

    #[test]
    fn test_has_with_fallback() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let primary_prefix = vec![1];
        let fallback_prefix = vec![2];
        let access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), primary_prefix);
        let fallback_access = CachedDbAccess::<Hash, u64>::new(db.clone(), CachePolicy::Count(10), fallback_prefix.clone());

        let key_in_fallback: Hash = 1.into();
        let key_not_found: Hash = 2.into();

        // Write to fallback
        fallback_access.write(DirectDbWriter::new(&db), key_in_fallback, 100).unwrap();

        // Check for key in fallback, should exist
        assert!(access.has_with_fallback(&fallback_prefix, key_in_fallback).unwrap());

        // Check for key that doesn't exist, should not be found
        assert!(!access.has_with_fallback(&fallback_prefix, key_not_found).unwrap());
    }
}
