use std::ops::Deref;
use std::sync::Arc;

use super::ghostdag::GhostdagData;
use super::utxo_set::DbUtxoSetStore;
use arc_swap::ArcSwap;
use kaspa_consensus_core::api::stats::VirtualStateStats;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher, block::VirtualStateApproxId, coinbase::BlockRewardData,
    config::genesis::GenesisBlock, utxo::utxo_diff::UtxoDiff,
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
    /// Pre-KIP21: tx digests for accepted_id_merkle_root computation.
    /// Post-KIP21: single-element vec containing the seq_commit hash.
    pub accepted_id_digests: Vec<Hash>,
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
        accepted_id_digests: Vec<Hash>,
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
            accepted_id_digests,
            mergeset_rewards,
            mergeset_non_daa,
        }
    }

    pub fn from_genesis(genesis: &GenesisBlock, ghostdag_data: GhostdagData, covenants_activation: ForkActivation) -> Self {
        let accepted_id_digests = if covenants_activation.is_active(genesis.daa_score) {
            // Post-KIP21: compute the genesis seq_commit by processing genesis
            // transactions through the full lane pipeline.
            use kaspa_hashes::{SeqCommitActiveNode, ZERO_HASH};
            use kaspa_seq_commit::hashing::*;
            use kaspa_seq_commit::types::*;
            use kaspa_smt::SmtHasher;

            let txs = genesis.build_genesis_transactions();
            let blue_score = ghostdag_data.blue_score;

            // Collect per-lane activity from genesis transactions (coinbase)
            let mut lane_activities: std::collections::BTreeMap<[u8; 20], Vec<Hash>> = std::collections::BTreeMap::new();
            for (idx, tx) in txs.iter().enumerate() {
                let lane_id: [u8; 20] = *tx.subnetwork_id.as_bytes();
                let tx_digest = kaspa_consensus_core::hashing::tx::seq_commit_tx_digest(tx.id(), tx.version);
                lane_activities.entry(lane_id).or_default().push(activity_leaf(&tx_digest, idx as u32));
            }

            let context_hash = mergeset_context_hash(&MergesetContext {
                timestamp: seq_commit_timestamp(genesis.timestamp),
                daa_score: genesis.daa_score,
                blue_score,
            });

            // Miner payload from genesis coinbase
            let mpl = miner_payload_leaf(&MinerPayloadLeafInput {
                block_hash: &genesis.hash,
                blue_work_bytes: &kaspa_consensus_core::BlueWorkType::ZERO.to_le_bytes(),
                payload: genesis.coinbase_payload,
            });
            let payload_root = miner_payload_root(std::iter::once(mpl));

            // Build SMT — new lanes anchor at ZERO_HASH (no parent seq_commit)
            let parent_seq_commit = ZERO_HASH;
            let leaf_updates = kaspa_smt::store::SortedLeafUpdates::from_unsorted(lane_activities.iter().map(
                |(lane_id, leaves): (&[u8; 20], &Vec<Hash>)| {
                    let lk = lane_key(lane_id);
                    let ad = activity_digest_lane(leaves.iter().copied());
                    let tip = lane_tip_next(&LaneTipInput {
                        parent_ref: &parent_seq_commit,
                        lane_key: &lk,
                        activity_digest: &ad,
                        context_hash: &context_hash,
                    });
                    kaspa_smt::store::LeafUpdate {
                        key: lk,
                        leaf_hash: smt_leaf_hash(&SmtLeafInput { lane_key: &lk, lane_tip: &tip, blue_score }),
                    }
                },
            ));

            let empty_store = kaspa_smt::store::BTreeSmtStore::new();
            let (lanes_root, _) = kaspa_smt::tree::compute_root_update::<SeqCommitActiveNode, _>(
                &empty_store,
                SeqCommitActiveNode::empty_root(),
                leaf_updates,
            )
            .unwrap();
            let pd = kaspa_seq_commit::hashing::payload_and_context_digest(&context_hash, &payload_root);
            let state_root = seq_state_root(&SeqState { lanes_root: &lanes_root, payload_and_ctx_digest: &pd });
            let commit = seq_commit(&SeqCommitInput { parent_seq_commit: &parent_seq_commit, state_root: &state_root });
            vec![commit]
        } else {
            genesis.build_genesis_transactions().iter().map(|tx| tx.id()).collect()
        };
        Self {
            parents: vec![genesis.hash],
            ghostdag_data,
            daa_score: genesis.daa_score,
            bits: genesis.bits,
            past_median_time: genesis.timestamp,
            multiset: MuHash::new(),
            utxo_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            accepted_id_digests,
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::from_iter(std::iter::once(genesis.hash)),
        }
    }

    pub fn to_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.daa_score, self.ghostdag_data.blue_work, self.ghostdag_data.selected_parent)
    }
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
    pub fn new(db: Arc<DB>, lkg_virtual_state: LkgVirtualState, utxoset_cache_policy: CachePolicy) -> Self {
        Self {
            state: DbVirtualStateStore::new(db.clone(), lkg_virtual_state),
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
    pub fn new(db: Arc<DB>, lkg_virtual_state: LkgVirtualState) -> Self {
        let access = CachedDbItem::new(db.clone(), DatabaseStorePrefixes::VirtualState.into());
        // Init the LKG cache from DB store data
        lkg_virtual_state.store(access.read().optional().unwrap().unwrap_or_default());
        Self { db, access, lkg_virtual_state }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(self.db.clone(), self.lkg_virtual_state.clone())
    }

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
