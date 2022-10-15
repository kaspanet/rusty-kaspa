use std::sync::Arc;

use super::{caching::CachedDbItem, errors::StoreResult, ghostdag::GhostdagData, DB};
use consensus_core::utxo::utxo_diff::UtxoDiff;
use hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct VirtualState {
    pub parents: Vec<Hash>,
    pub ghostdag_data: GhostdagData,
    pub utxo_diff: UtxoDiff,
}

impl VirtualState {
    pub fn from_genesis(genesis_hash: Hash, initial_ghostdag_data: GhostdagData) -> Self {
        Self {
            parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
        }
    }
}

/// Reader API for `VirtualStateStore`.
pub trait VirtualStateStoreReader {
    fn get(&self) -> StoreResult<Arc<VirtualState>>;
}

pub trait VirtualStateStore: VirtualStateStoreReader {
    fn set(&mut self, state: VirtualState) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"virtual-state";

/// A DB + cache implementation of `VirtualStateStore` trait
#[derive(Clone)]
pub struct DbVirtualStateStore {
    raw_db: Arc<DB>,
    cached_access: CachedDbItem<Arc<VirtualState>>,
}

impl DbVirtualStateStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbItem::new(db.clone(), STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, state: VirtualState) -> StoreResult<()> {
        self.cached_access.write_batch(batch, &Arc::new(state))
    }
}

impl VirtualStateStoreReader for DbVirtualStateStore {
    fn get(&self) -> StoreResult<Arc<VirtualState>> {
        self.cached_access.read()
    }
}

impl VirtualStateStore for DbVirtualStateStore {
    fn set(&mut self, state: VirtualState) -> StoreResult<()> {
        self.cached_access.write(&Arc::new(state))
    }
}
