use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use super::ghostdag::GhostdagData;
use super::utxo_set::DbUtxoSetStore;
use crate::model::stores::block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore};
use arc_swap::ArcSwap;
use kaspa_consensus_core::api::stats::VirtualStateStats;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher, block::VirtualStateApproxId, coinbase::BlockRewardData,
    config::genesis::GenesisBlock, tx::TransactionId, utxo::utxo_diff::UtxoDiff,
};
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter, StoreResultExt};
use kaspa_database::prelude::{CachePolicy, StoreResult};
use kaspa_database::prelude::{DB, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

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
    pub accepted_tx_payload_digests: Vec<Hash>,
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub mergeset_non_daa: BlockHashSet,
}

impl VirtualState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        parents: Vec<Hash>,
        daa_score: u64,
        bits: u32,
        past_median_time: u64,
        multiset: MuHash,
        utxo_diff: UtxoDiff,
        accepted_tx_ids: Vec<TransactionId>,
        accepted_tx_payload_digests: Vec<Hash>,
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
            accepted_tx_payload_digests,
            mergeset_rewards,
            mergeset_non_daa,
        }
    }

    pub fn from_genesis(genesis: &GenesisBlock, ghostdag_data: GhostdagData) -> Self {
        let genesis_txs = genesis.build_genesis_transactions();
        Self {
            parents: vec![genesis.hash],
            ghostdag_data,
            daa_score: genesis.daa_score,
            bits: genesis.bits,
            past_median_time: genesis.timestamp,
            multiset: MuHash::new(),
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            accepted_tx_ids: genesis_txs.iter().map(|tx| tx.id()).collect(),
            accepted_tx_payload_digests: genesis_txs.iter().map(|tx| tx.payload_digest()).collect(),
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::from_iter(std::iter::once(genesis.hash)),
        }
    }

    pub fn to_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.daa_score, self.ghostdag_data.blue_work, self.ghostdag_data.selected_parent)
    }

    fn from_deprecated(
        VirtualStateDeprecated {
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
        }: VirtualStateDeprecated,
        store: &DbBlockTransactionsStore,
    ) -> Self {
        let mut txid_to_payload: HashMap<_, _, std::hash::RandomState> =
            HashMap::from_iter(accepted_tx_ids.iter().map(|tx| (*tx, None)));
        for merged_block in ghostdag_data.mergeset_blues.iter().chain(ghostdag_data.mergeset_reds.iter()).copied().rev() {
            let txs = store.get(merged_block).unwrap();
            for tx in txs.iter() {
                txid_to_payload.entry(tx.id()).and_modify(|old| {
                    old.replace(tx.payload_digest());
                });
            }
        }
        Self {
            parents,
            ghostdag_data,
            daa_score,
            bits,
            past_median_time,
            multiset,
            utxo_diff,
            accepted_tx_payload_digests: accepted_tx_ids.iter().map(|txid| txid_to_payload[txid].unwrap()).collect(),
            accepted_tx_ids,
            mergeset_rewards,
            mergeset_non_daa,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct VirtualStateDeprecated {
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

impl From<&VirtualState> for VirtualStateStats {
    fn from(state: &VirtualState) -> Self {
        Self {
            num_parents: state.parents.len() as u32,
            daa_score: state.daa_score,
            bits: state.bits,
            past_median_time: state.past_median_time,
        }
    }
}

/// Represents the "last known good" virtual state. To be used by any logic which does not want to wait
/// for a possible virtual state write to complete but can rather settle with the last known state
#[derive(Clone, Default)]
pub struct LkgVirtualState {
    inner: Arc<ArcSwap<VirtualState>>,
}

/// Guard for accessing the last known good virtual state (lock-free)
/// It's a simple newtype over arc_swap::Guard just to avoid explicit dependency
pub struct LkgVirtualStateGuard(arc_swap::Guard<Arc<VirtualState>>);

impl Deref for LkgVirtualStateGuard {
    type Target = Arc<VirtualState>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl LkgVirtualState {
    /// Provides a temporary borrow to the last known good virtual state.
    pub fn load(&self) -> LkgVirtualStateGuard {
        LkgVirtualStateGuard(self.inner.load())
    }

    /// Loads the last known good virtual state.
    pub fn load_full(&self) -> Arc<VirtualState> {
        self.inner.load_full()
    }

    // Kept private in order to make sure it is only updated by DbVirtualStateStore
    fn store(&self, virtual_state: Arc<VirtualState>) {
        self.inner.store(virtual_state)
    }
}

/// Used in order to group virtual related stores under a single lock
pub struct VirtualStores {
    pub state: DbVirtualStateStore,
    pub utxo_set: DbUtxoSetStore,
}

impl VirtualStores {
    pub fn new(
        db: Arc<DB>,
        lkg_virtual_state: LkgVirtualState,
        utxoset_cache_policy: CachePolicy,
        db_tx_store: &DbBlockTransactionsStore,
    ) -> Self {
        Self {
            state: DbVirtualStateStore::new(db.clone(), lkg_virtual_state, db_tx_store),
            utxo_set: DbUtxoSetStore::new(db, utxoset_cache_policy, DatabaseStorePrefixes::VirtualUtxoset.into()),
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
    /// The "last known good" virtual state
    lkg_virtual_state: LkgVirtualState,
}

impl DbVirtualStateStore {
    pub fn new(db: Arc<DB>, lkg_virtual_state: LkgVirtualState, db_tx_store: &DbBlockTransactionsStore) -> Self {
        let mut access_v1 = CachedDbItem::new(db.clone(), DatabaseStorePrefixes::VirtualStateV1.into());
        let state = if let Some(state) = access_v1.read().optional().unwrap() {
            state
        } else {
            let access_deprecated = CachedDbItem::new(db.clone(), DatabaseStorePrefixes::VirtualState.into());
            if let Some(state) = access_deprecated.read().optional().unwrap().unwrap_or_default() {
                let state = Arc::new(VirtualState::from_deprecated(state, db_tx_store));
                access_v1.write(DirectDbWriter::new(&db), &state).unwrap();
                state
            } else {
                Arc::new(VirtualState::default())
            }
        };
        // Init the LKG cache from DB store data
        lkg_virtual_state.store(state);
        Self { db, access: access_v1, lkg_virtual_state }
    }

    // pub fn clone_with_new_cache(&self) -> Self {
    //     Self::new(self.db.clone(), self.lkg_virtual_state.clone())
    // }

    pub fn is_initialized(&self) -> StoreResult<bool> {
        match self.access.read() {
            Ok(_) => Ok(true),
            Err(StoreError::KeyNotFound(_)) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub fn set_batch(&mut self, batch: &mut WriteBatch, state: Arc<VirtualState>) -> StoreResult<()> {
        self.lkg_virtual_state.store(state.clone()); // Keep the LKG cache up-to-date
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
        self.lkg_virtual_state.store(state.clone()); // Keep the LKG cache up-to-date
        self.access.write(DirectDbWriter::new(&self.db), &state)
    }
}
