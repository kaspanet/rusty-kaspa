use crate::{cache::CachePolicy, db::DB, errors::StoreError};

use super::prelude::{Cache, DbKey, DbWriter};
use parking_lot::{RwLock, RwLockReadGuard};
use rocksdb::{IterateBounds, IteratorMode, ReadOptions};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{hash_map::RandomState, HashSet},
    fmt::Debug,
    hash::BuildHasher,
    marker::PhantomData,
    sync::Arc,
};

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

/// A concurrent DB store for **set** access with typed caching.
#[derive(Clone)]
pub struct CachedDbSetAccess<TKey, TData, S = RandomState, W = RandomState>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
    W: Send + Sync,
{
    // The inner uncached DB access
    inner: DbSetAccess<TKey, TData>,

    // Cache
    cache: Cache<TKey, Arc<RwLock<HashSet<TData, W>>>, S>,
}

impl<TKey, TData, S, W> CachedDbSetAccess<TKey, TData, S, W>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync + AsRef<[u8]>,
    TData: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
    S: BuildHasher + Default,
    W: BuildHasher + Default + Send + Sync,
{
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, prefix: Vec<u8>) -> Self {
        Self { inner: DbSetAccess::new(db, prefix), cache: Cache::new(cache_policy) }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<ReadLock<HashSet<TData, W>>> {
        self.cache.get(&key).map(ReadLock::new)
    }

    /// Returns the set entry wrapped with a read-write lock. If the entry is not cached then it is read from the DB and cached.
    fn read_locked_entry(&self, key: TKey) -> Result<Arc<RwLock<HashSet<TData, W>>>, StoreError> {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let data: HashSet<TData, _> = self.inner.bucket_iterator(key.clone()).collect::<Result<_, _>>()?;
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
        self.cache.update_if_entry_exists(key.clone(), |locked_entry| {
            locked_entry.write().insert(data.clone());
        });
        self.inner.write(writer, key, data)
    }

    pub fn delete_bucket(&self, writer: impl DbWriter, key: TKey) -> Result<(), StoreError> {
        self.cache.remove(&key);
        self.inner.delete_bucket(writer, key)
    }

    pub fn delete(&self, writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        // We remove the item from cache only if the full set entry already exists in the cache
        self.cache.update_if_entry_exists(key.clone(), |locked_entry| {
            locked_entry.write().remove(&data);
        });
        self.inner.delete(writer, key, data)?;
        Ok(())
    }

    pub fn prefix(&self) -> &[u8] {
        self.inner.prefix()
    }
}

/// A concurrent DB store for typed **set** access *without* caching.
#[derive(Clone)]
pub struct DbSetAccess<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
{
    db: Arc<DB>,

    // DB bucket/path
    prefix: Vec<u8>,

    _phantom: PhantomData<(TKey, TData)>,
}

impl<TKey, TData> DbSetAccess<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync + AsRef<[u8]>,
    TData: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
{
    pub fn new(db: Arc<DB>, prefix: Vec<u8>) -> Self {
        Self { db, prefix, _phantom: Default::default() }
    }

    pub fn write(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        writer.put(self.get_db_key(&key, &data)?, [])?;
        Ok(())
    }

    fn get_db_key(&self, key: &TKey, data: &TData) -> Result<DbKey, StoreError> {
        let bin_data = bincode::serialize(&data)?;
        Ok(DbKey::new_with_bucket(&self.prefix, key, bin_data))
    }

    pub fn delete_bucket(&self, mut writer: impl DbWriter, key: TKey) -> Result<(), StoreError> {
        let db_key = DbKey::new_with_bucket(&self.prefix, &key, []);
        let (from, to) = rocksdb::PrefixRange(db_key.as_ref()).into_bounds();
        writer.delete_range(from.unwrap(), to.unwrap())?;
        Ok(())
    }

    pub fn delete(&self, mut writer: impl DbWriter, key: TKey, data: TData) -> Result<(), StoreError> {
        writer.delete(self.get_db_key(&key, &data)?)?;
        Ok(())
    }

    fn seek_iterator(
        &self,
        key: TKey,
        limit: usize,     // amount to take.
        skip_first: bool, // skips the first value, (useful in conjunction with the seek-key, as to not re-retrieve).
    ) -> impl Iterator<Item = Result<Box<[u8]>, StoreError>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        let db_key = DbKey::new_with_bucket(&self.prefix, &key, []);
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

    pub fn bucket_iterator(&self, key: TKey) -> impl Iterator<Item = Result<TData, StoreError>> + '_
    where
        TKey: Clone + AsRef<[u8]>,
        TData: DeserializeOwned,
    {
        self.seek_iterator(key, usize::MAX, false).map(|res| match res {
            Ok(data) => Ok(bincode::deserialize(&data)?),
            Err(err) => Err(err),
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
    fn test_delete_bucket() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10)).expect("Failed to create temp db");
        let access = DbSetAccess::<Hash, u64>::new(db.clone(), vec![1, 2]);

        for i in 0..16 {
            for j in 0..2 {
                access.write(DirectDbWriter::new(&db), i.into(), i + j).unwrap();
            }
        }
        for i in 0..16 {
            assert_eq!(2, access.bucket_iterator(i.into()).count());
        }
        access.delete_bucket(DirectDbWriter::new(&db), 3.into()).unwrap();
        assert_eq!(0, access.bucket_iterator(3.into()).count());

        let mut batch = WriteBatch::default();
        access.delete_bucket(BatchDbWriter::new(&mut batch), 6.into()).unwrap();
        db.write(batch).unwrap();
        assert_eq!(0, access.bucket_iterator(6.into()).count());
    }
}
