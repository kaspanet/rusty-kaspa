use crate::{cache::CachePolicy, db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::{Direction, IterateBounds, IteratorMode, ReadOptions};
use serde::{Serialize, de::DeserializeOwned};
use std::{collections::hash_map::RandomState, error::Error, hash::BuildHasher, ops::RangeInclusive, sync::Arc};

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

pub type KeyDataResult<TData> = Result<(Box<[u8]>, TData), Box<dyn Error>>;

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

    pub fn iterator(&self) -> impl Iterator<Item = KeyDataResult<TData>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        let prefix_key = DbKey::prefix_only(&self.prefix);
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(prefix_key.as_ref()));
        self.db.iterator_opt(IteratorMode::From(prefix_key.as_ref(), Direction::Forward), read_opts).map(move |iter_result| {
            match iter_result {
                Ok((key, data_bytes)) => match bincode::deserialize(&data_bytes) {
                    Ok(data) => Ok((key[prefix_key.prefix_len()..].into(), data)),
                    Err(e) => Err(e.into()),
                },
                Err(e) => Err(e.into()),
            }
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

    /// A dynamic iterator that can iterate through a specific prefix / bucket, or from a certain start point.
    pub fn seek_iterator(
        &self,
        bucket: Option<&[u8]>,   // iter self.prefix if None, else append bytes to self.prefix.
        seek_from: Option<TKey>, // iter whole range if None
        seek_to: Option<TKey>,   // iter until the seek_to key (exclusive) if Some, else iter whole range.
        limit: usize,            // amount to take.
        skip_first: bool,        // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = KeyDataResult<TData>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
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

        if let Some(seek_from) = &seek_from {
            read_opts.set_iterate_lower_bound(DbKey::new(db_key.as_ref(), seek_from).as_ref());
        };
        if let Some(seek_to) = &seek_to {
            read_opts.set_iterate_upper_bound(DbKey::new(db_key.as_ref(), seek_to).as_ref());
        };

        let mut db_iterator = self.db.iterator_opt(IteratorMode::Start, read_opts);

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

    pub fn multi_range_seek_iterator<'a>(
        &'a self,
        mut seek_ranges: impl Iterator<Item = RangeInclusive<TKey>> + 'a,
        seek_min: Option<TKey>,
        seek_max: Option<TKey>,
        limit: usize,
    ) -> impl Iterator<Item = KeyDataResult<TData>> + 'a
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_range(rocksdb::PrefixRange(self.prefix.as_slice()));

        if let Some(seek_min) = &seek_min {
            read_opts.set_iterate_lower_bound(DbKey::new(&self.prefix, seek_min).as_ref());
        }
        if let Some(seek_max) = &seek_max {
            read_opts.set_iterate_upper_bound(DbKey::new(&self.prefix, seek_max).as_ref());
        }

        let mut db_iterator = self.db.raw_iterator_opt(read_opts);
        let mut count = 0usize;
        // Use Option to unify the "no ranges" and "ranges exhausted" cases in a single return path.
        let mut current_seek_range = seek_ranges.next().inspect(|range| {
            db_iterator.seek(DbKey::new(&self.prefix, range.start().clone()).as_ref());
        });

        std::iter::from_fn(move || {
            while db_iterator.valid() && count < limit && current_seek_range.is_some() {
                let key_bytes: Box<[u8]> = db_iterator.key().unwrap()[self.prefix.len()..].into();
                if key_bytes.as_ref() > current_seek_range.as_ref().unwrap().end().as_ref() {
                    current_seek_range = seek_ranges.next().inspect(|next_range| {
                        db_iterator.seek(DbKey::new(&self.prefix, next_range.start().clone()).as_ref());
                    });
                    continue;
                }
                let value_bytes = db_iterator.value().unwrap();
                let res = match bincode::deserialize::<TData>(value_bytes) {
                    Ok(value) => Some(Ok((key_bytes, value))),
                    Err(err) => Some(Err(err.into())),
                };
                db_iterator.next();
                count += 1;
                return res;
            }
            None
        })
    }

    #[inline(always)]
    pub fn prefix(&self) -> &[u8] {
        &self.prefix
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

    fn range_key(n: u64) -> Vec<u8> {
        n.to_be_bytes().to_vec()
    }

    #[test]
    fn test_multi_range_seek_iterator_empty_ranges() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Vec<u8>, u64>::new(db.clone(), CachePolicy::Count(10), vec![5]);
        access.write_many(DirectDbWriter::new(&db), &mut (0u64..10).map(|i| (range_key(i), i))).unwrap();

        let results: Vec<_> = access.multi_range_seek_iterator(std::iter::empty(), None, None, 100).collect();
        assert!(results.is_empty());
    }

    #[test]
    fn test_multi_range_seek_iterator_single_range() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Vec<u8>, u64>::new(db.clone(), CachePolicy::Count(10), vec![5]);
        access.write_many(DirectDbWriter::new(&db), &mut (0u64..10).map(|i| (range_key(i), i))).unwrap();

        let ranges = [range_key(2)..=range_key(5)];
        let results: Vec<_> = access.multi_range_seek_iterator(ranges.into_iter(), None, None, 100).map(|r| r.unwrap()).collect();

        let values: Vec<u64> = results.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, vec![2, 3, 4, 5]);
    }

    #[test]
    fn test_multi_range_seek_iterator_multiple_ranges() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Vec<u8>, u64>::new(db.clone(), CachePolicy::Count(10), vec![5]);
        access.write_many(DirectDbWriter::new(&db), &mut (0u64..20).map(|i| (range_key(i), i))).unwrap();

        let ranges = [range_key(1)..=range_key(3), range_key(7)..=range_key(9)];
        let results: Vec<_> = access.multi_range_seek_iterator(ranges.into_iter(), None, None, 100).map(|r| r.unwrap()).collect();

        let values: Vec<u64> = results.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, vec![1, 2, 3, 7, 8, 9]);
    }

    #[test]
    fn test_multi_range_iterator_limit() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Vec<u8>, u64>::new(db.clone(), CachePolicy::Count(10), vec![5]);
        access.write_many(DirectDbWriter::new(&db), &mut (0u64..20).map(|i| (range_key(i), i))).unwrap();

        let ranges = [range_key(0)..=range_key(19)];
        let results: Vec<_> = access.multi_range_seek_iterator(ranges.into_iter(), None, None, 5).map(|r| r.unwrap()).collect();

        assert_eq!(results.len(), 5);
        assert_eq!(results[0].1, 0);
        assert_eq!(results[4].1, 4);
    }

    #[test]
    fn test_multi_range_iterator_sparse_data() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let access = CachedDbAccess::<Vec<u8>, u64>::new(db.clone(), CachePolicy::Count(10), vec![5]);
        // Only store even values: 0, 2, 4, 6, 8
        access.write_many(DirectDbWriter::new(&db), &mut (0u64..5).map(|i| (range_key(i * 2), i * 2))).unwrap();

        // Range [1, 7] should return 2, 4, 6
        let ranges = [range_key(1)..=range_key(7)];
        let results: Vec<_> = access.multi_range_seek_iterator(ranges.into_iter(), None, None, 100).map(|r| r.unwrap()).collect();

        let values: Vec<u64> = results.iter().map(|(_, v)| *v).collect();
        assert_eq!(values, vec![2, 4, 6]);
    }
}
