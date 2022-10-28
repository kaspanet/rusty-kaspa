use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter},
    errors::StoreResult,
    ghostdag::GhostdagData,
    DB,
};
use consensus_core::utxo::utxo_diff::UtxoDiff;
use hashes::Hash;
use kaspa_utils::arc::ArcExtensions;
use muhash::MuHash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct VirtualState {
    pub parents: Vec<Hash>,
    pub ghostdag_data: GhostdagData,
    pub daa_score: u64,
    pub multiset: MuHash,
    pub utxo_diff: UtxoDiff,
}

impl VirtualState {
    pub fn new(parents: Vec<Hash>, ghostdag_data: Arc<GhostdagData>, daa_score: u64, multiset: MuHash, utxo_diff: UtxoDiff) -> Self {
        Self { parents, ghostdag_data: ArcExtensions::unwrap_or_clone(ghostdag_data), daa_score, multiset, utxo_diff }
    }

    pub fn from_genesis(genesis_hash: Hash, initial_ghostdag_data: GhostdagData) -> Self {
        Self {
            parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            daa_score: 0,
            multiset: MuHash::new(),
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
    db: Arc<DB>,
    access: CachedDbItem<Arc<VirtualState>>,
}

impl DbVirtualStateStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, state: VirtualState) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &Arc::new(state))
    }
}

impl VirtualStateStoreReader for DbVirtualStateStore {
    fn get(&self) -> StoreResult<Arc<VirtualState>> {
        self.access.read()
    }
}

impl VirtualStateStore for DbVirtualStateStore {
    fn set(&mut self, state: VirtualState) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &Arc::new(state))
    }
}
