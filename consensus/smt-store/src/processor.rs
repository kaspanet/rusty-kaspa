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
use std::convert::Infallible;
use std::marker::PhantomData;
use std::ops::RangeInclusive;
use std::sync::Arc;

use parking_lot::Mutex;

use kaspa_database::prelude::{BatchDbWriter, DB, DirectDbWriter, StoreError, StoreResult};
use kaspa_hashes::{Hash, SeqCommitActiveNode, ZERO_HASH};
use kaspa_seq_commit::hashing::smt_leaf_hash;
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::proof::OwnedSmtProof;
use kaspa_smt::store::{BranchKey, CollapsedLeaf, Node, SmtStore, SortedLeafUpdates};
use kaspa_smt::streaming::{ChildInfo, MergeSink, StreamError, StreamingSmtBuilder};
use kaspa_smt::tree::{SmtNodeChanges, SparseMerkleTree, compute_root_update};
use kaspa_smt::{DEPTH, SmtHasher, bit_at, hash_node};
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
    bounds: SmtReadBounds,
    is_canonical: F,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmtReadBounds {
    /// Inclusive upper bound blue score to start the scan from (high to low)
    pub target_blue_score: u64,
    /// Inclusive lower bound blue score below which entries are out of scope/inactive
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

impl From<RangeInclusive<u64>> for SmtReadBounds {
    fn from(range: RangeInclusive<u64>) -> Self {
        Self::new(*range.end(), *range.start())
    }
}

impl<F: Fn(Hash) -> bool> SmtStore for VersionedBranchReader<'_, F> {
    type Error = StoreError;

    fn get_node(&self, key: &BranchKey) -> Result<Option<Node>, StoreError> {
        let entity = BranchEntity { depth: key.depth, node_key: key.node_key };
        Ok(self.stores.get_node(entity, self.bounds, |bh| (self.is_canonical)(bh)).and_then(|v| *v.data()))
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

struct RootOnlyMergeSink<H>(PhantomData<H>);

impl<H> Default for RootOnlyMergeSink<H> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<H: SmtHasher> MergeSink for RootOnlyMergeSink<H> {
    type Error = Infallible;

    fn merge(
        &mut self,
        left: Hash,
        right: Hash,
        _parent_key: BranchKey,
        _left_info: ChildInfo,
        _right_info: ChildInfo,
    ) -> Result<Hash, Self::Error> {
        Ok(hash_node::<H>(left, right))
    }

    fn merge_chain_with_empty(
        &mut self,
        hash: Hash,
        from_depth: usize,
        to_depth: usize,
        representative_key: &Hash,
    ) -> Result<Hash, Self::Error> {
        let mut current_hash = hash;

        for depth in (to_depth..from_depth).rev() {
            let height = DEPTH - 1 - depth;
            let empty_hash = H::EMPTY_HASHES[height];
            let goes_right = bit_at(representative_key, depth);
            let (left_hash, right_hash) = if goes_right { (empty_hash, current_hash) } else { (current_hash, empty_hash) };
            current_hash = hash_node::<H>(left_hash, right_hash);
        }

        Ok(current_hash)
    }

    fn write_collapsed(&mut self, _branch_key: BranchKey, _leaf: CollapsedLeaf) -> Result<(), Self::Error> {
        Ok(())
    }
}

fn stream_error_to_store_error<E: std::fmt::Debug>(err: StreamError<E>) -> StoreError {
    StoreError::DataInconsistency(format!("streaming SMT root recompute: {err}"))
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
    ///
    /// A cache hit is authoritative (no DB fallback) because of the
    /// newest-suffix invariant: the cache retains, per entity, a
    /// blue-score-newest suffix of the versions written through the
    /// incremental `flush` path. See the module doc in [`crate::cache`] for
    /// the full argument and the interaction with the IBD cache-bypass path.
    pub fn get_node(
        &self,
        entity: BranchEntity,
        bounds: SmtReadBounds,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<Option<Node>>> {
        if let Some((score, block_hash, value)) =
            self.branch_cache.lock().get(entity, bounds.target_blue_score, bounds.min_blue_score, &mut is_canonical)
        {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.branch_version
            .get_at_canonical(entity.depth, entity.node_key, bounds.target_blue_score, bounds.min_blue_score, is_canonical)
            .unwrap()
    }

    /// Find the latest canonical lane version in `[min_blue_score, target_blue_score]`,
    /// checking cache first then DB.
    ///
    /// A cache hit is authoritative for the same reason as [`Self::get_node`];
    /// see that method's doc and [`crate::cache`] for the newest-suffix
    /// invariant.
    pub fn get_lane(
        &self,
        lane_key: LaneKey,
        bounds: SmtReadBounds,
        mut is_canonical: impl FnMut(Hash) -> bool,
    ) -> Option<Verified<LaneTipHash>> {
        if let Some((score, block_hash, value)) =
            self.lane_cache.lock().get(lane_key, bounds.target_blue_score, bounds.min_blue_score, &mut is_canonical)
        {
            return Some(Verified::new(*value, score, block_hash));
        }
        self.lane_version.get_at_canonical(lane_key, bounds.target_blue_score, bounds.min_blue_score, is_canonical).unwrap()
    }

    /// Read the lanes root hash from the branch store at depth=0.
    /// Returns the empty root if no root node exists.
    pub fn get_lanes_root(&self, bounds: SmtReadBounds, is_canonical: impl FnMut(Hash) -> bool) -> Hash {
        let root_entity = BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) };
        match self.get_node(root_entity, bounds, is_canonical) {
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

    /// Recompute the lanes root directly from the active lane-version stream,
    /// without writing any branch nodes or using temporary stores.
    pub fn recompute_lanes_root_from_leaf_stream(
        &self,
        bounds: SmtReadBounds,
        total_count: u64,
        is_canonical: impl Fn(Hash) -> bool,
    ) -> StoreResult<(Hash, u64)> {
        let mut builder =
            StreamingSmtBuilder::<SeqCommitActiveNode, _>::new(total_count, RootOnlyMergeSink::<SeqCommitActiveNode>::default());
        let mut count = 0u64;

        for lane in self.lane_version.iter_all_canonical(None, bounds.min_blue_score, Some(bounds.target_blue_score), is_canonical) {
            let (lane_key, verified) = lane?;
            let leaf_hash = smt_leaf_hash(&SmtLeafInput { lane_tip: verified.data(), blue_score: verified.blue_score() });
            builder.feed(lane_key, leaf_hash).map_err(stream_error_to_store_error)?;
            count += 1;
        }

        let (root, _) = builder.finish().map_err(stream_error_to_store_error)?;
        Ok((root, count))
    }

    /// Generate an inclusion proof for `lane_key` in the canonical tree as of
    /// `target_blue_score`.
    pub fn prove_lane(
        &self,
        lane_key: &Hash,
        bounds: SmtReadBounds,
        is_canonical: impl Fn(Hash) -> bool,
    ) -> StoreResult<OwnedSmtProof> {
        let reader = VersionedBranchReader { stores: self, bounds, is_canonical };
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

    /// Clear all versioned SMT stores and caches. Used before IBD SMT sync
    /// to ensure that the caches are cold and the DB is empty, preserving
    /// the authoritative-read invariants when incremental processing resumes.
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
        SortedLeafUpdates::from_sorted_map(&self.changes, |_key, change| match change {
            Some(tip) => smt_leaf_hash(&SmtLeafInput { lane_tip: tip, blue_score: bs }),
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
    bounds: SmtReadBounds,
    current_lanes_root: Hash,
    lane_changes: BlockLaneChanges,
}

impl<'a> SmtProcessor<'a> {
    pub fn new(stores: &'a SmtStores, write_blue_score: u64, bounds: SmtReadBounds, current_lanes_root: Hash) -> Self {
        Self { stores, bounds, current_lanes_root, lane_changes: BlockLaneChanges::new(write_blue_score) }
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
        let reader = VersionedBranchReader { stores: self.stores, bounds: self.bounds, is_canonical };
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::create_temp_db;
    use kaspa_database::prelude::{ConnBuilder, DirectDbWriter};

    fn hash(v: u8) -> Hash {
        Hash::from_bytes([v; 32])
    }

    fn make_stores() -> (kaspa_database::utils::DbLifetime, Arc<DB>, SmtStores) {
        let (lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let stores = SmtStores::new(db.clone(), 16, 16);
        (lifetime, db, stores)
    }

    fn bounds(target: u64, min: u64) -> SmtReadBounds {
        SmtReadBounds::new(target, min)
    }

    fn internal(h: Hash) -> Option<Node> {
        Some(Node::Internal(h))
    }

    fn entity() -> BranchEntity {
        BranchEntity { depth: 7, node_key: hash(0xEE) }
    }

    /// The cache's "for every entity, cached versions are a blue-score-newest
    /// suffix of that entity's write history" invariant is a *same-session*
    /// property: it only holds for versions written since the cache was last
    /// cleared. A process restart clears the cache but leaves the DB intact,
    /// so post-restart the cache can hold a version at blue_score X while the
    /// DB retains pre-restart versions at scores higher than X that were
    /// never reloaded into the cache.
    ///
    /// The surviving invariant is weaker: if the cache has entry `(X, Y)` for
    /// entity E, it has every newer entry for E whose block_hash has Y as a
    /// chain ancestor — i.e. only along Y's chain. DB-only entries above X on
    /// an off-chain branch (a pre-restart canonical write on a chain that
    /// the post-restart cache fill didn't touch) are allowed.
    ///
    /// `SmtStores::get_node` must still return the canonical DB entry above
    /// the cache's oldest version in this scenario. A DB-continuation
    /// strategy that resumes strictly after the cache's last-visited
    /// `(score, bh)` silently misses the higher-score DB entry and returns
    /// `None` — an authoritative-looking stale result.
    #[test]
    fn get_node_restart_then_off_chain_cache_misses_db_entry_above() {
        let (_lt, db, stores) = make_stores();
        let e = entity();
        let pre_restart_bh = hash(0xAA);
        let pre_restart_node = internal(hash(0xCC));
        let post_restart_bh = hash(0xBB);
        let post_restart_node = internal(hash(0xDD));

        // Pre-restart DB write at blue_score 200. The restart clears the
        // in-memory cache, so this version is now DB-only.
        stores.branch_version.put(DirectDbWriter::new(&db), e.depth, e.node_key, 200, pre_restart_bh, pre_restart_node).unwrap();

        // Post-restart normal flush path: write E at blue_score 100 into
        // both DB and cache. The cached entry is on an off-chain fork
        // relative to the query's `is_canonical` predicate below.
        stores.branch_version.put(DirectDbWriter::new(&db), e.depth, e.node_key, 100, post_restart_bh, post_restart_node).unwrap();
        stores.branch_cache.lock().insert(e, 100, post_restart_bh, post_restart_node);

        // Query from the POV of a chain whose canonical entry for E is the
        // pre-restart DB-only version at 200.
        let got = stores
            .get_node(e, bounds(300, 0), |bh| bh == pre_restart_bh)
            .expect("pre-restart canonical DB entry above the cache's oldest must still be returned");
        assert_eq!(got.blue_score(), 200, "must return the DB entry at blue_score 200 above the cache's (100, post-restart-bh)");
        assert_eq!(got.block_hash(), pre_restart_bh);
        assert_eq!(*got.data(), pre_restart_node);
    }

    /// Lane-cache analogue of
    /// [`get_node_restart_then_off_chain_cache_misses_db_entry_above`].
    ///
    /// The restart+reorg scenario — post-restart cache only holds an
    /// off-chain version at blue_score 100 while the DB still has a
    /// pre-restart canonical version at blue_score 200 — applies equally to
    /// the lane store. `get_lane` must return the DB entry at 200 for an
    /// `is_canonical` predicate that matches the pre-restart block_hash.
    #[test]
    fn get_lane_restart_then_off_chain_cache_misses_db_entry_above() {
        let (_lt, db, stores) = make_stores();
        let lane_key = hash(0xEE);
        let pre_restart_bh = hash(0xAA);
        let pre_restart_tip = hash(0xCC);
        let post_restart_bh = hash(0xBB);
        let post_restart_tip = hash(0xDD);

        stores.lane_version.put(DirectDbWriter::new(&db), lane_key, 200, pre_restart_bh, &pre_restart_tip).unwrap();

        stores.lane_version.put(DirectDbWriter::new(&db), lane_key, 100, post_restart_bh, &post_restart_tip).unwrap();
        stores.lane_cache.lock().insert(lane_key, 100, post_restart_bh, post_restart_tip);

        let got = stores
            .get_lane(lane_key, bounds(300, 0), |bh| bh == pre_restart_bh)
            .expect("pre-restart canonical DB lane entry above the cache's oldest must still be returned");
        assert_eq!(got.blue_score(), 200);
        assert_eq!(got.block_hash(), pre_restart_bh);
        assert_eq!(*got.data(), pre_restart_tip);
    }
}
