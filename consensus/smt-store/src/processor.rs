//! `SmtProcessor` — two-phase SMT lane processing.
//!
//! **Phase 1 — Accumulation** (`update_lane` / `expire_lane`):
//! Collects lane updates and expirations into a [`LaneChanges`] collection.
//!
//! **Phase 2 — Build & Persist** (`build` then `flush`):
//! `build()` derives leaf hashes, calls [`compute_root_update`] against an
//! immutable DB reader, and returns an [`SmtBuild`] with the root and changed
//! branches. `flush()` persists to a `WriteBatch`; the caller commits atomically.
//!
//! The processor is generic over `C: LaneChanges`:
//! - [`BlockLaneChanges`] (default) — single-block processing, all lanes share one blue_score.
//! - [`ImportLaneChanges`] — IBD import, each lane carries its own blue_score.

use std::collections::BTreeMap;
use std::sync::Arc;

use parking_lot::Mutex;

use kaspa_database::prelude::{BatchDbWriter, DB, StoreError, StoreResult};
use kaspa_hashes::{Hash, SeqCommitActiveNode, ZERO_HASH};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::SmtHasher;
use kaspa_smt::store::{BranchKey, Node, SmtStore, SortedLeafUpdates};
use kaspa_smt::tree::{SmtNodeChanges, compute_root_update};
use rocksdb::WriteBatch;

use crate::branch_version_store::DbBranchVersionStore;
use crate::cache::{BranchEntity, BranchVersionCache, LaneVersionCache};
use crate::lane_version_store::DbLaneVersionStore;
use crate::maybe_fork::Verified;
use crate::score_index::DbScoreIndex;
use crate::values::LaneTipHash;
use crate::{BlockHash, LaneKey};

struct VersionedBranchReader<'a, F: Fn(Hash) -> bool> {
    stores: &'a SmtStores,
    min_blue_score: u64,
    is_canonical: F,
}

impl<F: Fn(Hash) -> bool> SmtStore for VersionedBranchReader<'_, F> {
    type Error = StoreError;

    fn get_node(&self, key: &BranchKey) -> Result<Option<Node>, StoreError> {
        let entity = BranchEntity { depth: key.depth, node_key: key.node_key };
        Ok(self.stores.get_node(entity, self.min_blue_score, |bh| (self.is_canonical)(bh)).and_then(|v| *v.data()))
    }
}

/// All versioned SMT DB stores with in-memory caches.
pub struct SmtStores {
    pub branch_version: DbBranchVersionStore,
    pub lane_version: DbLaneVersionStore,
    pub score_index: DbScoreIndex,
    branch_cache: Mutex<BranchVersionCache>,
    lane_cache: Mutex<LaneVersionCache>,
}

impl SmtStores {
    pub fn new(db: Arc<DB>, branch_cache_capacity: usize, lane_cache_capacity: usize) -> Self {
        Self {
            branch_version: DbBranchVersionStore::new(db.clone()),
            lane_version: DbLaneVersionStore::new(db.clone()),
            score_index: DbScoreIndex::new(db),
            branch_cache: Mutex::new(BranchVersionCache::new(branch_cache_capacity)),
            lane_cache: Mutex::new(LaneVersionCache::new(lane_cache_capacity)),
        }
    }

