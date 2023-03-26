use crate::{db::DB, errors::StoreError};

use super::prelude::{DbKey, DbWriter};
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::Arc;

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
