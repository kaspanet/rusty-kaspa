use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `Source`.
pub trait TxIndexSourceReader {
    fn get(&self) -> StoreResult<Option<Hash>>;
}

pub trait TxIndexSourceStore: TxIndexSourceReader {
    fn set(&mut self, source: Hash) -> StoreResult<()>;
    fn remove_batch_via_batch_writer(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
    fn replace_if_new(&mut self, batch: &mut WriteBatch, new_source: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `TxIndexSource` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbTxIndexSourceStore {
    db: Arc<DB>,
    access: CachedDbItem<Hash>,
}

impl DbTxIndexSourceStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexSource.into()) }
    }
}

impl TxIndexSourceReader for DbTxIndexSourceStore {
    fn get(&self) -> StoreResult<Option<Hash>> {
        self.access.read().map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }
}

impl TxIndexSourceStore for DbTxIndexSourceStore {
    fn remove_batch_via_batch_writer(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.remove(&mut writer)
    }

    fn set(&mut self, source: Hash) -> StoreResult<()> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.write(&mut writer, &source)
    }

    fn replace_if_new(&mut self, batch: &mut WriteBatch, new_source: Hash) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        if let Some(old_source) = self.get()? {
            if old_source == new_source {
                return Ok(());
            };
        };
        self.access.write(&mut writer, &new_source)
    }
}
