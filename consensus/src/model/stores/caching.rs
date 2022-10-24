use super::{errors::StoreError, DB};
use indexmap::IndexMap;
use parking_lot::RwLock;
use rand::Rng;
use rocksdb::WriteBatch;
use serde::{de::DeserializeOwned, Serialize};
use std::{fmt::Display, str, sync::Arc};

const SEP: u8 = b'/';

#[derive(Debug, Clone)]
pub struct DbKey {
    path: Vec<u8>,
}

impl DbKey {
    pub fn new<TKey: Copy + AsRef<[u8]>>(prefix: &[u8], key: TKey) -> Self {
        Self { path: prefix.iter().chain(std::iter::once(&SEP)).chain(key.as_ref().iter()).copied().collect() }
    }

    pub fn prefix_only(prefix: &[u8]) -> Self {
        Self::new(prefix, b"")
    }
}

impl AsRef<[u8]> for DbKey {
    fn as_ref(&self) -> &[u8] {
        &self.path
    }
}

impl Display for DbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let pos = self.path.len() - 1 - self.path.iter().rev().position(|c| *c == SEP).unwrap(); // Find the last position of `SEP`
        let (prefix, key) = (&self.path[..pos], &self.path[pos + 1..]);
        f.write_str(str::from_utf8(prefix).unwrap_or("{cannot display prefix}"))?;
        f.write_str("/")?;
        f.write_str(&faster_hex::hex_string(key))
    }
}

#[derive(Clone)]
pub struct Cache<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync> {
    map: Arc<RwLock<IndexMap<TKey, TData>>>, // We use IndexMap and not HashMap, because it makes it cheaper to remove a random element when the cache is full.
    size: usize,
}

impl<TKey: Clone + std::hash::Hash + Eq + Send + Sync, TData: Clone + Send + Sync> Cache<TKey, TData> {
    pub fn new(size: u64) -> Self {
        Self { map: Arc::new(RwLock::new(IndexMap::with_capacity(size as usize))), size: size as usize }
    }

    pub fn get(&self, key: &TKey) -> Option<TData> {
        self.map.read().get(key).cloned()
    }

    pub fn contains_key(&self, key: &TKey) -> bool {
        self.map.read().contains_key(key)
    }

    pub fn insert(&self, key: TKey, data: TData) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        if write_guard.len() == self.size {
            write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
        }
        write_guard.insert(key, data);
    }

    pub fn insert_many(&self, iter: &mut impl Iterator<Item = (TKey, TData)>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        for (key, data) in iter {
            if write_guard.len() == self.size {
                write_guard.swap_remove_index(rand::thread_rng().gen_range(0..self.size));
            }
            write_guard.insert(key, data);
        }
    }

    pub fn remove(&self, key: &TKey) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        write_guard.swap_remove(key);
    }

    pub fn remove_many(&self, key_iter: &mut impl Iterator<Item = TKey>) {
        if self.size == 0 {
            return;
        }
        let mut write_guard = self.map.write();
        for key in key_iter {
            write_guard.swap_remove(&key);
        }
    }
}

/// A concurrent DB store with typed caching.
#[derive(Clone)]
pub struct CachedDbAccess<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, Arc<TData>>,

    // DB bucket/path
    prefix: &'static [u8],
}

impl<TKey, TData> CachedDbAccess<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Send + Sync,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: &'static [u8]) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn read_from_cache(&self, key: TKey) -> Option<Arc<TData>>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        self.cache.get(&key)
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&key) || self.db.get_pinned(DbKey::new(self.prefix, key))?.is_some())
    }

    pub fn read(&self, key: TKey) -> Result<Arc<TData>, StoreError>
    where
        TKey: Copy + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let db_key = DbKey::new(self.prefix, key);
            if let Some(slice) = self.db.get_pinned(&db_key)? {
                let data: Arc<TData> = Arc::new(bincode::deserialize(&slice)?);
                self.cache.insert(key, Arc::clone(&data));
                Ok(data)
            } else {
                Err(StoreError::KeyNotFound(db_key))
            }
        }
    }

    pub fn write(&self, key: TKey, data: &Arc<TData>) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, Arc::clone(data));
        let bin_data = bincode::serialize(data.as_ref())?;
        self.db.put(DbKey::new(self.prefix, key), bin_data)?;
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

    pub fn write_many(
        &self,
        writer: &mut impl DbWriter,
        iter: &mut (impl Iterator<Item = (TKey, Arc<TData>)> + Clone),
    ) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        let iter_clone = iter.clone();
        self.cache.insert_many(iter);
        for (key, data) in iter_clone {
            let bin_data = bincode::serialize(data.as_ref())?;
            writer.put(DbKey::new(self.prefix, key), bin_data)?;
        }
        Ok(())
    }

    pub fn delete(&self, writer: &mut impl DbWriter, key: TKey) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        self.cache.remove(&key);
        writer.delete(DbKey::new(self.prefix, key))?;
        Ok(())
    }

    pub fn delete_many(
        &self,
        writer: &mut impl DbWriter,
        key_iter: &mut (impl Iterator<Item = TKey> + Clone),
    ) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        let key_iter_clone = key_iter.clone();
        self.cache.remove_many(key_iter);
        for key in key_iter_clone {
            writer.delete(DbKey::new(self.prefix, key))?;
        }
        Ok(())
    }
}

