// This is required to keep block added / included transactions in sync.

use std::sync::Arc;

use kaspa_consensus_core::Hash;
use kaspa_database::{
    prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SinkData {
    sink_hash: Hash,
    sink_blue_score: u64,
}

pub trait TxIndexSinkStoreReader {
    fn get_sink(&self) -> StoreResult<Hash>;
    fn get_sink_blue_score(&self) -> StoreResult<u64>;
}

pub trait TxIndexSinkStore: TxIndexSinkStoreReader {
    fn set_sink(&mut self, writer: BatchDbWriter, new_sink: Hash, new_sink_blue_score: u64) -> StoreResult<()>;
    fn remove_sink(&mut self, writer: BatchDbWriter) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbTxIndexSinkStore {
    db: Arc<DB>,
    access: CachedDbItem<SinkData>,
}

impl DbTxIndexSinkStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::TxIndexSink.into()) }
    }
}

impl TxIndexSinkStoreReader for DbTxIndexSinkStore {
    fn get_sink(&self) -> StoreResult<Hash> {
        let sink_data = self.access.read()?;
        Ok(sink_data.sink_hash)
    }

    fn get_sink_blue_score(&self) -> StoreResult<u64> {
        let sink_data = self.access.read()?;
        Ok(sink_data.sink_blue_score)
    }
}

impl TxIndexSinkStore for DbTxIndexSinkStore {
    fn set_sink(&mut self, mut writer: BatchDbWriter, new_sink: Hash, new_sink_blue_score: u64) -> StoreResult<()> {
        let sink_data = SinkData { sink_hash: new_sink, sink_blue_score: new_sink_blue_score };
        self.access.write(&mut writer, &sink_data)
    }

    fn remove_sink(&mut self, mut writer: BatchDbWriter) -> StoreResult<()> {
        self.access.remove(&mut writer)
    }
}

// --- tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, StoreError, WriteBatch, DB},
    };
    use kaspa_hashes::Hash;
    use std::sync::Arc;

    #[test]
    fn test_txindex_sink_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTxIndexSinkStore::new(txindex_db.clone());
        let sink1 = Hash::from_slice(&[1u8; 32]);
        let sink2 = Hash::from_slice(&[2u8; 32]);

        // Initially empty
        assert!(matches!(store.get_sink().unwrap_err(), StoreError::KeyNotFound(_)));

        // Set sink1
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_sink(writer, sink1, 0).unwrap();
        txindex_db.write(write_batch).unwrap();
        let retrieved_sink = store.get_sink().unwrap();
        let retrieved_sink_blue_score = store.get_sink_blue_score().unwrap();
        assert_eq!(retrieved_sink_blue_score, 0);
        assert_eq!(retrieved_sink, sink1);

        // Update to sink2
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.set_sink(writer, sink2, 1).unwrap();
        txindex_db.write(write_batch).unwrap();
        let retrieved_sink = store.get_sink().unwrap();
        let retrieved_sink_blue_score = store.get_sink_blue_score().unwrap();
        assert_eq!(retrieved_sink_blue_score, 1);
        assert_eq!(retrieved_sink, sink2);

        // Remove sink
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.remove_sink(writer).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert!(matches!(store.get_sink().unwrap_err(), StoreError::KeyNotFound(_)));
    }
}
