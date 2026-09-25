use std::sync::Arc;

use kaspa_database::{
    prelude::{CachedDbItem, DB, DirectDbWriter, StoreResult},
    registry::DatabaseStorePrefixes,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UtxoIndexDbMetaData {
    pub version: u16,
}

pub trait UtxoIndexDbMetaStoreReader {
    fn get(&self) -> StoreResult<UtxoIndexDbMetaData>;
}

pub trait UtxoIndexDbMetaStore: UtxoIndexDbMetaStoreReader {
    fn set(&mut self, version: UtxoIndexDbMetaData) -> StoreResult<()>;
    fn remove(&mut self) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbUtxoIndexDbMetaStore {
    db: Arc<DB>,
    access: CachedDbItem<UtxoIndexDbMetaData>,
}

impl DbUtxoIndexDbMetaStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::UtxoIndexDbVersion.into()) }
    }
}

impl UtxoIndexDbMetaStoreReader for DbUtxoIndexDbMetaStore {
    fn get(&self) -> StoreResult<UtxoIndexDbMetaData> {
        self.access.read()
    }
}

impl UtxoIndexDbMetaStore for DbUtxoIndexDbMetaStore {
    fn set(&mut self, version: UtxoIndexDbMetaData) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &version)
    }

    fn remove(&mut self) -> StoreResult<()> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}
