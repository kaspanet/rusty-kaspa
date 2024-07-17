use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `Source`.
pub trait TxIndexHistoryRootReader {
    fn get(&self) -> StoreResult<Option<Hash>>;
}

pub trait TxIndexHistoryRootStore: TxIndexHistoryRootReader {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
    fn set(&mut self, new_history_root: Hash) -> StoreResult<()>;
    fn set_if_new(&mut self, batch: &mut WriteBatch, new_source: Hash) -> StoreResult<()>;
}

/// A DB + cache implementation of `TxIndexSource` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbTxIndexHistoryRootStore {
    db: Arc<DB>,
    access: CachedDbItem<Hash>,
}

impl DbTxIndexHistoryRootStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexHistoryRoot.into()) }
    }
}

impl TxIndexHistoryRootReader for DbTxIndexHistoryRootStore {
    fn get(&self) -> StoreResult<Option<Hash>> {
        self.access.read().map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }
}

impl TxIndexHistoryRootStore for DbTxIndexHistoryRootStore {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.remove(&mut writer)
    }

    fn set(&mut self, new_history_root: Hash) -> StoreResult<()> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.write(&mut writer, &new_history_root)
    }

    fn set_if_new(&mut self, batch: &mut WriteBatch, history_root_candidate: Hash) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        if let Some(old_history_root) = self.get()? {
            if old_history_root == history_root_candidate {
                return Ok(());
            };
        };
        self.access.write(&mut writer, &history_root_candidate)
    }
}
