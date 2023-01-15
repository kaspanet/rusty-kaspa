use std::{sync::{Arc, atomic::AtomicU64}, ops::Deref};

use consensus::model::stores::{
    database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter},
    errors::StoreResult,
    DB,
};
use consensus_core::BlockHashSet;
use hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `UtxoIndexTipsStore`.
pub trait CirculatingSupplyStoreReader {
    fn get(&self) -> StoreResult<Arc<u64>>;
}

pub trait CirculatingSupplyStore: CirculatingSupplyStoreReader {
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: i64) -> StoreResult<u64>;
    
    fn insert(&mut self, circulating_supply: u64);
}

pub const CIRCULATING_SUPPLY_STORE_PREFIX: &[u8] = b"utxoindex-circulating-supply";

/// A DB + cache implementation of `UtxoIndexTipsStore` trait
#[derive(Clone)]
pub struct DbCirculatingSupplyStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<u64>>,
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
    fn get(&self) -> StoreResult<Arc<u64>> {
        self.access.read()
    }
}

impl CirculatingSupplyStore for DbCirculatingSupplyStore {
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: i64) -> Result<Arc<u64>, StoreError>
    {
        let circulating_supply = self.access.update(DirectDbWriter::new(&self.db), move | circulating_supply |  {
            if circulating_supply_diff > 0 { 
               Arc::new(circulating_supply.deref() + (circulating_supply_diff as u64))
            } else { circulating_supply }
        }); //force monotonic
        circulating_supply
    }

    fn insert(&mut self, circulating_supply: u64) {
        self.access.write(DirectDbWriter::new(&self.db), &Arc::new(circulating_supply))  
    }
}
