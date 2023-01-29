use std::sync::Arc;

use consensus::model::stores::{
    database::prelude::{CachedDbItem, DirectDbWriter},
    errors::{StoreError, StoreResult},
    DB,
};

use crate::core::{CirculatingSupply, CirculatingSupplyDiff};

/// Reader API for `UtxoIndexTipsStore`.
pub trait CirculatingSupplyStoreReader {
    fn get(&self) -> StoreResult<u64>;
}

pub trait CirculatingSupplyStore: CirculatingSupplyStoreReader {
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: i64) -> StoreResult<u64>;

    fn insert(&mut self, circulating_supply: u64) -> StoreResult<()>;

    fn remove(&mut self) -> Result<(), StoreError>;
}

pub const CIRCULATING_SUPPLY_STORE_PREFIX: &[u8] = b"utxoindex-circulating-supply";

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

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }
}

impl CirculatingSupplyStoreReader for DbCirculatingSupplyStore {
    fn get(&self) -> StoreResult<u64> {
        self.access.read()
    }
}

impl CirculatingSupplyStore for DbCirculatingSupplyStore {
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: CirculatingSupplyDiff) -> Result<u64, StoreError> {
        let circulating_supply = self.access.update(DirectDbWriter::new(&self.db), move |circulating_supply| {
            circulating_supply + (circulating_supply_diff as CirculatingSupply)
        }); //force monotonic
        circulating_supply
    }

    fn insert(&mut self, circulating_supply: CirculatingSupply) -> Result<(), StoreError> {
        self.access.write(DirectDbWriter::new(&self.db), &circulating_supply)
    }

    fn remove(&mut self) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.remove(writer)
    }
}
