use std::sync::Arc;

use kaspa_consensus_core::config::genesis::GenesisBlock;
use kaspa_consensus_core::{
    coinbase::BlockRewardData, tx::TransactionId, utxo::utxo_diff::UtxoDiff, BlockHashMap, BlockHashSet, HashMapCustomHasher,
};
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::prelude::{StoreError, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

use super::ghostdag::GhostdagData;
use super::utxo_set::DbUtxoSetStore;

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

    pub fn from_genesis(genesis: &GenesisBlock, ghostdag_data: GhostdagData) -> Self {
        Self {
            parents: vec![genesis.hash],
            ghostdag_data,
            daa_score: genesis.daa_score,
            bits: genesis.bits,
            past_median_time: genesis.timestamp,
            multiset: MuHash::new(),
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            accepted_tx_ids: genesis.build_genesis_transactions().into_iter().map(|tx| tx.id()).collect(),
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::from_iter(std::iter::once(genesis.hash)),
        }
    }
}

/// Used in order to group virtual related stores under a single lock
pub struct VirtualStores {
    pub state: DbVirtualStateStore,
    pub utxo_set: DbUtxoSetStore,
}

impl VirtualStores {
    pub fn new(db: Arc<DB>, utxoset_cache_size: u64) -> Self {
        Self {
            state: DbVirtualStateStore::new(db.clone()),
            utxo_set: DbUtxoSetStore::new(db, utxoset_cache_size, DatabaseStorePrefixes::VirtualUtxoset.into()),
        }
    }
}

/// Reader API for `VirtualStateStore`.
pub trait VirtualStateStoreReader {
    fn get(&self) -> StoreResult<Arc<VirtualState>>;
}

pub trait VirtualStateStore: VirtualStateStoreReader {
    fn set(&mut self, state: Arc<VirtualState>) -> StoreResult<()>;
}

/// A DB + cache implementation of `VirtualStateStore` trait
#[derive(Clone)]
pub struct DbVirtualStateStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<VirtualState>>,
}

impl DbVirtualStateStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::VirtualState.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn is_initialized(&self) -> StoreResult<bool> {
        match self.access.read() {
            Ok(_) => Ok(true),
            Err(StoreError::KeyNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, state: Arc<VirtualState>) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &state)
    }
}

impl VirtualStateStoreReader for DbVirtualStateStore {
    fn get(&self) -> StoreResult<Arc<VirtualState>> {
        self.access.read()
    }
}

impl VirtualStateStore for DbVirtualStateStore {
    fn set(&mut self, state: Arc<VirtualState>) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), &state)
    }
}
