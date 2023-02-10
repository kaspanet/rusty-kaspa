use std::sync::Arc;

use consensus_core::{
    coinbase::BlockRewardData, events::VirtualChangeSetEvent, tx::TransactionId, utxo::utxo_diff::UtxoDiff, BlockHashMap,
    BlockHashSet, HashMapCustomHasher,
};
use database::prelude::StoreResult;
use database::prelude::DB;
use database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use hashes::Hash;
use muhash::MuHash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

use super::ghostdag::GhostdagData;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct VirtualState {
    pub parents: Vec<Hash>,
    pub ghostdag_data: GhostdagData,
    pub daa_score: u64,
    pub bits: u32,
    pub past_median_time: u64,
    pub multiset: MuHash,
    pub utxo_diff: UtxoDiff, // This is the UTXO diff from the selected tip to the virtual. i.e., if this diff is applied on the past UTXO of the selected tip, we'll get the virtual UTXO set.
    pub accepted_tx_ids: Vec<TransactionId>, // TODO: consider saving `accepted_id_merkle_root` directly
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub mergeset_non_daa: BlockHashSet,
}

impl VirtualState {
    pub fn new(
        parents: Vec<Hash>,
        daa_score: u64,
        bits: u32,
        past_median_time: u64,
        multiset: MuHash,
        utxo_diff: UtxoDiff,
        accepted_tx_ids: Vec<TransactionId>,
        mergeset_rewards: BlockHashMap<BlockRewardData>,
        mergeset_non_daa: BlockHashSet,
        ghostdag_data: GhostdagData,
    ) -> Self {
        Self {
            parents,
            ghostdag_data,
            daa_score,
            bits,
            past_median_time,
            multiset,
            utxo_diff,
            accepted_tx_ids,
            mergeset_rewards,
            mergeset_non_daa,
        }
    }

    pub fn from_genesis(
        genesis_hash: Hash,
        genesis_bits: u32,
        past_median_time: u64,
        accepted_tx_ids: Vec<TransactionId>,
        initial_ghostdag_data: GhostdagData,
    ) -> Self {
        Self {
            parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            daa_score: 0,
            bits: genesis_bits,
            past_median_time,
            multiset: MuHash::new(),
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            accepted_tx_ids,
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::from_iter(std::iter::once(genesis_hash)),
        }
    }
}

impl From<VirtualState> for VirtualChangeSetEvent {
    fn from(virtual_state: VirtualState) -> Self {
        Self {
            utxo_diff: Arc::new(virtual_state.utxo_diff),
            parents: Arc::new(virtual_state.parents),
            selected_parent_blue_score: virtual_state.ghostdag_data.blue_score,
            daa_score: virtual_state.daa_score,
            mergeset_blues: virtual_state.ghostdag_data.mergeset_blues,
            mergeset_reds: virtual_state.ghostdag_data.mergeset_reds,
            accepted_tx_ids: Arc::new(virtual_state.accepted_tx_ids),
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
