use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use kaspa_index_core::models::txindex::TxIndexPruningState;
use rocksdb::WriteBatch;

/// Reader API for `Source`.
pub trait TxIndexPruningStateReader {
    fn get(&self) -> StoreResult<Option<TxIndexPruningState>>;
}

pub trait TxIndexPruningStateStore: TxIndexPruningStateReader {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
    fn set(&mut self, batch: &mut WriteBatch, pruning_state: TxIndexPruningState) -> StoreResult<()>;
    fn update(&mut self, batch: &mut WriteBatch, op: Fn(TxIndexPruningState) -> TxIndexPruningState) -> StoreResult<()>;
}

/// A DB + cache implementation of `TxIndexSource` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbTxIndexPruningStateStore {
    access: CachedDbItem<Hash>,
}

impl DbTxIndexPruningStateStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexPruningState.into()) }
    }
}

impl TxIndexPruningStateReader for DbTxIndexPruningStateStore {
    fn get(&self) -> StoreResult<Option<TxIndexPruningState>> {
        self.access.read().map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }
}

impl TxIndexPruningStateStore for DbTxIndexPruningStateStore {
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.remove(&mut writer)
    }

    fn set(&mut self, batch: &mut WriteBatch, pruning_state: TxIndexPruningState) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.write(&mut writer, &new_source)
    }

    fn update(&mut self, batch: &mut WriteBatch, op: Fn(TxIndexPruningState) -> TxIndexPruningState) -> StoreResult<()> {
        self.access.update(batch, op)
    }
}
