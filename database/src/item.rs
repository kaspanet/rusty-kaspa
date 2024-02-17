use crate::{
    db::DB,
    errors::StoreError,
    prelude::{DbSetAccess, ReadLock},
};

use super::prelude::{DbKey, DbWriter};
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::{hash_map::RandomState, HashSet},
    hash::BuildHasher,
    sync::Arc,
};

/// A cached DB item with concurrency support
#[derive(Clone)]
pub struct CachedDbItem<T> {
    db: Arc<DB>,
    key: Vec<u8>,
    cached_item: Arc<RwLock<Option<T>>>,
}

impl<T> CachedDbItem<T> {
    pub fn new(db: Arc<DB>, key: Vec<u8>) -> Self {
        Self { db, key, cached_item: Arc::new(RwLock::new(None)) }
    }

    pub fn read(&self) -> Result<T, StoreError>
    where
        T: Clone + DeserializeOwned,
    {
        if let Some(item) = self.cached_item.read().clone() {
            return Ok(item);
        }
        if let Some(slice) = self.db.get_pinned(&self.key)? {
            let item: T = bincode::deserialize(&slice)?;
            *self.cached_item.write() = Some(item.clone());
            Ok(item)
        } else {
            Err(StoreError::KeyNotFound(DbKey::prefix_only(&self.key)))
        }
    }

    pub fn write(&mut self, mut writer: impl DbWriter, item: &T) -> Result<(), StoreError>
    where
        T: Clone + Serialize,
    {
        *self.cached_item.write() = Some(item.clone());
        let bin_data = bincode::serialize(item)?;
        writer.put(&self.key, bin_data)?;
        Ok(())
    }

    pub fn remove(&mut self, mut writer: impl DbWriter) -> Result<(), StoreError>
where {
        *self.cached_item.write() = None;
        writer.delete(&self.key)?;
        Ok(())
    }

    pub fn update<F>(&mut self, mut writer: impl DbWriter, op: F) -> Result<T, StoreError>
    where
        T: Clone + Serialize + DeserializeOwned,
        F: Fn(T) -> T,
    {
        let mut guard = self.cached_item.write();
        let mut item = if let Some(item) = guard.take() {
            item
        } else if let Some(slice) = self.db.get_pinned(&self.key)? {
            let item: T = bincode::deserialize(&slice)?;
            item
        } else {
            return Err(StoreError::KeyNotFound(DbKey::prefix_only(&self.key)));
        };

        item = op(item); // Apply the update op
        *guard = Some(item.clone());
        let bin_data = bincode::serialize(&item)?;
        writer.put(&self.key, bin_data)?;
        Ok(item)
    }
}

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
struct EmptyKey;

impl AsRef<[u8]> for EmptyKey {
    fn as_ref(&self) -> &[u8] {
        &[]
    }
}

type LockedSet<T, S> = Arc<RwLock<HashSet<T, S>>>;

#[derive(Clone)]
pub struct CachedDbSetItem<T: Clone + Send + Sync, S = RandomState> {
    access: DbSetAccess<EmptyKey, T>,
    cached_set: Arc<RwLock<Option<LockedSet<T, S>>>>,
}

impl<T, S> CachedDbSetItem<T, S>
where
    T: Clone + std::hash::Hash + Eq + Send + Sync + DeserializeOwned + Serialize,
    S: BuildHasher + Default,
{
    pub fn new(db: Arc<DB>, key: Vec<u8>) -> Self {
        Self { access: DbSetAccess::new(db, key), cached_set: Arc::new(RwLock::new(None)) }
    }

    fn read_locked_set(&self) -> Result<LockedSet<T, S>, StoreError>
    where
        T: Clone + DeserializeOwned,
    {
        if let Some(item) = self.cached_set.read().clone() {
            return Ok(item);
        }
        let set = self.access.bucket_iterator(EmptyKey).collect::<Result<HashSet<_, _>, _>>()?;
        let set = Arc::new(RwLock::new(set));
        self.cached_set.write().replace(set.clone());
        Ok(set)
    }

    pub fn read(&self) -> Result<ReadLock<HashSet<T, S>>, StoreError>
    where
        T: Clone + DeserializeOwned,
    {
        Ok(ReadLock::new(self.read_locked_set()?))
    }

    pub fn update(
        &mut self,
        mut writer: impl DbWriter,
        added_items: &[T],
        removed_items: &[T],
    ) -> Result<ReadLock<HashSet<T, S>>, StoreError>
    where
        T: Clone + Serialize,
    {
        let set = self.read_locked_set()?;
        {
            let mut set_write = set.write();
            for item in removed_items.iter() {
                self.access.delete(&mut writer, EmptyKey, item.clone())?;
                set_write.remove(item);
            }
            for item in added_items.iter().cloned() {
                self.access.write(&mut writer, EmptyKey, item.clone())?;
                set_write.insert(item);
            }
        }
        Ok(ReadLock::new(set))
    }
}
