use std::sync::Arc;

use kaspa_database::{
    prelude::{CachedDbItem, DB, DirectDbWriter, StoreResult},
    registry::DatabaseStorePrefixes,
};

pub trait UtxoIndexDbVersionStoreReader {
    fn get(&self) -> StoreResult<u16>;
}

pub trait UtxoIndexDbVersionStore: UtxoIndexDbVersionStoreReader {
    fn set(&mut self, version: u16) -> StoreResult<()>;
    fn remove(&mut self) -> StoreResult<()>;
}

#[derive(Clone)]
pub struct DbUtxoIndexDbVersionStore {
    db: Arc<DB>,
    access: CachedDbItem<u16>,
}

impl DbUtxoIndexDbVersionStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::UtxoIndexDbVersion.into()) }
    }
}

impl UtxoIndexDbVersionStoreReader for DbUtxoIndexDbVersionStore {
    fn get(&self) -> StoreResult<u16> {
        self.access.read()
    }
}

impl UtxoIndexDbVersionStore for DbUtxoIndexDbVersionStore {
    fn set(&mut self, version: u16) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &version)
    }

    fn remove(&mut self) -> StoreResult<()> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}