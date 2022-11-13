use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter},
    errors::StoreResult,
    ghostdag::GhostdagData,
    DB,
};
use consensus_core::{
    coinbase::BlockRewardData, tx::TransactionId, utxo::utxo_diff::UtxoDiff, BlockHashMap, BlockHashSet, HashMapCustomHasher,
};
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
    pub bits: u32,
    pub multiset: MuHash,
    pub utxo_diff: UtxoDiff,
    pub accepted_tx_ids: Vec<TransactionId>, // TODO: consider saving `accepted_id_merkle_root` directly
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub mergeset_non_daa: BlockHashSet,
    pub pruning_point: Hash,
}

impl VirtualState {
    pub fn new(
        parents: Vec<Hash>,
        ghostdag_data: Arc<GhostdagData>,
        daa_score: u64,
        bits: u32,
        multiset: MuHash,
        utxo_diff: UtxoDiff,
        accepted_tx_ids: Vec<TransactionId>,
        mergeset_rewards: BlockHashMap<BlockRewardData>,
        mergeset_non_daa: BlockHashSet,
        pruning_point: Hash,
    ) -> Self {
        Self {
            parents,
            ghostdag_data: ArcExtensions::unwrap_or_clone(ghostdag_data),
            daa_score,
            bits,
            multiset,
            utxo_diff,
            accepted_tx_ids,
            mergeset_rewards,
            mergeset_non_daa,
            pruning_point,
        }
    }

    pub fn from_genesis(genesis_hash: Hash, genesis_bits: u32, initial_ghostdag_data: GhostdagData) -> Self {
        Self {
            parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            daa_score: 0,
            bits: genesis_bits,
            multiset: MuHash::new(),
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            accepted_tx_ids: Vec::new(),
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::from_iter(std::iter::once(genesis_hash)),
            pruning_point: genesis_hash,
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