/// A concurrent DB store with typed caching for `Copy` types.
/// TODO: try and generalize under `CachedDbAccess`
#[derive(Clone)]
pub struct CachedDbAccessForCopy<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Copy + Send + Sync,
{
    db: Arc<DB>,

    // Cache
    cache: Cache<TKey, TData>,

    // DB bucket/path
    prefix: &'static [u8],
}

impl<TKey, TData> CachedDbAccessForCopy<TKey, TData>
where
    TKey: Clone + std::hash::Hash + Eq + Send + Sync,
    TData: Clone + Copy + Send + Sync,
{
    pub fn new(db: Arc<DB>, cache_size: u64, prefix: &'static [u8]) -> Self {
        Self { db, cache: Cache::new(cache_size), prefix }
    }

    pub fn has(&self, key: TKey) -> Result<bool, StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
    {
        Ok(self.cache.contains_key(&key) || self.db.get_pinned(DbKey::new(self.prefix, key))?.is_some())
    }

    pub fn read(&self, key: TKey) -> Result<TData, StoreError>
    where
        TKey: Copy + AsRef<[u8]> + ToString,
        TData: DeserializeOwned, // We need `DeserializeOwned` since the slice coming from `db.get_pinned` has short lifetime
    {
        if let Some(data) = self.cache.get(&key) {
            Ok(data)
        } else {
            let db_key = DbKey::new(self.prefix, key);
            if let Some(slice) = self.db.get_pinned(&db_key)? {
                let data: TData = bincode::deserialize(&slice)?;
                self.cache.insert(key, data);
                Ok(data)
            } else {
                Err(StoreError::KeyNotFound(db_key))
            }
        }
    }

    pub fn write(&self, key: TKey, data: TData) -> Result<(), StoreError>
    where
        TKey: Copy + AsRef<[u8]>,
        TData: Serialize,
    {
        self.cache.insert(key, data);
        let bin_data = bincode::serialize(&data)?;
        self.db.put(DbKey::new(self.prefix, key), bin_data)?;
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
        T: Clone + DeserializeOwned,
    {
        if let Some(item) = self.cached_item.read().clone() {
            Ok(item)
        } else if let Some(slice) = self.db.get_pinned(self.key)? {
            let item: T = bincode::deserialize(&slice)?;
            *self.cached_item.write() = Some(item.clone());
            Ok(item)
        } else {
            Err(StoreError::KeyNotFound(DbKey::prefix_only(self.key)))
        }
    }

    pub fn write(&mut self, item: &T) -> Result<(), StoreError>
    where
        T: Clone + Serialize,
    {
        *self.cached_item.write() = Some(item.clone());
        let bin_data = bincode::serialize(&item)?;
        self.db.put(self.key, bin_data)?;
        Ok(())
    }

    pub fn write_batch(&mut self, batch: &mut WriteBatch, item: &T) -> Result<(), StoreError>
    where
        T: Clone + Serialize,
    {
        *self.cached_item.write() = Some(item.clone());
        let bin_data = bincode::serialize(&item)?;
        batch.put(self.key, bin_data);
        Ok(())
    }

    pub fn update<F>(&mut self, writer: &mut impl DbWriter, op: F) -> Result<T, StoreError>
    where
        T: Clone + Serialize + DeserializeOwned,
        F: Fn(T) -> T,
    {
        let mut guard = self.cached_item.write();
        let mut item = if let Some(item) = guard.take() {
            item
        } else if let Some(slice) = self.db.get_pinned(self.key)? {
            let item: T = bincode::deserialize(&slice)?;
            item
        } else {
            return Err(StoreError::KeyNotFound(DbKey::prefix_only(self.key)));
        };

        item = op(item); // Apply the update op
        *guard = Some(item.clone());
        let bin_data = bincode::serialize(&item)?;
        writer.put(self.key, bin_data)?;
        Ok(item)
    }
}

/// Abstraction over direct/batched DB writing
/// TODO: use for all CachedAccess/Item writing ops
pub trait DbWriter {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>;
    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error>;
}

pub struct DirectDbWriter<'a> {
    db: &'a DB,
}

impl<'a> DirectDbWriter<'a> {
    pub fn new(db: &'a DB) -> Self {
        Self { db }
    }
}

impl DbWriter for DirectDbWriter<'_> {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.db.put(key, value)
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error> {
        self.db.delete(key)
    }
}

pub struct BatchDbWriter<'a> {
    batch: &'a mut WriteBatch,
}

impl<'a> BatchDbWriter<'a> {
    pub fn new(batch: &'a mut WriteBatch) -> Self {
        Self { batch }
    }
}

impl DbWriter for BatchDbWriter<'_> {
    fn put<K, V>(&mut self, key: K, value: V) -> Result<(), rocksdb::Error>
    where
        K: AsRef<[u8]>,
        V: AsRef<[u8]>,
    {
        self.batch.put(key, value);
        Ok(())
    }

    fn delete<K: AsRef<[u8]>>(&mut self, key: K) -> Result<(), rocksdb::Error> {
        self.batch.delete(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hashes::Hash;

    #[test]
    fn test_db_key_display() {
        let key1 = DbKey::new(b"human-readable", Hash::from_u64_word(34567890));
        let key2 = DbKey::new(&[1, 2, 2, 89], Hash::from_u64_word(345690));
        let key3 = DbKey::prefix_only(&[1, 2, 2, 89]);
        let key4 = DbKey::prefix_only(b"direct-prefix");
        println!("{}", key1);
        println!("{}", key2);
        println!("{}", key3);
        println!("{}", key4);
    }
}
