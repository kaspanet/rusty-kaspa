//! `SmtProcessor` — two-phase SMT lane processing for a single block.
//!
//! # Design
//!
//! **Phase 1 — Accumulation** (`update_lane` / `expire_lane`):
//! Collects lane updates and expirations. No tree computation occurs.
//!
//! **Phase 2 — Build & Persist** (`build` then `flush`):
//! `build()` derives leaf hashes from accumulated lane changes, calls
//! [`compute_root_update`] against an immutable DB reader, and returns
//! an [`SmtBuild`] containing the root and only the changed branches.
//! `flush()` persists the diff to a `WriteBatch`. The caller commits
//! atomically via `db.write(batch)`.

use std::collections::BTreeMap;
use std::sync::Arc;

use kaspa_database::prelude::{BatchDbWriter, DB, StoreError, StoreResult};
use kaspa_hashes::{Hash, SeqCommitActiveNode, ZERO_HASH};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::store::{BranchChildren, BranchKey, SmtStore, SortedLeafUpdates};
use kaspa_smt::tree::{SmtBranchChanges, compute_root_update};
use rocksdb::WriteBatch;

use crate::branch_version_store::DbBranchVersionStore;
use crate::lane_version_store::DbLaneVersionStore;
use crate::score_index::DbScoreIndex;
use crate::values::LaneVersion;
use crate::{BlockHash, LANE_INACTIVITY_THRESHOLD, LaneKey};

// ---------------------------------------------------------------------------
// VersionedBranchReader — read-only SmtStore impl over versioned DB
// ---------------------------------------------------------------------------

/// Reads branches from the versioned DB with canonicality + inactivity filtering.
///
/// Implements [`SmtStore`] (read-only) for use with [`compute_root_update`].
struct VersionedBranchReader<'a, F: Fn(Hash) -> bool> {
    store: &'a DbBranchVersionStore,
    min_blue_score: u64,
    is_canonical: F,
}

impl<F: Fn(Hash) -> bool> SmtStore for VersionedBranchReader<'_, F> {
    type Error = StoreError;

    fn get_branch(&self, key: &BranchKey) -> Result<Option<BranchChildren>, StoreError> {
        let version = self.store.get(key.height, key.node_key, self.min_blue_score, |bh| (self.is_canonical)(bh))?;
        Ok(version.map(|v| *v.data()))
    }
}

// ---------------------------------------------------------------------------
// SmtStores — bundled DB stores
// ---------------------------------------------------------------------------

/// All versioned SMT DB stores, bundled for convenience.
///
/// Created once during consensus init and shared across block processing.
pub struct SmtStores {
    pub branch_version: DbBranchVersionStore,
    pub lane_version: DbLaneVersionStore,
    pub score_index: DbScoreIndex,
}

impl SmtStores {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            branch_version: DbBranchVersionStore::new(db.clone()),
            lane_version: DbLaneVersionStore::new(db.clone()),
            score_index: DbScoreIndex::new(db),
        }
    }
}

// ---------------------------------------------------------------------------
// SmtProcessor — accumulate lane changes, build, flush
// ---------------------------------------------------------------------------

/// Accumulates SMT lane updates for a single block.
///
/// # Usage
///
/// ```ignore
/// let mut proc = SmtProcessor::new(&stores, blue_score, parent_lanes_root);
/// proc.update_lane(key_a, lane_id_a, tip_a);
/// proc.expire_lane(key_expired);
/// let build = proc.build(|bh| reachability.is_chain_ancestor_of(bh, tip))?;
/// let root = build.root;
/// build.flush(&stores, &mut batch, blue_score, block_hash)?;
/// db.write(batch)?;
/// ```
pub struct SmtProcessor<'a> {
    stores: &'a SmtStores,
    blue_score: u64,
    current_lanes_root: Hash,
    /// Lane changes: key = lane_key, value = Some(version) for update, None for expiration.
    lane_changes: BTreeMap<LaneKey, Option<LaneVersion>>,
}

impl<'a> SmtProcessor<'a> {
    pub fn new(stores: &'a SmtStores, blue_score: u64, current_lanes_root: Hash) -> Self {
        Self { stores, blue_score, current_lanes_root, lane_changes: BTreeMap::new() }
    }

