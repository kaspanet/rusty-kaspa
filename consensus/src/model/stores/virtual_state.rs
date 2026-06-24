use std::ops::Deref;
use std::sync::Arc;

use super::ghostdag::GhostdagData;
use super::utxo_set::DbUtxoSetStore;
use arc_swap::ArcSwap;
use kaspa_consensus_core::api::stats::VirtualStateStats;
use kaspa_consensus_core::utxo::pre_toccata::PreToccataUtxoDiff;
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

/// Version suffix appended to every post-Toccata `VirtualState` row's DB key.
/// Pre-Toccata rows have no suffix and are decoded through
/// [`PreToccataVirtualState`] by the version-aware path in [`CachedDbItem`].
pub const POST_TOCCATA_VIRTUAL_STATE_VERSION: u8 = 1;

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

    /// Build the initial virtual state for genesis. `accepted_id_digests` must be
    /// pre-computed by the caller (the virtual processor) — pre-KIP21 it's the vec
    /// of genesis tx ids; post-KIP21 it's a single-element vec with the genesis
    /// `seq_commit`.
    pub fn from_genesis(genesis: &GenesisBlock, ghostdag_data: GhostdagData, accepted_id_digests: Vec<Hash>) -> Self {
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

/// Pre-Toccata layout of [`VirtualState`]. Structurally identical to the
/// live type, except `utxo_diff` decodes through [`PreToccataUtxoDiff`] —
/// whose inner [`crate::tx::UtxoEntry`]-equivalent has no trailing
/// `covenant_id: Option<Hash>` field. The EOF-tolerance trick on the live
/// `UtxoEntry` does not compose inside a `HashMap`, so deserializing a
/// pre-Toccata `VirtualState` blob directly into the live type fails with
/// `Io(UnexpectedEof)`. The version-aware path in `CachedDbItem` dispatches
/// pre-Toccata rows through this shadow and converts via `From`.
///
/// All other fields are bincode-stable across the Toccata fork (none of
/// `GhostdagData`, `MuHash`, `BlockRewardData`, `BlockHashSet`, or
/// `Vec<Hash>` touch `UtxoEntry` / `UtxoDiff` / `Transaction`).
#[derive(Deserialize)]
#[cfg_attr(test, derive(serde::Serialize))]
struct PreToccataVirtualState {
    parents: Vec<Hash>,
    ghostdag_data: GhostdagData,
    daa_score: u64,
    bits: u32,
    past_median_time: u64,
    multiset: MuHash,
    utxo_diff: PreToccataUtxoDiff,
    accepted_id_digests: Vec<Hash>,
    mergeset_rewards: BlockHashMap<BlockRewardData>,
    mergeset_non_daa: BlockHashSet,
}

impl From<PreToccataVirtualState> for VirtualState {
    fn from(v: PreToccataVirtualState) -> Self {
        Self {
            parents: v.parents,
            ghostdag_data: v.ghostdag_data,
            daa_score: v.daa_score,
            bits: v.bits,
            past_median_time: v.past_median_time,
            multiset: v.multiset,
            utxo_diff: v.utxo_diff.into(),
            accepted_id_digests: v.accepted_id_digests,
            mergeset_rewards: v.mergeset_rewards,
            mergeset_non_daa: v.mergeset_non_daa,
        }
    }
}

impl From<PreToccataVirtualState> for Arc<VirtualState> {
    fn from(v: PreToccataVirtualState) -> Self {
        Arc::new(VirtualState::from(v))
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

/// A DB + cache implementation of `VirtualStateStore` trait.
///
/// Writes go under the post-Toccata versioned key layout
/// `[prefix || POST_TOCCATA_VIRTUAL_STATE_VERSION]`. Reads transparently
/// handle both layouts: when no versioned row is present the unversioned
/// `[prefix]` row is decoded via [`PreToccataVirtualState`] and converted
/// to `Arc<VirtualState>` with every nested UTXO entry's `covenant_id`
/// defaulting to `None`. See `kaspa_database::prelude::CachedDbItem` for
/// the full semantics.
#[derive(Clone)]
pub struct DbVirtualStateStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<VirtualState>, PreToccataVirtualState>,
    /// The "last known good" virtual state
    lkg_virtual_state: LkgVirtualState,
}

impl DbVirtualStateStore {
    pub fn new(db: Arc<DB>, lkg_virtual_state: LkgVirtualState) -> Self {
        let access = CachedDbItem::new_with_version_suffix(
            db.clone(),
            DatabaseStorePrefixes::VirtualState.into(),
            POST_TOCCATA_VIRTUAL_STATE_VERSION,
            // `None` ⇒ legacy rows live at the unversioned `[prefix]` key.
            // A future Toccata→post-Toccata-N migration would pass
            // `Some(POST_TOCCATA_VIRTUAL_STATE_VERSION)` and bump the
            // current-version constant.
            None,
        );
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

#[cfg(test)]
mod tests {
    //! End-to-end compat tests for `DbVirtualStateStore` across the Toccata
    //! version boundary. These drive a real RocksDB and exercise every
    //! layer: the constructor's initial LKG load, the version-aware
    //! `CachedDbItem`, the `PreToccataVirtualState` shadow decoder, and
    //! the row-level key layout.
    use super::*;
    use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint, UtxoEntry};
    use kaspa_consensus_core::utxo::pre_toccata::{PreToccataUtxoDiff, PreToccataUtxoEntry};
    use kaspa_consensus_core::utxo::utxo_diff::UtxoDiff;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::ConnBuilder;
    use kaspa_hashes::Hash;
    use std::collections::HashMap;

    fn legacy_row_key() -> Vec<u8> {
        DatabaseStorePrefixes::VirtualState.into()
    }

    fn versioned_row_key() -> Vec<u8> {
        let mut key: Vec<u8> = DatabaseStorePrefixes::VirtualState.into();
        key.push(POST_TOCCATA_VIRTUAL_STATE_VERSION);
        key
    }

    fn pre_toccata_entry(amount: u64, daa: u64, coinbase: bool, spk: &[u8]) -> PreToccataUtxoEntry {
        PreToccataUtxoEntry {
            amount,
            script_public_key: ScriptPublicKey::new(0, spk.iter().copied().collect()),
            block_daa_score: daa,
            is_coinbase: coinbase,
        }
    }

    fn outpoint(byte: u8, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_bytes([byte; 32]), index)
    }

    /// The regression test for the user-reported panic. Plants a pre-Toccata
    /// `VirtualState` bincode blob at `[VirtualState prefix]`, then
    /// constructs `DbVirtualStateStore` and asserts the LKG state decodes
    /// correctly through `PreToccataVirtualState`. Before this fix, the
    /// constructor unwrapped a `DeserializationError(Io(UnexpectedEof))`
    /// because the post-Toccata `UtxoEntry` visitor's EOF→None trick does
    /// not compose inside `UtxoDiff`'s `HashMap`s.
    #[test]
    fn legacy_virtual_state_decodes_via_shadow() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut add = HashMap::new();
        add.insert(outpoint(0x11, 0), pre_toccata_entry(0x0123_4567_89ab_cdef, 42, true, &[0x76, 0xa9, 0x14]));
        add.insert(outpoint(0x33, 4), pre_toccata_entry(500, 100, false, &[0xaa, 0xbb]));
        let mut remove = HashMap::new();
        remove.insert(outpoint(0x22, 7), pre_toccata_entry(2_000_000, 99, false, &[0x51, 0x52]));
        remove.insert(outpoint(0x44, 11), pre_toccata_entry(777, 12, true, &[0xde, 0xad, 0xbe, 0xef]));
        let pre_diff = PreToccataUtxoDiff { add, remove };

        let pre_state = PreToccataVirtualState {
            parents: vec![Hash::from_bytes([0x77; 32])],
            ghostdag_data: GhostdagData::default(),
            daa_score: 999,
            bits: 0x1f00ffff,
            past_median_time: 1_700_000_000,
            multiset: MuHash::new(),
            utxo_diff: pre_diff,
            accepted_id_digests: vec![Hash::from_bytes([0x88; 32])],
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::default(),
        };

        // Plant raw pre-Toccata bytes at the unversioned key.
        db.put(legacy_row_key(), bincode::serialize(&pre_state).unwrap()).unwrap();

        // Constructing the store used to panic here on a pre-Toccata DB.
        let lkg = LkgVirtualState::default();
        let _store = DbVirtualStateStore::new(db.clone(), lkg.clone());

        let loaded = lkg.load_full();
        assert_eq!(loaded.daa_score, 999);
        assert_eq!(loaded.bits, 0x1f00ffff);
        assert_eq!(loaded.past_median_time, 1_700_000_000);
        assert_eq!(loaded.utxo_diff.add.len(), 2);
        assert_eq!(loaded.utxo_diff.remove.len(), 2);
        for entry in loaded.utxo_diff.add.values().chain(loaded.utxo_diff.remove.values()) {
            assert_eq!(entry.covenant_id, None, "every legacy entry must come back with covenant_id == None");
        }

        // The constructor's initial read migrated the row: the legacy key
        // is gone and the versioned key now holds the converted bytes.
        assert!(db.get_pinned(legacy_row_key()).unwrap().is_none(), "legacy row must be cleared after migration");
        assert!(db.get_pinned(versioned_row_key()).unwrap().is_some(), "versioned row must exist after migration");
    }

    /// A post-Toccata `set` through the store lands under the versioned
    /// `[prefix || 1]` layout and round-trips via `get`.
    #[test]
    fn post_toccata_set_lands_at_versioned_key() {
        let (_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let lkg = LkgVirtualState::default();
        let mut store = DbVirtualStateStore::new(db.clone(), lkg);

        let mut add = HashMap::new();
        add.insert(
            outpoint(0x77, 3),
            UtxoEntry::new(
                1_234_567,
                ScriptPublicKey::new(0, [0xa1, 0xa2, 0xa3].as_slice().iter().copied().collect()),
                88,
                false,
                Some(Hash::from_bytes([0x5a; 32])),
            ),
        );
        let diff = UtxoDiff { add, remove: Default::default() };

        let state = Arc::new(VirtualState {
            parents: vec![Hash::from_bytes([0x99; 32])],
            ghostdag_data: GhostdagData::default(),
            daa_score: 1234,
            bits: 0x1e00ffff,
            past_median_time: 1_710_000_000,
            multiset: MuHash::new(),
            utxo_diff: diff,
            accepted_id_digests: vec![Hash::from_bytes([0x10; 32])],
            mergeset_rewards: BlockHashMap::new(),
            mergeset_non_daa: BlockHashSet::default(),
        });

        store.set(state.clone()).unwrap();

        // The row lives under the versioned key, NOT the legacy one.
        assert!(db.get_pinned(versioned_row_key()).unwrap().is_some());
        assert!(db.get_pinned(legacy_row_key()).unwrap().is_none());

        // Round-trip via a fresh store (forces a DB hit, bypassing any cache).
        let fresh = DbVirtualStateStore::new(db, LkgVirtualState::default());
        let round = fresh.get().unwrap();
        assert_eq!(round.daa_score, state.daa_score);
        assert_eq!(round.bits, state.bits);
        assert_eq!(round.utxo_diff.add.len(), 1);
        let entry = round.utxo_diff.add.values().next().unwrap();
        assert_eq!(entry.covenant_id, Some(Hash::from_bytes([0x5a; 32])));
    }
}
