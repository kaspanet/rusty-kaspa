//! `SmtProcessor` — two-phase SMT lane processing.
//!
//! **Phase 1 — Accumulation** (`update_lane` / `expire_lane`):
//! Collects lane updates and expirations into [`BlockLaneChanges`].
//!
//! **Phase 2 — Build & Persist** (`build` then `flush`):
//! `build()` derives leaf hashes, calls [`compute_root_update`] against an
//! immutable DB reader, and returns an [`SmtBuild`] with the root and changed
//! branches. `flush()` persists to a `WriteBatch`; the caller commits atomically.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use parking_lot::Mutex;

use kaspa_database::prelude::{BatchDbWriter, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_hashes::{Hash, SeqCommitActiveNode, ZERO_HASH};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::SmtHasher;
use kaspa_smt::proof::OwnedSmtProof;
use kaspa_smt::store::{BranchKey, Node, SmtStore, SortedLeafUpdates};
use kaspa_smt::tree::{SmtNodeChanges, SparseMerkleTree, compute_root_update};
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
    target_blue_score: u64,
    min_blue_score: u64,
    is_canonical: F,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmtReadBounds {
    pub target_blue_score: u64,
    pub min_blue_score: u64,
}

impl SmtReadBounds {
    pub const fn new(target_blue_score: u64, min_blue_score: u64) -> Self {
        Self { target_blue_score, min_blue_score }
    }

    pub const fn for_pov(pov_blue_score: u64, inactivity_threshold: u64) -> Self {
        Self { target_blue_score: pov_blue_score, min_blue_score: pov_blue_score.saturating_sub(inactivity_threshold) }
    }
}

impl<F: Fn(Hash) -> bool> SmtStore for VersionedBranchReader<'_, F> {
    type Error = StoreError;

    fn get_node(&self, key: &BranchKey) -> Result<Option<Node>, StoreError> {
        let entity = BranchEntity { depth: key.depth, node_key: key.node_key };
        Ok(self
            .stores
            .get_node(entity, self.target_blue_score, self.min_blue_score, |bh| (self.is_canonical)(bh))
            .and_then(|v| *v.data()))
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

struct PruneEntry {
    lane_key: Hash,
    blue_score: u64,
    block_hash: Hash,
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

    /// Find the latest canonical node version in `[min_blue_score, target_blue_score]`,
    /// checking cache first then DB. `target_blue_score` is the block at which the
    /// read is happening — it drives `get_at`'s seek so non-canonical future
    /// versions are skipped in O(log n) rather than scanned linearly.
    pub fn get_node(
        &self,
        entity: BranchEntity,
        target_blue_score: u64,
        min_blue_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<Option<Node>>> {
        if let Some((score, block_hash, value)) =
            self.branch_cache.lock().get(entity, target_blue_score, min_blue_score, &mut is_canonical)
        {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.branch_version.get_at_canonical(entity.depth, entity.node_key, target_blue_score, min_blue_score, is_canonical).unwrap()
    }

    /// Find the latest canonical lane version in `[min_blue_score, target_blue_score]`,
    /// checking cache first then DB.
    pub fn get_lane(
        &self,
        lane_key: LaneKey,
        target_blue_score: u64,
        min_blue_score: u64,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<LaneTipHash>> {
        if let Some((score, block_hash, value)) =
            self.lane_cache.lock().get(lane_key, target_blue_score, min_blue_score, &mut is_canonical)
        {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.lane_version.get_at_canonical(lane_key, target_blue_score, min_blue_score, is_canonical).unwrap()
    }

    /// Read the lanes root hash from the branch store at depth=0.
    /// Returns the empty root if no root node exists.
    pub fn get_lanes_root(&self, target_blue_score: u64, min_blue_score: u64, is_canonical: impl FnMut(Hash) -> bool) -> Hash {
        let root_entity = BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) };
        match self.get_node(root_entity, target_blue_score, min_blue_score, is_canonical) {
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

    /// Generate an inclusion proof for `lane_key` in the canonical tree as of
    /// `target_blue_score`.
    pub fn prove_lane(
        &self,
        lane_key: &Hash,
        target_blue_score: u64,
        min_blue_score: u64,
        is_canonical: impl Fn(Hash) -> bool,
    ) -> StoreResult<OwnedSmtProof> {
        let reader = VersionedBranchReader { stores: self, target_blue_score, min_blue_score, is_canonical };
        // Root value is unused by `prove` — it walks the store directly.
        let tree = SparseMerkleTree::<SeqCommitActiveNode, _>::with_store(reader);
        tree.prove(lane_key)
    }

    pub fn evict_caches_below_score(&self, min_score: u64) {
        self.branch_cache.lock().evict_below_score(min_score);
        self.lane_cache.lock().evict_below_score(min_score);
    }

    /// Prune all lane-version, branch-version, and score-index entries whose
    /// blue_score is at or below `cutoff_blue_score`.
    ///
    /// The score index is the discovery mechanism: it records which lane_keys
    /// were touched at each `(blue_score, block_hash)` pair (both `LeafUpdate`
    /// and `Structural` kinds). Since the score index already provides the full
    /// `(lane_key, blue_score, block_hash)` triple, we construct delete keys
    /// directly — no reads from lane_version or branch_version are needed.
    ///
    /// Work is batched into chunks of score-index entries to bound
    /// `WriteBatch` memory. After all chunks, the score index itself is
    /// range-deleted and caches are evicted.
    pub fn prune(&self, db: &DB, cutoff_blue_score: u64) {
        // Number of score-index entries to accumulate before flushing a WriteBatch.
        const CHUNK_ENTRIES: usize = 1024;

        let mut entries: Vec<PruneEntry> = Vec::new();
        let mut entries_in_chunk = 0usize;
        let mut total_lane_deletes = 0u64;
        let mut total_branch_deletes = 0u64;
        let mut chunks_written = 0u64;

        // Iterate both LeafUpdate and Structural entries at scores ≤ cutoff
        for entry in self.score_index.get_all(0..=cutoff_blue_score) {
            let entry = entry.unwrap();
            let blue_score = entry.blue_score();
            let block_hash = entry.block_hash();
            for lk in entry.data().iter() {
                entries.push(PruneEntry { lane_key: *lk, blue_score, block_hash });
            }
            entries_in_chunk += 1;

            if entries_in_chunk >= CHUNK_ENTRIES {
                let (ld, bd) = self.prune_chunk(db, &entries);
                total_lane_deletes += ld;
                total_branch_deletes += bd;
                chunks_written += 1;
                entries.clear();
                entries_in_chunk = 0;
            }
        }

        // Flush remaining entries
        if !entries.is_empty() {
            let (ld, bd) = self.prune_chunk(db, &entries);
            total_lane_deletes += ld;
            total_branch_deletes += bd;
            chunks_written += 1;
        }

        // Range-delete score-index entries at scores ≤ cutoff (single tombstone)
        self.score_index.delete_range(DirectDbWriter::new(db), cutoff_blue_score).unwrap();
        self.evict_caches_below_score(cutoff_blue_score);

        log::info!(
            "SMT pruning complete: {} chunks, {} lane version deletes, {} branch version deletes (cutoff={})",
            chunks_written,
            total_lane_deletes,
            total_branch_deletes,
            cutoff_blue_score
        );
    }

    /// Delete lane-version and branch-version entries directly from known keys,
    /// writing all deletes into a single `WriteBatch`. No DB reads required —
    /// keys are constructed from the score-index data.
    fn prune_chunk(&self, db: &DB, entries: &[PruneEntry]) -> (u64, u64) {
        let mut batch = WriteBatch::default();

        // Delete lane-version entries directly
        let lane_deletes = entries.len() as u64;
        for e in entries {
            self.lane_version.delete(BatchDbWriter::new(&mut batch), e.lane_key, e.blue_score, e.block_hash).unwrap();
        }

        // Derive branch keys at all 256 depths from each entry. BTreeSet
        // deduplicates: at low depths many lane_keys map to the same node_key
        // (e.g. depth 0 always maps to ZERO_HASH).
        let mut branch_keys: BTreeSet<(BranchKey, u64, Hash)> = BTreeSet::new();
        for e in entries {
            for depth in 0..=255u8 {
                branch_keys.insert((BranchKey::new(depth, &e.lane_key), e.blue_score, e.block_hash));
            }
        }

        let branch_deletes = branch_keys.len() as u64;
        for (bk, blue_score, block_hash) in &branch_keys {
            self.branch_version.delete(BatchDbWriter::new(&mut batch), bk.depth, bk.node_key, *blue_score, *block_hash).unwrap();
        }

        db.write(batch).unwrap();
        (lane_deletes, branch_deletes)
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

    pub fn update(&mut self, lane_key: LaneKey, lane_tip_hash: Hash) {
        self.changes.insert(lane_key, Some(lane_tip_hash));
    }

    pub fn to_leaf_updates(&self) -> SortedLeafUpdates {
        let bs = self.blue_score;
        SortedLeafUpdates::from_sorted_map(&self.changes, |key, change| match change {
            Some(tip) => smt_leaf_hash(&SmtLeafInput { lane_key: key, lane_tip: tip, blue_score: bs }),
            None => ZERO_HASH,
        })
    }

    pub fn flush_lanes(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
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

    pub fn flush_score_index(&self, stores: &SmtStores, batch: &mut WriteBatch, block_hash: BlockHash) -> StoreResult<()> {
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

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn len(&self) -> usize {
        self.changes.len()
    }
}

/// Accumulates SMT lane changes and builds the tree.
pub struct SmtProcessor<'a> {
    stores: &'a SmtStores,
    read_bounds: SmtReadBounds,
    current_lanes_root: Hash,
    lane_changes: BlockLaneChanges,
}

impl<'a> SmtProcessor<'a> {
    pub fn new(stores: &'a SmtStores, write_blue_score: u64, read_bounds: SmtReadBounds, current_lanes_root: Hash) -> Self {
        Self { stores, read_bounds, current_lanes_root, lane_changes: BlockLaneChanges::new(write_blue_score) }
    }

    pub fn update_lane(&mut self, lane_key: LaneKey, lane_tip_hash: Hash) {
        self.lane_changes.update(lane_key, lane_tip_hash);
    }

    pub fn expire_lane(&mut self, lane_key: LaneKey) {
        self.lane_changes.expire(lane_key);
    }

    pub fn build(self, is_canonical: impl Fn(Hash) -> bool) -> StoreResult<SmtBuild> {
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
            target_blue_score: self.read_bounds.target_blue_score,
            min_blue_score: self.read_bounds.min_blue_score,
            is_canonical,
        };
        let (root, node_changes) = compute_root_update::<SeqCommitActiveNode, _>(&reader, self.current_lanes_root, leaf_updates)?;
        Ok(SmtBuild { root, node_changes, lane_changes: self.lane_changes, payload_and_ctx_digest: ZERO_HASH, active_lanes_count: 0 })
    }
}

/// Result of building an SMT: root hash + changed nodes + lane changes + metadata.
pub struct SmtBuild {
    pub root: Hash,
    node_changes: SmtNodeChanges,
    lane_changes: BlockLaneChanges,
    /// Set by `build_seq_commit` after computing the seq_commit components.
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
}

impl SmtBuild {
    pub fn lane_update_count(&self) -> usize {
        self.lane_changes.len()
    }

    pub fn diff_branch_count(&self) -> usize {
        self.node_changes.len()
    }

    /// Persist the build's node/lane/score-index diff to a `WriteBatch` and populate caches.
    ///
    /// `branch_blue_score` versions the nodes and is the same blue_score used
    /// for lane/score-index writes inside `BlockLaneChanges`.
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