    /// Accumulate a lane update (new or existing lane with activity).
    pub fn update_lane(&mut self, lane_key: LaneKey, lane_id: [u8; 20], lane_tip_hash: Hash) {
        self.lane_changes.insert(lane_key, Some(LaneVersion { lane_id, lane_tip_hash }));
    }

    /// Mark a lane as expired (ZERO_HASH leaf). Does not create a lane
    /// version entry — the lane simply disappears from the tree.
    pub fn expire_lane(&mut self, lane_key: LaneKey) {
        self.lane_changes.insert(lane_key, None);
    }

    /// Build the SMT: derive leaf hashes from lane changes, compute root
    /// against immutable DB, return only changed branches.
    ///
    /// The `is_canonical` closure filters fork entries in the versioned store.
    pub fn build(self, is_canonical: impl Fn(Hash) -> bool) -> StoreResult<SmtBuild> {
        // No lanes touched or expired: skip build entirely, reuse parent root
        if self.lane_changes.is_empty() {
            return Ok(SmtBuild {
                root: self.current_lanes_root,
                branch_changes: SmtBranchChanges::new(),
                lane_changes: self.lane_changes,
            });
        }

        // Derive leaf updates from lane changes (BTreeMap guarantees sorted + unique keys)
        let blue_score = self.blue_score;
        let leaf_updates = SortedLeafUpdates::from_sorted_map(&self.lane_changes, |_key, change| match change {
            Some(v) => smt_leaf_hash(&SmtLeafInput { lane_id: &v.lane_id, lane_tip: &v.lane_tip_hash, blue_score }),
            None => ZERO_HASH,
        });

        // Pure computation: reads from immutable DB, returns only changed branches.
        let reader = VersionedBranchReader {
            store: &self.stores.branch_version,
            min_blue_score: self.blue_score.saturating_sub(LANE_INACTIVITY_THRESHOLD),
            is_canonical,
        };
        let (root, branch_changes) = compute_root_update::<SeqCommitActiveNode, _>(&reader, self.current_lanes_root, leaf_updates)?;

        Ok(SmtBuild { root, branch_changes, lane_changes: self.lane_changes })
    }
}

// ---------------------------------------------------------------------------
// SmtBuild — result of build, ready for persistence
// ---------------------------------------------------------------------------

/// Result of building an SMT: root hash + only changed branches, ready for persistence.
pub struct SmtBuild {
    /// The computed SMT root after all updates.
    pub root: Hash,
    /// Only the branches that actually changed (unchanged branches are NOT included).
    branch_changes: SmtBranchChanges,
    /// Lane changes: key = lane_key, value = Some(version) for update, None for expiration.
    lane_changes: BTreeMap<LaneKey, Option<LaneVersion>>,
}

impl SmtBuild {
    /// Number of branch entries that changed.
    pub fn diff_branch_count(&self) -> usize {
        self.branch_changes.len()
    }

    /// Persist the build's diff to a `WriteBatch`.
    ///
    /// Writes only changed branch versions, active lane versions, and score index.
    /// Expired lanes are NOT recorded in the score index (only branch changes are persisted).
    /// The caller commits atomically via `db.write(batch)`.
    pub fn flush(self, stores: &SmtStores, batch: &mut WriteBatch, blue_score: u64, block_hash: BlockHash) -> StoreResult<Hash> {
        let root = self.root;

        // Branch versions — only changed branches
        for (bk, bc) in &self.branch_changes {
            stores.branch_version.put(BatchDbWriter::new(batch), bk.height, bk.node_key, blue_score, block_hash, bc)?;
        }

        // Lane versions (only for updates, not expirations)
        for (lane_key, version) in &self.lane_changes {
            if let Some(v) = version {
                stores.lane_version.put(BatchDbWriter::new(batch), *lane_key, blue_score, block_hash, v)?;
            }
        }

        // Score index — only active lane updates (not expirations).
        // Expired lanes don't need index entries because:
        // - Branch changes from expirations are in branch_version_store directly
        // - The score index serves expire_stale_lanes which only needs active touches
        let active_keys: Vec<LaneKey> = self.lane_changes.iter().filter_map(|(k, v)| v.as_ref().map(|_| *k)).collect();
        if !active_keys.is_empty() {
            stores.score_index.put(BatchDbWriter::new(batch), blue_score, block_hash, &active_keys)?;
        }

        Ok(root)
    }
}
