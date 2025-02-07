use std::sync::Arc;

use kaspa_database::{
    prelude::{CachedDbItem, DirectDbWriter, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};

use crate::model::{CirculatingSupply, CirculatingSupplyDiff};

/// Reader API for `UtxoIndexTipsStore`.
pub trait CirculatingSupplyStoreReader {
    fn get(&self) -> StoreResult<u64>;
}

pub trait CirculatingSupplyStore: CirculatingSupplyStoreReader {
    fn update_circulating_supply(&mut self, supply_delta: CirculatingSupplyDiff) -> StoreResult<u64>;
    fn insert(&mut self, circulating_supply: u64) -> StoreResult<()>;
    fn remove(&mut self) -> StoreResult<()>;
}

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbCirculatingSupplyStore {
    db: Arc<DB>,
    access: CachedDbItem<u64>,
}

impl DbCirculatingSupplyStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::CirculatingSupply.into()) }
    }
}

impl CirculatingSupplyStoreReader for DbCirculatingSupplyStore {
    fn get(&self) -> StoreResult<u64> {
        self.access.read()
    }
}

impl CirculatingSupplyStore for DbCirculatingSupplyStore {
    fn update_circulating_supply(&mut self, supply_delta: CirculatingSupplyDiff) -> StoreResult<u64> {
        if supply_delta == 0 {
            return self.get();
        }

        let circulating_supply = self
            .access
            .update(DirectDbWriter::new(&self.db), move |circulating_supply| circulating_supply.saturating_add_signed(supply_delta));

        circulating_supply
    }

    fn insert(&mut self, circulating_supply: CirculatingSupply) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &circulating_supply)
    }

    fn remove(&mut self) -> StoreResult<()> {
        self.access.remove(DirectDbWriter::new(&self.db))
    }
}
