//! Batched RocksDB node writer implementing [`MergeSink`].

use std::collections::HashMap;

use kaspa_database::prelude::{BatchDbWriter, DB, StoreError, StoreResult};
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_smt::store::{BranchKey, CollapsedLeaf, Node};
use kaspa_smt::streaming::{ChildInfo, MergeSink};
use kaspa_smt::{DEPTH, SmtHasher, bit_at, hash_node};
use rocksdb::WriteBatch;

use crate::BlockHash;
use crate::processor::SmtStores;

pub(crate) struct DbSink<'a> {
    db: &'a DB,
    stores: &'a SmtStores,
    batch: WriteBatch,
    batch_count: usize,
    max_batch_entries: usize,
    block_hash: BlockHash,
    nodes_written: usize,
    /// Per-lane seal depth, populated by [`MergeSink::record_seal`] callbacks.
    /// Presence in the map = the lane has been sealed (its seal_depth is the
    /// deepest depth we'll ever need for branches with rep_key matching this
    /// lane). Absence = unsealed; its containing pending score-index entry
    /// can't flush yet. `streaming_import` calls [`forget_lane`] after
    /// flushing an entry to bound memory.
    lane_seal_depth: HashMap<Hash, u8>,
}

impl<'a> DbSink<'a> {
    pub(crate) fn new(db: &'a DB, stores: &'a SmtStores, block_hash: BlockHash, max_batch_entries: usize) -> Self {
        Self {
            db,
            stores,
            batch: WriteBatch::default(),
            batch_count: 0,
            max_batch_entries,
            block_hash,
            nodes_written: 0,
            lane_seal_depth: HashMap::with_capacity(max_batch_entries),
        }
    }

    /// Persist a branch_version entry at the given `blue_score`.
    ///
    /// The bs is supplied per-write (not per-sink) so collapsed leaves are
    /// versioned at the lane's own bs and internal nodes at the max bs of
    /// leaves underneath them — matching what the live processor would have
    /// written. See [`crate::streaming_import`] for the broader motivation.
    pub(crate) fn write_node(&mut self, bk: BranchKey, node: Node, blue_score: u64) -> StoreResult<()> {
        // Writes go directly to the DB branch-version store and intentionally
        // skip the in-memory branch cache. `SmtStores::get_node` treats a
        // cache hit as authoritative (see the newest-suffix invariant in
        // `crate::cache`), so bypassing the cache is safe only because IBD
        // SMT import runs after `SmtStores::clear_all()` has emptied both
        // the DB stores and the caches. Thus there can be no stale cached
        // branch versions disagreeing with the imported DB state. After
        // import the caches remain cold, and reads fall back to DB until
        // later incremental writes repopulate them.
        self.stores.branch_version.put(
            BatchDbWriter::new(&mut self.batch),
            bk.depth,
            bk.node_key,
            blue_score,
            self.block_hash,
            Some(node),
        )?;
        self.batch_count += 1;
        self.nodes_written += 1;
        if self.batch_count >= self.max_batch_entries {
            self.flush_batch()?;
        }
        Ok(())
    }

    pub(crate) fn flush_batch(&mut self) -> StoreResult<()> {
        if self.batch_count > 0 {
            self.db.write(std::mem::take(&mut self.batch)).map_err(StoreError::DbError)?;
            self.batch_count = 0;
        }
        Ok(())
    }

    pub(crate) fn nodes_written(&self) -> usize {
        self.nodes_written
    }

    /// `Some(depth)` if the lane has been sealed, `None` otherwise.
    pub(crate) fn seal_depth_for(&self, lane_key: &Hash) -> Option<u8> {
        self.lane_seal_depth.get(lane_key).copied()
    }

    /// Drop the per-lane seal entry for each given `lane_key`. Called by
    /// `streaming_import` after a pending score-index entry has been flushed,
    /// so the map's footprint stays bounded by unflushed lanes rather than
    /// the full import.
    pub(crate) fn forget_lanes<I: IntoIterator<Item = Hash>>(&mut self, lane_keys: I) {
        for lk in lane_keys {
            self.lane_seal_depth.remove(&lk);
        }
    }

    fn write_collapsed_child(&mut self, info: &ChildInfo) -> StoreResult<()> {
        if let ChildInfo::Collapsed { branch_key, leaf, blue_score } = info {
            self.write_node(*branch_key, Node::Collapsed(*leaf), *blue_score)?;
        }
        Ok(())
    }
}

impl MergeSink for DbSink<'_> {
    type Error = StoreError;

    fn merge(
        &mut self,
        left: Hash,
        right: Hash,
        parent_key: BranchKey,
        left_info: ChildInfo,
        right_info: ChildInfo,
        parent_blue_score: u64,
    ) -> Result<Hash, Self::Error> {
        self.write_collapsed_child(&left_info)?;
        self.write_collapsed_child(&right_info)?;
        let parent_hash = hash_node::<SeqCommitActiveNode>(left, right);
        self.write_node(parent_key, Node::Internal(parent_hash), parent_blue_score)?;
        Ok(parent_hash)
    }

    fn merge_chain_with_empty(
        &mut self,
        hash: Hash,
        from_depth: usize,
        to_depth: usize,
        representative_key: &Hash,
        blue_score: u64,
    ) -> Result<Hash, Self::Error> {
        let mut current_hash = hash;
        for d in (to_depth..from_depth).rev() {
            let height = DEPTH - 1 - d;
            let goes_right = bit_at(representative_key, d);
            let empty_h = SeqCommitActiveNode::EMPTY_HASHES[height];
            let (left_h, right_h) = if goes_right { (empty_h, current_hash) } else { (current_hash, empty_h) };
            current_hash = hash_node::<SeqCommitActiveNode>(left_h, right_h);
            self.write_node(BranchKey::new(d as u8, representative_key), Node::Internal(current_hash), blue_score)?;
        }
        Ok(current_hash)
    }

    fn write_collapsed(&mut self, branch_key: BranchKey, leaf: CollapsedLeaf, blue_score: u64) -> Result<(), Self::Error> {
        // Single-leaf-tree finalize path: ensure the lane is recorded as
        // sealed at the actual write depth. `chain_up` may have already
        // recorded `target_depth + 1`; the `and_modify` below keeps the max.
        self.record_seal(leaf.lane_key, branch_key.depth);
        self.write_node(branch_key, Node::Collapsed(leaf), blue_score)
    }

    fn record_seal(&mut self, lane_key: Hash, seal_depth: u8) {
        self.lane_seal_depth.entry(lane_key).and_modify(|d| *d = (*d).max(seal_depth)).or_insert(seal_depth);
    }
}