    /// Find the latest canonical node version, checking cache first then DB.
    pub fn get_node(
        &self,
        entity: BranchEntity,
        min_blue_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<Option<Node>>> {
        if let Some((score, block_hash, value)) = self.branch_cache.lock().get(entity, u64::MAX, min_blue_score, &mut is_canonical) {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.branch_version.get(entity.depth, entity.node_key, min_blue_score, is_canonical).unwrap()
    }

    /// Find the latest canonical lane version, checking cache first then DB.
    pub fn get_lane(
        &self,
        lane_key: LaneKey,
        min_blue_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<LaneTipHash>> {
        if let Some((score, block_hash, value)) = self.lane_cache.lock().get(lane_key, u64::MAX, min_blue_score, &mut is_canonical) {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.lane_version.get(lane_key, min_blue_score, is_canonical).unwrap()
    }

    /// Read the lanes root hash from the branch store at depth=0.
    /// Returns the empty root if no root node exists.
    pub fn get_lanes_root(&self, min_blue_score: u64, is_canonical: impl FnMut(Hash) -> bool) -> Hash {
        let root_entity = BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) };
        match self.get_node(root_entity, min_blue_score, is_canonical) {
            Some(v) => match *v.data() {
                Some(Node::Internal(hash)) => hash,
                Some(Node::Collapsed(cl)) => {
                    kaspa_smt::hash_node::<kaspa_hashes::SeqCommitActiveCollapsedNode>(cl.lane_key, cl.leaf_hash)
                }
                None => kaspa_hashes::SeqCommitActiveNode::empty_root(),
            },
            None => kaspa_hashes::SeqCommitActiveNode::empty_root(),
        }
    }

    pub fn evict_caches_below_score(&self, min_score: u64) {
        self.branch_cache.lock().evict_below_score(min_score);
        self.lane_cache.lock().evict_below_score(min_score);
    }

    /// Clear all versioned SMT stores and caches. Used before IBD SMT sync.
    pub fn clear_all(&self) {
        self.branch_version.delete_all();
        self.lane_version.delete_all();
        self.score_index.delete_all();
        self.branch_cache.lock().clear();
        self.lane_cache.lock().clear();
    }
}

/// Abstraction over lane change collections.
///
/// `LaneMeta` is per-lane metadata: `()` for block processing (blue_score
/// is uniform), `u64` for IBD import (each lane has its own blue_score).
pub trait LaneChanges {
    type LaneMeta;

    fn update(&mut self, lane_key: LaneKey, lane_tip_hash: Hash, meta: Self::LaneMeta);
    fn to_leaf_updates(&self) -> SortedLeafUpdates;
    fn flush_lanes(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()>;
    fn flush_score_index(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()>;
    fn is_empty(&self) -> bool;
    fn len(&self) -> usize;
}

/// Lane changes within a single block. All lanes share the block's blue_score.
pub struct BlockLaneChanges {
    blue_score: u64,
    changes: BTreeMap<LaneKey, Option<LaneTipHash>>,
}

impl BlockLaneChanges {
    pub fn new(blue_score: u64) -> Self {
        Self { blue_score, changes: BTreeMap::new() }
    }

    pub fn expire(&mut self, lane_key: LaneKey) {
        self.changes.insert(lane_key, None);
    }
}

impl LaneChanges for BlockLaneChanges {
    type LaneMeta = ();

    fn update(&mut self, lane_key: LaneKey, lane_tip_hash: Hash, _extra: ()) {
        self.changes.insert(lane_key, Some(lane_tip_hash));
    }

    fn to_leaf_updates(&self) -> SortedLeafUpdates {
        let bs = self.blue_score;
        SortedLeafUpdates::from_sorted_map(&self.changes, |key, change| match change {
            Some(tip) => smt_leaf_hash(&SmtLeafInput { lane_key: key, lane_tip: tip, blue_score: bs }),
            None => ZERO_HASH,
        })
    }

    fn flush_lanes(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
        for (lane_key, tip) in &self.changes {
            if let Some(tip) = tip {
                stores.lane_version.put(BatchDbWriter::new(batch), *lane_key, self.blue_score, block_hash, tip)?;
            }
        }
        let mut lc = stores.lane_cache.lock();
        for (lane_key, tip) in &self.changes {
            if let Some(tip) = tip {
                lc.insert(*lane_key, self.blue_score, block_hash, *tip);
            }
        }
        Ok(())
    }

    fn flush_score_index(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
        use crate::keys::ScoreIndexKind;
        let updated: Vec<LaneKey> = self.changes.iter().filter_map(|(k, v)| v.as_ref().map(|_| *k)).collect();
        let expired: Vec<LaneKey> = self.changes.iter().filter_map(|(k, v)| if v.is_none() { Some(*k) } else { None }).collect();
        if !updated.is_empty() {
            stores.score_index.put(BatchDbWriter::new(batch), self.blue_score, ScoreIndexKind::LeafUpdate, block_hash, &updated)?;
        }
        if !expired.is_empty() {
            stores.score_index.put(BatchDbWriter::new(batch), self.blue_score, ScoreIndexKind::Structural, block_hash, &expired)?;
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Lane imports with per-lane blue_score. No expirations.
#[derive(Default)]
pub struct ImportLaneChanges {
    changes: BTreeMap<LaneKey, (LaneTipHash, u64)>,
}

impl ImportLaneChanges {
    pub fn new() -> Self {
        Self::default()
    }
}

impl LaneChanges for ImportLaneChanges {
    type LaneMeta = u64;

    fn update(&mut self, lane_key: LaneKey, lane_tip_hash: Hash, blue_score: u64) {
        self.changes.insert(lane_key, (lane_tip_hash, blue_score));
    }

    fn to_leaf_updates(&self) -> SortedLeafUpdates {
        SortedLeafUpdates::from_sorted_map(&self.changes, |key, (tip, bs)| {
            smt_leaf_hash(&SmtLeafInput { lane_key: key, lane_tip: tip, blue_score: *bs })
        })
    }

    fn flush_lanes(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
        for (lane_key, (tip, bs)) in &self.changes {
            stores.lane_version.put(BatchDbWriter::new(batch), *lane_key, *bs, block_hash, tip)?;
        }
        let mut lc = stores.lane_cache.lock();
        for (lane_key, (tip, bs)) in &self.changes {
            lc.insert(*lane_key, *bs, block_hash, *tip);
        }
        Ok(())
    }

    fn flush_score_index(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
        use crate::keys::ScoreIndexKind;
        let mut groups: BTreeMap<u64, Vec<LaneKey>> = BTreeMap::new();
        for (lane_key, (_, bs)) in &self.changes {
            groups.entry(*bs).or_default().push(*lane_key);
        }
        for (bs, keys) in &groups {
            stores.score_index.put(BatchDbWriter::new(batch), *bs, ScoreIndexKind::LeafUpdate, block_hash, keys)?;
        }
        Ok(())
    }

    fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Accumulates SMT lane changes and builds the tree.
pub struct SmtProcessor<'a, C: LaneChanges = BlockLaneChanges> {
    stores: &'a SmtStores,
    blue_score: u64,
    inactivity_threshold: u64,
    current_lanes_root: Hash,
    lane_changes: C,
}

impl<'a> SmtProcessor<'a, BlockLaneChanges> {
    pub fn new(stores: &'a SmtStores, blue_score: u64, inactivity_threshold: u64, current_lanes_root: Hash) -> Self {
        Self { stores, blue_score, inactivity_threshold, current_lanes_root, lane_changes: BlockLaneChanges::new(blue_score) }
    }

    pub fn update_lane(&mut self, lane_key: LaneKey, lane_tip_hash: Hash) {
        self.lane_changes.update(lane_key, lane_tip_hash, ());
    }

    pub fn expire_lane(&mut self, lane_key: LaneKey) {
        self.lane_changes.expire(lane_key);
    }
}

impl<'a> SmtProcessor<'a, ImportLaneChanges> {
    pub fn new_import(stores: &'a SmtStores, blue_score: u64, inactivity_threshold: u64, current_lanes_root: Hash) -> Self {
        Self { stores, blue_score, inactivity_threshold, current_lanes_root, lane_changes: ImportLaneChanges::new() }
    }

    pub fn update_lane(&mut self, lane_key: LaneKey, lane_tip_hash: Hash, blue_score: u64) {
        self.lane_changes.update(lane_key, lane_tip_hash, blue_score);
    }
}

impl<'a, C: LaneChanges> SmtProcessor<'a, C> {
    pub fn build(self, is_canonical: impl Fn(Hash) -> bool) -> StoreResult<SmtBuild<C>> {
        if self.lane_changes.is_empty() {
            return Ok(SmtBuild {
                root: self.current_lanes_root,
                node_changes: SmtNodeChanges::new(),
                lane_changes: self.lane_changes,
                payload_and_ctx_digest: ZERO_HASH,
                active_lanes_count: 0,
            });
        }

        let leaf_updates = self.lane_changes.to_leaf_updates();
        let reader = VersionedBranchReader {
            stores: self.stores,
            min_blue_score: self.blue_score.saturating_sub(self.inactivity_threshold),
            is_canonical,
        };
        let (root, node_changes) = compute_root_update::<SeqCommitActiveNode, _>(&reader, self.current_lanes_root, leaf_updates)?;
        Ok(SmtBuild { root, node_changes, lane_changes: self.lane_changes, payload_and_ctx_digest: ZERO_HASH, active_lanes_count: 0 })
    }
}

/// Result of building an SMT: root hash + changed nodes + lane changes + metadata.
pub struct SmtBuild<C: LaneChanges = BlockLaneChanges> {
    pub root: Hash,
    node_changes: SmtNodeChanges,
    lane_changes: C,
    /// Set by `build_seq_commit` after computing the seq_commit components.
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
}

impl<C: LaneChanges> SmtBuild<C> {
    pub fn lane_update_count(&self) -> usize {
        self.lane_changes.len()
    }

    pub fn diff_branch_count(&self) -> usize {
        self.node_changes.len()
    }

    /// Persist the build's node/lane/score-index diff to a `WriteBatch` and populate caches.
    ///
    /// `branch_blue_score` versions the nodes. Lane and score_index
    /// blue_scores are determined by the `LaneChanges` implementation.
    /// Metadata is written separately by the caller via `DbSmtMetadataStore`.
    pub fn flush(
        self,
        stores: &SmtStores,
        batch: &mut WriteBatch,
        branch_blue_score: u64,
        block_hash: BlockHash,
    ) -> StoreResult<Hash> {
        let root = self.root;

        for (bk, node) in &self.node_changes {
            stores.branch_version.put(BatchDbWriter::new(batch), bk.depth, bk.node_key, branch_blue_score, block_hash, *node)?;
        }
        {
            let mut bc = stores.branch_cache.lock();
            for (bk, node) in &self.node_changes {
                bc.insert(BranchEntity { depth: bk.depth, node_key: bk.node_key }, branch_blue_score, block_hash, *node);
            }
        }

        self.lane_changes.flush_lanes(stores, batch, block_hash)?;
        self.lane_changes.flush_score_index(stores, batch, block_hash)?;

        Ok(root)
    }
}
