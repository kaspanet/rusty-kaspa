use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `Source`.
pub trait TxIndexSourceReader {
    fn get(&self) -> StoreResult<Option<Hash>>;
}

pub trait TxIndexSourceStore: TxIndexSourceReader {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
    fn set(&mut self, batch: &mut WriteBatch, new_source: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `TxIndexSource` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbTxIndexSourceStore {
    access: CachedDbItem<Hash>,
}

impl DbTxIndexSourceStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexSource.into()) }
    }
}

impl TxIndexSourceReader for DbTxIndexSourceStore {
    fn get(&self) -> StoreResult<Option<Hash>> {
        self.access.read().map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }
}

impl TxIndexSourceStore for DbTxIndexSourceStore {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.remove(&mut writer)
    }

    fn set(&mut self, batch: &mut WriteBatch, new_source: Hash) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.write(&mut writer, &new_source)
    }
}
