use std::sync::Arc;

use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, StoreError, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `SinkStore`.
pub trait TxIndexSinkDataReader {
    fn get(&self) -> StoreResult<Option<SinkData>>;
}

pub trait TxIndexSinkDataStore: TxIndexSinkReader {
    fn set(&mut self, batch: &mut WriteBatch, sink: Hash) -> StoreResult<()>;
    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()>;
}

/// A DB + cache implementation of `SinkStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbTxIndexSinkStore {
    access: CachedDbItem<SinkData>,
}

impl DbTxIndexSinkStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexSink.into()) }
    }
}

impl TxIndexSinkReader for DbTxIndexSinkStore {
    fn get(&self) -> StoreResult<Option<SinkData>> {
        self.access.read().map(Some).or_else(|e| if let StoreError::KeyNotFound(_) = e { Ok(None) } else { Err(e) })
    }
}

impl TxIndexSinkStore for DbTxIndexSinkStore {
    fn set(&mut self, batch: &mut WriteBatch, sink_data: SinkData) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.write(&mut writer, &sink)
    }

    fn remove(&mut self, batch: &mut WriteBatch) -> StoreResult<()> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.remove(&mut writer)
    }
}
