use std::sync::{Arc, atomic::AtomicU64};

use consensus::model::stores::{
    database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, AtomicCache},
    errors::StoreResult,
    DB,
};
use consensus_core::BlockHashSet;
use hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `UtxoIndexTipsStore`.
pub trait CirculatingSupplyStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait CirculatingSupplyStore: CirculatingSupplyStoreReader {
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: i64) -> StoreResult<u64> ;
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
        Self { db: Arc::clone(&db), access: CachedDbItem::new(self.db, CIRCULATING_SUPPLY_STORE_PREFIX) }
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
    fn add_circulating_supply_diff(&mut self, circulating_supply_diff: i64) -> StoreResult<u64> {
        circulating_supply = self.access.update(DirectDbWriter::new(&self.db), move | circulating_supply |  {
            if circulating_supply_diff > 0 { 
                circulating_supply + circulating_supply_diff as u64
            } else {circulating_supply} 
        }); //force monotonic
        Ok(circulating_supply)    
    }
}
