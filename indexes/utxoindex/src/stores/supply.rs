use std::sync::Arc;

use consensus::model::stores::{
    database::prelude::{CachedDbItem, DirectDbWriter},
    errors::{StoreError, StoreResult},
    DB,
};

use crate::model::{CirculatingSupply, CirculatingSupplyDiff};

/// Reader API for `UtxoIndexTipsStore`.
pub trait CirculatingSupplyStoreReader {
    fn get(&self) -> StoreResult<u64>;
}

pub trait CirculatingSupplyStore: CirculatingSupplyStoreReader {
    fn update_circulating_supply(&mut self, circulating_supply_diff: i64) -> StoreResult<u64>;

    fn insert(&mut self, circulating_supply: u64) -> StoreResult<()>;

    fn remove(&mut self) -> Result<(), StoreError>;
}

pub const CIRCULATING_SUPPLY_STORE_PREFIX: &[u8] = b"circulating-supply";

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbCirculatingSupplyStore {
    db: Arc<DB>,
    access: CachedDbItem<u64>,
}

impl DbCirculatingSupplyStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, CIRCULATING_SUPPLY_STORE_PREFIX) }
    }
}

impl CirculatingSupplyStoreReader for DbCirculatingSupplyStore {
    fn get(&self) -> StoreResult<u64> {
        self.access.read()
    }
}

impl CirculatingSupplyStore for DbCirculatingSupplyStore {
    fn update_circulating_supply(&mut self, circulating_supply_diff: CirculatingSupplyDiff) -> Result<u64, StoreError> {
        let circulating_supply = self.access.update(DirectDbWriter::new(&self.db), move |circulating_supply| {
            circulating_supply + (circulating_supply_diff as CirculatingSupply)
        });
        circulating_supply
    }

    fn insert(&mut self, circulating_supply: CirculatingSupply) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), &circulating_supply)
    }

    fn remove(&mut self) -> Result<(), StoreError> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}
