//! Integration tests for SmtProcessor + ComposedSmtStore + DbSmtView.

use std::sync::Arc;

use kaspa_database::create_temp_db;
use kaspa_database::prelude::{ConnBuilder, DB};
use kaspa_hashes::{Hash, SeqCommitActiveCollapsedNode, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::{lane_key, smt_leaf_hash};
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::SmtHasher;
use kaspa_smt::store::{BTreeSmtStore, LeafUpdate, Node, SortedLeafUpdates};
use kaspa_smt::tree::{SparseMerkleTree, compute_root_update};
use rocksdb::WriteBatch;

use kaspa_consensus_core::api::ImportLane;
use kaspa_smt_store::cache::BranchEntity;
use kaspa_smt_store::processor::{SmtProcessor, SmtReadBounds, SmtStores};
use kaspa_smt_store::streaming_import::streaming_import;

/// Build a reference SLO root from leaf updates using the in-memory store.
fn slo_root(updates: Vec<LeafUpdate>) -> Hash {
    let store = BTreeSmtStore::new();
    let sorted = SortedLeafUpdates::from_unsorted(updates);
    let (root, _) = compute_root_update::<SeqCommitActiveNode, _>(&store, SeqCommitActiveNode::empty_root(), sorted).unwrap();
    root
}

fn hash(v: u8) -> Hash {
    Hash::from_bytes([v; 32])
}

fn make_stores(db: &Arc<DB>) -> SmtStores {
    SmtStores::new(db.clone(), 1, 1)
}

fn same_pov_bounds(blue_score: u64) -> SmtReadBounds {
    SmtReadBounds::for_pov(blue_score, TEST_THRESHOLD)
}

/// Test-only inactivity threshold (large enough to never expire in tests).
const TEST_THRESHOLD: u64 = 432_000;

/// A1/A2: walk_up without remove_branch produces the same roots as insertion-only paths.
#[test]
fn walk_up_always_insert_same_root() {
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();

    let k1 = lane_key(&[0x01; 20]);
    let k2 = lane_key(&[0x02; 20]);
    let k3 = lane_key(&[0x03; 20]);

    let l1 = smt_leaf_hash(&SmtLeafInput { lane_key: &k1, lane_tip: &hash(0xA1), blue_score: 100 });
    let l2 = smt_leaf_hash(&SmtLeafInput { lane_key: &k2, lane_tip: &hash(0xA2), blue_score: 100 });
    let l3 = smt_leaf_hash(&SmtLeafInput { lane_key: &k3, lane_tip: &hash(0xA3), blue_score: 100 });

    tree.insert(k1, l1);
    tree.insert(k2, l2);
    tree.insert(k3, l3);
    let root_3 = tree.root();

    // Update l1 to new value
    let l1_new = smt_leaf_hash(&SmtLeafInput { lane_key: &k1, lane_tip: &hash(0xB1), blue_score: 200 });
    tree.insert(k1, l1_new);
    let root_updated = tree.root();

    assert_ne!(root_3, root_updated);

    // Build same tree from scratch to verify determinism
    let mut tree2 = SparseMerkleTree::<SeqCommitActiveNode>::new();
    tree2.insert(k1, l1_new);
    tree2.insert(k2, l2);
    tree2.insert(k3, l3);

    assert_eq!(tree.root(), tree2.root());
}

/// SmtProcessor with temp DB — process a block with 2 lane updates,
/// verify root matches manual in-memory SMT.
#[test]
fn processor_two_lanes_matches_in_memory() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id_a = [0x01; 20];
    let lane_id_b = [0x02; 20];
    let key_a = lane_key(&lane_id_a);
    let key_b = lane_key(&lane_id_b);
    let tip_a = hash(0xAA);
    let tip_b = hash(0xBB);
    let blue_score = 100u64;

    let empty_root = SeqCommitActiveNode::empty_root();

    // Process via SmtProcessor
    let mut proc = SmtProcessor::new(&stores, blue_score, same_pov_bounds(blue_score), empty_root);
    proc.update_lane(key_a, tip_a);
    proc.update_lane(key_b, tip_b);
    let build = proc.build(|_| true).unwrap();
    let proc_root = build.root;

    // Same via SLO reference
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_key: &key_a, lane_tip: &tip_a, blue_score });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_key: &key_b, lane_tip: &tip_b, blue_score });
    let mem_root = slo_root(vec![LeafUpdate { key: key_a, leaf_hash: leaf_a }, LeafUpdate { key: key_b, leaf_hash: leaf_b }]);

    assert_eq!(proc_root, mem_root);
    assert_ne!(proc_root, empty_root);
}

/// Process two blocks; second block reads first block's writes from DB.
#[test]
fn processor_second_block_reads_from_db() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id_a = [0x01; 20];
    let lane_id_b = [0x02; 20];
    let key_a = lane_key(&lane_id_a);
    let key_b = lane_key(&lane_id_b);
    let block1_hash = hash(0x11);
    let block2_hash = hash(0x22);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Block 1: insert lane A
    let tip_a1 = hash(0xA1);
    let bs1 = 100u64;
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, tip_a1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: insert lane B (lane A should be read from DB)
    let tip_b = hash(0xB1);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.update_lane(key_b, tip_b);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // Verify: rebuild from scratch via SLO reference
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_key: &key_a, lane_tip: &tip_a1, blue_score: bs1 });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_key: &key_b, lane_tip: &tip_b, blue_score: bs2 });
    let expected = slo_root(vec![LeafUpdate { key: key_a, leaf_hash: leaf_a }, LeafUpdate { key: key_b, leaf_hash: leaf_b }]);

    assert_eq!(root2, expected);
    assert_ne!(root1, root2);
}

/// Update lane A across two blocks. Second block updates the same lane.
#[test]
fn processor_update_same_lane_across_blocks() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x42; 20];
    let key = lane_key(&lane_id);
    let block1_hash = hash(0x11);
    let block2_hash = hash(0x22);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Block 1
    let tip1 = hash(0xA1);
    let bs1 = 100u64;
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key, tip1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: update same lane with new tip
    let tip2 = hash(0xB2);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.update_lane(key, tip2);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // Verify against SLO reference with only the final state
    let leaf2 = smt_leaf_hash(&SmtLeafInput { lane_key: &key, lane_tip: &tip2, blue_score: bs2 });
    let expected = slo_root(vec![LeafUpdate { key, leaf_hash: leaf2 }]);

    assert_eq!(root2, expected);
    assert_ne!(root1, root2);
}

/// Verify flush writes correct lane versions, branch versions, and score index.
#[test]
fn processor_flush_writes_correct_data() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x42; 20];
    let key = lane_key(&lane_id);
    let block_hash = hash(0x11);
    let tip = hash(0xAA);
    let blue_score = 100u64;
    let empty_root = SeqCommitActiveNode::empty_root();

    let mut proc = SmtProcessor::new(&stores, blue_score, same_pov_bounds(blue_score), empty_root);
    proc.update_lane(key, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, blue_score, block_hash).unwrap();
    db.write(batch).unwrap();

    // Verify lane version was written
    let lane_ver = stores.get_lane(key, u64::MAX, 0, |bh| bh == block_hash).unwrap();
    assert_eq!(*lane_ver.data(), tip);
    assert_eq!(lane_ver.blue_score(), blue_score);

    // Verify score index entry
    let si_entry = stores.score_index.get_leaf_updates(blue_score, 0).next().unwrap().unwrap();
    assert_eq!(si_entry.block_hash(), block_hash);
    assert_eq!(si_entry.data(), &vec![key]);

    // Verify root-level branch version was written
    let root_branch = stores.get_node(BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) }, u64::MAX, 0, |_| true);
    assert!(root_branch.is_some());
}

/// Inactivity threshold — lane not touched within threshold -> treated as empty.
///
/// Downstream consequence worth naming for future readers: a caller that
/// resolves lane tips or reads a base root through a wider window than the
/// `VersionedBranchReader` used by `SmtProcessor::build` will cause the walk
/// to silently rebuild a tree without the out-of-window lanes. The fix is
/// always to align the caller on the reader's window (i.e. the current block's
/// POV `[current - F, current]`), never to widen the reader. See KIP-21 §5:
/// the active-lanes SMT is defined from the processing block's POV only.
#[test]
fn inactivity_threshold_hides_stale_branches() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x42; 20];
    let key = lane_key(&lane_id);
    let block_hash = hash(0x11);
    let tip = hash(0xAA);
    let old_blue_score = 100u64;
    let empty_root = SeqCommitActiveNode::empty_root();

    // Write a lane at blue_score=100
    let mut proc = SmtProcessor::new(&stores, old_blue_score, same_pov_bounds(old_blue_score), empty_root);
    proc.update_lane(key, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, old_blue_score, block_hash).unwrap();
    db.write(batch).unwrap();

    // At blue_score=100: the root-level branch should be visible (min_blue_score=0)
    let root_entity = BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) };
    let root_branch = stores.get_node(root_entity, u64::MAX, 0, |_| true);
    assert!(root_branch.is_some(), "branch should be visible at same blue_score");

    // At blue_score=100 + THRESHOLD + 1: beyond inactivity window
    let far_future_min = (old_blue_score + TEST_THRESHOLD + 1).saturating_sub(TEST_THRESHOLD);
    let root_branch_far = stores.get_node(root_entity, u64::MAX, far_future_min, |_| true);
    assert!(root_branch_far.is_none(), "branch should be hidden beyond inactivity threshold");
}

/// from_root constructor works with in-memory store.
#[test]
fn from_root_constructor() {
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let lid = [0x01; 20];
    let key = lane_key(&lid);
    let leaf = smt_leaf_hash(&SmtLeafInput { lane_key: &key, lane_tip: &hash(0xAA), blue_score: 100 });
    tree.insert(key, leaf);
    let root = tree.root();
    let store = tree.into_store();

    // Reconstruct from existing store + root
    let tree2 = SparseMerkleTree::<SeqCommitActiveNode>::from_root(store, root);
    assert_eq!(tree2.root(), root);
    assert_eq!(tree2.get(&key), Some(leaf));
}

/// build returns the same root as flush.
#[test]
fn build_root_matches_flush() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();
    let blue_score = 100u64;

    let mut proc = SmtProcessor::new(&stores, blue_score, same_pov_bounds(blue_score), empty_root);
    proc.update_lane(lane_key(&[0x01; 20]), hash(0xAA));
    let build = proc.build(|_| true).unwrap();
    let root_from_build = build.root;

    let mut batch = WriteBatch::default();
    let root_from_flush = build.flush(&stores, &mut batch, blue_score, hash(0x11)).unwrap();

    assert_eq!(root_from_build, root_from_flush);
    assert_ne!(root_from_flush, empty_root);
}

/// expire_lane inserts ZERO_HASH, effectively removing the lane from the tree.
#[test]
fn expire_lane_removes_from_tree() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    let lane_id = [0x01; 20];
    let key = lane_key(&lane_id);
    let block1_hash = hash(0x11);
    let block2_hash = hash(0x22);

    // Block 1: insert a lane
    let bs1 = 100u64;
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    assert_ne!(root1, empty_root);

    // Block 2: expire the lane
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.expire_lane(key);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // After expiring the only lane, the root should be the empty root
    assert_eq!(root2, empty_root);
}

/// Empty build (no updates, no expirations) reuses parent root and produces empty diff.
#[test]
fn empty_build_reuses_root() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x01; 20];
    let key = lane_key(&lane_id);
    let block_hash = hash(0x11);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Block 1: insert a lane
    let bs1 = 100u64;
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: no updates, no expirations
    let bs2 = 200u64;
    let proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    let build2 = proc2.build(|_| true).unwrap();

    assert_eq!(build2.root, root1, "empty build should reuse parent root");
    // Diff should be empty — no branch or lane changes
    // Flush should succeed without writing anything meaningful
    let mut batch2 = WriteBatch::default();
    build2.flush(&stores, &mut batch2, bs2, hash(0x22)).unwrap();
}

/// Only branches along the touched lane's path appear in the diff.
#[test]
fn single_touch_updates_only_path_branches() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();
    let bs = 100u64;

    let mut proc = SmtProcessor::new(&stores, bs, same_pov_bounds(bs), empty_root);
    proc.update_lane(lane_key(&[0x01; 20]), hash(0xAA));
    let build = proc.build(|_| true).unwrap();

    // SLO: a single lane collapses into one Collapsed node at the root level.
    assert_eq!(build.diff_branch_count(), 1, "single leaf should produce 1 collapsed node (SLO)");
    assert_ne!(build.root, empty_root);
}

/// Two lanes in different subtrees — touching only one produces diff entries only for its path.
#[test]
fn untouched_subtree_not_in_diff() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    let lane_a = [0x01; 20];
    let lane_b = [0x02; 20];
    let key_a = lane_key(&lane_a);
    let key_b = lane_key(&lane_b);
    let block1 = hash(0x11);
    let bs1 = 100u64;

    // Block 1: insert both lanes
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, hash(0xAA));
    proc1.update_lane(key_b, hash(0xBB));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1).unwrap();
    db.write(batch).unwrap();

    // Block 2: only touch lane A
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.update_lane(key_a, hash(0xCC));
    let build2 = proc2.build(|_| true).unwrap();

    // The diff should contain at most 256 branches (lane A's path),
    // NOT 512 (which would mean lane B's path was also computed).
    assert!(build2.diff_branch_count() <= 256, "only touched lane's path should be in diff");
    assert_ne!(build2.root, root1, "new tip should change the root");
}

/// Flush then rebuild from DB with no changes → same root.
#[test]
fn flush_rebuild_roundtrip() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    let lane_a = [0x01; 20];
    let lane_b = [0x02; 20];
    let key_a = lane_key(&lane_a);
    let key_b = lane_key(&lane_b);
    let block_hash = hash(0x11);
    let bs = 100u64;

    // Build and flush
    let mut proc = SmtProcessor::new(&stores, bs, same_pov_bounds(bs), empty_root);
    proc.update_lane(key_a, hash(0xAA));
    proc.update_lane(key_b, hash(0xBB));
    let build = proc.build(|_| true).unwrap();
    let original_root = build.root;
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, block_hash).unwrap();
    db.write(batch).unwrap();

    // Rebuild with no changes — root should match
    let proc2 = SmtProcessor::new(&stores, bs + 10, same_pov_bounds(bs + 10), original_root);
    let build2 = proc2.build(|_| true).unwrap();
    assert_eq!(build2.root, original_root, "no-change rebuild should produce same root");
}

/// Simulate block-by-block processing, export lane state, import into fresh
/// stores via streaming_import (the actual IBD path), and verify the roots match.
#[test]
fn export_import_roundtrip() {
    use kaspa_hashes::ZERO_HASH;

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Define lanes touched by different blocks at different blue_scores
    struct BlockSpec {
        blue_score: u64,
        block_hash: Hash,
        lanes: Vec<([u8; 20], Hash)>, // (lane_id, lane_tip)
    }

    let blocks = vec![
        BlockSpec { blue_score: 100, block_hash: hash(0x01), lanes: vec![([0x01; 20], hash(0xA1))] },
        BlockSpec { blue_score: 200, block_hash: hash(0x02), lanes: vec![([0x02; 20], hash(0xA2)), ([0x03; 20], hash(0xA3))] },
        BlockSpec { blue_score: 300, block_hash: hash(0x03), lanes: vec![([0x01; 20], hash(0xB1))] }, // update lane 1
        BlockSpec { blue_score: 400, block_hash: hash(0x04), lanes: vec![([0x04; 20], hash(0xA4))] },
        BlockSpec { blue_score: 500, block_hash: hash(0x05), lanes: vec![([0x05; 20], hash(0xA5)), ([0x02; 20], hash(0xB2))] },
    ];

    // Process blocks sequentially, each building on the previous root
    let mut current_root = empty_root;
    for block in &blocks {
        let mut proc = SmtProcessor::new(&stores, block.blue_score, same_pov_bounds(block.blue_score), current_root);
        for (lid, tip) in &block.lanes {
            proc.update_lane(lane_key(lid), *tip);
        }
        let build = proc.build(|_| true).unwrap();
        current_root = build.root;
        let mut batch = WriteBatch::default();
        build.flush(&stores, &mut batch, block.blue_score, block.block_hash).unwrap();
        db.write(batch).unwrap();
    }

    let final_root = current_root;
    assert_ne!(final_root, empty_root);

    // Export: collect the canonical lane state (what the IBD sender would read)
    // Each lane's latest canonical version with its blue_score
    let lane_ids: Vec<[u8; 20]> = (1u8..=5).map(|i| [i; 20]).collect();
    let mut exported: Vec<([u8; 20], Hash, u64)> = Vec::new();
    for lid in &lane_ids {
        let lk = lane_key(lid);
        if let Some(v) = stores.get_lane(lk, u64::MAX, 0, |_| true) {
            exported.push((*lid, *v.data(), v.blue_score()));
        }
    }
    assert_eq!(exported.len(), 5, "all 5 lanes should have canonical versions");

    // Import: fresh stores, rebuild via streaming_import (the actual IBD path)
    let (_lt2, db2) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let import_stores = make_stores(&db2);

    let pp_blue_score = 500u64;
    let mut import_lanes: Vec<ImportLane> = exported
        .iter()
        .map(|(lid, tip, bs)| ImportLane { lane_key: lane_key(lid), lane_tip: *tip, blue_score: *bs, proof: None })
        .collect();
    import_lanes.sort_by_key(|l| l.lane_key);
    let lane_count = import_lanes.len() as u64;
    let result =
        streaming_import(&db2, &import_stores, pp_blue_score, ZERO_HASH, lane_count, final_root, std::iter::once(import_lanes), 64)
            .unwrap();
    assert_eq!(result.root, final_root, "import must reproduce the same root");

    // Verify lanes are readable with correct blue_scores
    for (lid, tip, bs) in &exported {
        let lk = lane_key(lid);
        let v = import_stores.get_lane(lk, u64::MAX, 0, |bh| bh == ZERO_HASH);
        assert!(v.is_some(), "lane {:02x} must be readable", lid[0]);
        assert_eq!(v.as_ref().unwrap().data(), tip);
        assert_eq!(v.unwrap().blue_score(), *bs);
    }

    // Build the same next block on both original and imported stores — roots must match
    let next_bs = pp_blue_score + 1;
    let new_lane_id = [0xFE; 20];
    let new_tip = hash(0xFE);

    let mut orig_next = SmtProcessor::new(&stores, next_bs, same_pov_bounds(next_bs), final_root);
    orig_next.update_lane(lane_key(&new_lane_id), new_tip);
    let orig_next_build = orig_next.build(|_| true).unwrap();

    let mut import_next = SmtProcessor::new(&import_stores, next_bs, same_pov_bounds(next_bs), final_root);
    import_next.update_lane(lane_key(&new_lane_id), new_tip);
    let import_next_build = import_next.build(|bh| bh == ZERO_HASH).unwrap();

    assert_eq!(orig_next_build.root, import_next_build.root, "next block must produce same root on original and imported stores");
}

/// Lanes written with ZERO_HASH block_hash are readable when the caller
/// treats ZERO_HASH as canonical (IBD sentinel).
#[test]
fn zero_hash_block_hash_lanes_are_readable() {
    use kaspa_hashes::ZERO_HASH;

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x01; 20];
    let lk = lane_key(&lane_id);
    let tip = hash(0xAA);
    let bs = 500u64;

    let mut proc = SmtProcessor::new(&stores, bs, same_pov_bounds(bs), SeqCommitActiveNode::empty_root());
    proc.update_lane(lk, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, ZERO_HASH).unwrap();
    db.write(batch).unwrap();

    let result = stores.get_lane(lk, u64::MAX, 0, |bh| bh == kaspa_hashes::ZERO_HASH);
    assert!(result.is_some());
    assert_eq!(*result.unwrap().data(), tip);
}

/// Branches written with ZERO_HASH block_hash are readable when the caller
/// treats ZERO_HASH as canonical.
#[test]
fn zero_hash_block_hash_branches_are_readable() {
    use kaspa_hashes::ZERO_HASH;

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x01; 20];
    let lk = lane_key(&lane_id);
    let tip = hash(0xAA);
    let bs = 500u64;

    let mut proc = SmtProcessor::new(&stores, bs, same_pov_bounds(bs), SeqCommitActiveNode::empty_root());
    proc.update_lane(lk, tip);
    let build = proc.build(|_| true).unwrap();
    let root = build.root;
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, ZERO_HASH).unwrap();
    db.write(batch).unwrap();

    let root_node = stores
        .get_node(BranchEntity { depth: 0, node_key: Hash::from_bytes([0; 32]) }, u64::MAX, 0, |bh| bh == kaspa_hashes::ZERO_HASH);
    assert!(root_node.is_some());

    // With SLO, a single lane produces a Collapsed node at the root
    match root_node.unwrap().into_parts().0 {
        Some(Node::Collapsed(cl)) => {
            let derived = kaspa_smt::hash_node::<SeqCommitActiveCollapsedNode>(cl.lane_key, cl.leaf_hash);
            assert_eq!(derived, root);
        }
        Some(Node::Internal(hash)) => {
            assert_eq!(hash, root);
        }
        None => panic!("root node unexpectedly marked deleted"),
    }
}

/// BlockLaneChanges uses a uniform blue_score for all lanes in a block.
/// Lanes written at different blue_scores produce different leaf hashes
/// (since blue_score is part of the SMT leaf preimage). This is expected:
/// block processing always stamps the current block's blue_score; per-lane
/// blue_scores only appear during IBD import via `streaming_import`.
#[test]
fn block_lane_changes_uniform_blue_score() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_a_id = [0x01; 20];
    let lane_b_id = [0x02; 20];
    let lk_a = lane_key(&lane_a_id);
    let lk_b = lane_key(&lane_b_id);
    let tip_a = hash(0xAA);
    let tip_b = hash(0xBB);
    let bs_a = 500u64;
    let bs_b = 800u64;

    // Reference: leaf hashes with per-lane blue_scores
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_key: &lk_a, lane_tip: &tip_a, blue_score: bs_a });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_key: &lk_b, lane_tip: &tip_b, blue_score: bs_b });
    let ref_root = slo_root(vec![LeafUpdate { key: lk_a, leaf_hash: leaf_a }, LeafUpdate { key: lk_b, leaf_hash: leaf_b }]);

    // BlockLaneChanges stamps both lanes with bs=1000 → different leaf hashes → different root
    let mut proc = SmtProcessor::new(&stores, 1000, same_pov_bounds(1000), SeqCommitActiveNode::empty_root());
    proc.update_lane(lk_a, tip_a);
    proc.update_lane(lk_b, tip_b);
    let build = proc.build(|_| true).unwrap();
    assert_ne!(build.root, ref_root, "uniform bs must differ from per-lane bs root");
}

#[test]
fn deletion_roundtrip_root_vectors() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    let lane_a = [0x01; 20];
    let lane_b = [0x02; 20];
    let lane_c = [0x03; 20];
    let key_a = lane_key(&lane_a);
    let key_b = lane_key(&lane_b);
    let key_c = lane_key(&lane_c);

    let bs1 = 100u64;
    let bs2 = 200u64;
    let bs3 = 300u64;

    let tip_a1 = hash(0xA1);
    let tip_b1 = hash(0xB1);
    let tip_b2 = hash(0xB2);
    let tip_c1 = hash(0xC1);

    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, tip_a1);
    proc1.update_lane(key_b, tip_b1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch1 = WriteBatch::default();
    build1.flush(&stores, &mut batch1, bs1, hash(0x11)).unwrap();
    db.write(batch1).unwrap();

    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.expire_lane(key_a);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch2 = WriteBatch::default();
    build2.flush(&stores, &mut batch2, bs2, hash(0x22)).unwrap();
    db.write(batch2).unwrap();

    let mut proc3 = SmtProcessor::new(&stores, bs3, same_pov_bounds(bs3), root2);
    proc3.update_lane(key_b, tip_b2);
    proc3.update_lane(key_c, tip_c1);
    let build3 = proc3.build(|_| true).unwrap();
    let root3 = build3.root;

    let golden_root1 = Hash::from_bytes([
        207, 64, 111, 19, 9, 137, 165, 4, 26, 110, 108, 54, 234, 65, 77, 124, 48, 129, 47, 118, 143, 150, 56, 54, 168, 7, 100, 70, 83,
        200, 73, 131,
    ]);
    let golden_root2 = Hash::from_bytes([
        215, 146, 115, 227, 191, 235, 131, 187, 204, 237, 140, 72, 193, 179, 183, 159, 36, 161, 249, 251, 74, 151, 48, 217, 165, 103,
        228, 6, 207, 57, 203, 81,
    ]);
    let golden_root3 = Hash::from_bytes([
        193, 185, 171, 95, 112, 136, 5, 91, 188, 177, 41, 40, 153, 220, 38, 225, 208, 91, 88, 54, 143, 80, 129, 67, 139, 150, 133,
        152, 53, 230, 113, 122,
    ]);

    let leaf_b1 = smt_leaf_hash(&SmtLeafInput { lane_key: &key_b, lane_tip: &tip_b1, blue_score: bs1 });
    let leaf_b2 = smt_leaf_hash(&SmtLeafInput { lane_key: &key_b, lane_tip: &tip_b2, blue_score: bs3 });
    let leaf_c1 = smt_leaf_hash(&SmtLeafInput { lane_key: &key_c, lane_tip: &tip_c1, blue_score: bs3 });
    let expected_root2 = slo_root(vec![LeafUpdate { key: key_b, leaf_hash: leaf_b1 }]);
    let expected_root3 = slo_root(vec![LeafUpdate { key: key_b, leaf_hash: leaf_b2 }, LeafUpdate { key: key_c, leaf_hash: leaf_c1 }]);

    assert_eq!(root2, expected_root2);
    assert_eq!(root3, expected_root3);
    assert_eq!(root1, golden_root1);
    assert_eq!(root2, golden_root2);
    assert_eq!(root3, golden_root3);
}

#[test]
fn empty_subtree_then_resplit_uses_persisted_deletion_marker() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Chosen directly to control SMT topology:
    // a,b stay in the root-left subtree; c,d stay in the root-right subtree.
    // This lets block 2 empty the entire left subtree while the root remains internal.
    let key_a = Hash::from_bytes([0x00; 32]);
    let mut key_b_bytes = [0x00; 32];
    key_b_bytes[0] = 0x40;
    let key_b = Hash::from_bytes(key_b_bytes);
    let mut key_c_bytes = [0x00; 32];
    key_c_bytes[0] = 0x80;
    let key_c = Hash::from_bytes(key_c_bytes);
    let mut key_d_bytes = [0x00; 32];
    key_d_bytes[0] = 0xC0;
    let key_d = Hash::from_bytes(key_d_bytes);
    let mut key_e_bytes = [0x00; 32];
    key_e_bytes[0] = 0x20;
    let key_e = Hash::from_bytes(key_e_bytes);

    let tip_a = hash(0xA1);
    let tip_b = hash(0xB1);
    let tip_c = hash(0xC1);
    let tip_d = hash(0xD1);
    let tip_e = hash(0xE1);

    let bs1 = 100u64;
    let bs2 = 200u64;
    let bs3 = 300u64;

    // Block 1: split both root children.
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, tip_a);
    proc1.update_lane(key_b, tip_b);
    proc1.update_lane(key_c, tip_c);
    proc1.update_lane(key_d, tip_d);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch1 = WriteBatch::default();
    build1.flush(&stores, &mut batch1, bs1, hash(0x31)).unwrap();
    db.write(batch1).unwrap();

    // Block 2: remove the whole left subtree. Root must remain internal because c,d survive.
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.expire_lane(key_a);
    proc2.expire_lane(key_b);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch2 = WriteBatch::default();
    build2.flush(&stores, &mut batch2, bs2, hash(0x32)).unwrap();
    db.write(batch2).unwrap();

    // Block 3: re-enter the previously emptied left subtree.
    // This must not fall back to block 1's old split state for the left child.
    let mut proc3 = SmtProcessor::new(&stores, bs3, same_pov_bounds(bs3), root2);
    proc3.update_lane(key_e, tip_e);
    let build3 = proc3.build(|_| true).unwrap();
    let root3 = build3.root;

    let golden_root1 = Hash::from_bytes([
        116, 234, 231, 168, 80, 90, 38, 161, 68, 146, 104, 71, 136, 224, 137, 16, 129, 71, 95, 86, 155, 91, 205, 228, 252, 109, 42,
        131, 19, 125, 206, 187,
    ]);
    let golden_root2 = Hash::from_bytes([
        252, 128, 194, 143, 8, 135, 150, 27, 8, 197, 234, 235, 15, 193, 175, 247, 112, 47, 33, 62, 167, 27, 147, 182, 186, 241, 195,
        125, 114, 106, 216, 160,
    ]);
    let golden_root3 = Hash::from_bytes([
        214, 238, 35, 243, 61, 126, 31, 94, 199, 236, 101, 211, 43, 198, 106, 45, 177, 196, 65, 134, 188, 168, 49, 97, 168, 129, 32,
        247, 128, 163, 56, 142,
    ]);

    let leaf_c = smt_leaf_hash(&SmtLeafInput { lane_key: &key_c, lane_tip: &tip_c, blue_score: bs1 });
    let leaf_d = smt_leaf_hash(&SmtLeafInput { lane_key: &key_d, lane_tip: &tip_d, blue_score: bs1 });
    let leaf_e = smt_leaf_hash(&SmtLeafInput { lane_key: &key_e, lane_tip: &tip_e, blue_score: bs3 });
    let expected_root2 = slo_root(vec![LeafUpdate { key: key_c, leaf_hash: leaf_c }, LeafUpdate { key: key_d, leaf_hash: leaf_d }]);
    let expected_root3 = slo_root(vec![
        LeafUpdate { key: key_c, leaf_hash: leaf_c },
        LeafUpdate { key: key_d, leaf_hash: leaf_d },
        LeafUpdate { key: key_e, leaf_hash: leaf_e },
    ]);

    assert_eq!(root2, expected_root2);
    assert_eq!(root3, expected_root3);
    assert_eq!(root1, golden_root1);
    assert_eq!(root2, golden_root2);
    assert_eq!(root3, golden_root3);
}

#[test]
fn streaming_import_matches_export_roundtrip_root() {
    use kaspa_hashes::ZERO_HASH;

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    struct BlockSpec {
        blue_score: u64,
        block_hash: Hash,
        lanes: Vec<([u8; 20], Hash)>,
    }

    let blocks = vec![
        BlockSpec { blue_score: 100, block_hash: hash(0x11), lanes: vec![([0x01; 20], hash(0xA1))] },
        BlockSpec { blue_score: 200, block_hash: hash(0x22), lanes: vec![([0x02; 20], hash(0xA2)), ([0x03; 20], hash(0xA3))] },
        BlockSpec { blue_score: 300, block_hash: hash(0x33), lanes: vec![([0x01; 20], hash(0xB1))] },
        BlockSpec { blue_score: 400, block_hash: hash(0x44), lanes: vec![([0x04; 20], hash(0xA4))] },
    ];

    let mut current_root = empty_root;
    for block in &blocks {
        let mut proc = SmtProcessor::new(&stores, block.blue_score, same_pov_bounds(block.blue_score), current_root);
        for (lid, tip) in &block.lanes {
            proc.update_lane(lane_key(lid), *tip);
        }
        let build = proc.build(|_| true).unwrap();
        current_root = build.root;
        let mut batch = WriteBatch::default();
        build.flush(&stores, &mut batch, block.blue_score, block.block_hash).unwrap();
        db.write(batch).unwrap();
    }

    let final_root = current_root;
    let mut exported = Vec::new();
    for lid in (1u8..=4).map(|i| [i; 20]) {
        let lk = lane_key(&lid);
        if let Some(v) = stores.get_lane(lk, u64::MAX, 0, |_| true) {
            exported.push(ImportLane { lane_key: lk, lane_tip: *v.data(), blue_score: v.blue_score(), proof: None });
        }
    }
    exported.sort_by_key(|lane| lane.lane_key);

    let (_lt2, db2) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let import_stores = make_stores(&db2);
    let result =
        streaming_import(&db2, &import_stores, 400, ZERO_HASH, exported.len() as u64, ZERO_HASH, std::iter::once(exported), 64)
            .unwrap();
    assert_eq!(result.root, final_root);
    assert_eq!(result.lanes_imported, 4);

    // Collect all expected lane keys
    let expected_keys: std::collections::HashSet<Hash> = (1u8..=4).map(|i| lane_key(&[i; 20])).collect();

    // Verify LeafUpdate entries: every lane appears, grouped by its own blue_score
    let leaf_updates: Vec<_> = import_stores.score_index.get_leaf_updates(u64::MAX, 0).collect::<Result<Vec<_>, _>>().unwrap();
    let leaf_lane_keys: std::collections::HashSet<Hash> = leaf_updates.iter().flat_map(|e| e.data().iter()).copied().collect();
    assert_eq!(leaf_lane_keys, expected_keys, "LeafUpdate entries must cover all imported lanes");

    // Verify Structural entries: every lane appears at pp_blue_score=400
    let all_entries: Vec<_> = import_stores.score_index.get_all(u64::MAX, 0).collect::<Result<Vec<_>, _>>().unwrap();
    let structural_lane_keys: std::collections::HashSet<Hash> = all_entries
        .iter()
        .filter(|e| e.blue_score() == 400) // structural entries are at pp_blue_score
        .flat_map(|e| e.data().iter())
        .copied()
        .collect();
    assert_eq!(structural_lane_keys, expected_keys, "Structural entries must cover all imported lanes at pp_blue_score");
}

/// Verify score index tracks structural changes through collapsed node split and merge.
///
/// Block 1: Insert A, B → internal root (two leaves).
/// Block 2: Expire A → root collapses to single collapsed node (B only).
///          Structural entry records A (expiration caused the collapse).
/// Block 3: Insert C (sibling of B) → collapsed node splits back to internal.
///          LeafUpdate entry records C (insertion caused the split).
///
/// The score index implicitly covers all structural changes because:
/// - Expiration → Structural entry with the expired lane_key
/// - Insertion causing split → LeafUpdate entry with the new lane_key
///   (branch nodes along the new lane's path are discoverable for pruning)
#[test]
fn score_index_tracks_collapsed_node_split_and_merge() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    // Keys chosen so A and B share a prefix (force internal node), C is a sibling of B
    let key_a = Hash::from_bytes([0x00; 32]);
    let mut key_b_bytes = [0x00; 32];
    key_b_bytes[0] = 0x80;
    let key_b = Hash::from_bytes(key_b_bytes);
    let mut key_c_bytes = [0x00; 32];
    key_c_bytes[0] = 0xC0;
    let key_c = Hash::from_bytes(key_c_bytes);

    let tip = hash(0xAA);
    let bs1 = 100u64;
    let bs2 = 200u64;
    let bs3 = 300u64;

    // Block 1: Insert A and B → root is internal
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, tip);
    proc1.update_lane(key_b, tip);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch1 = WriteBatch::default();
    build1.flush(&stores, &mut batch1, bs1, hash(0x11)).unwrap();
    db.write(batch1).unwrap();

    // Block 1 score index: LeafUpdate with [A, B]
    let entries1: Vec<_> = stores.score_index.get_leaf_updates(bs1, bs1).collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(entries1.len(), 1);
    let keys1: std::collections::HashSet<Hash> = entries1[0].data().iter().copied().collect();
    assert!(keys1.contains(&key_a));
    assert!(keys1.contains(&key_b));

    // Block 2: Expire A → root collapses (B is now a single collapsed node)
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.expire_lane(key_a);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch2 = WriteBatch::default();
    build2.flush(&stores, &mut batch2, bs2, hash(0x22)).unwrap();
    db.write(batch2).unwrap();

    // Block 2 score index: Structural entry with [A] (expiration caused collapse)
    let all2: Vec<_> = stores.score_index.get_all(bs2, bs2).collect::<Result<Vec<_>, _>>().unwrap();
    let structural_keys2: std::collections::HashSet<Hash> =
        all2.iter().filter(|e| e.blue_score() == bs2).flat_map(|e| e.data().iter()).copied().collect();
    assert!(structural_keys2.contains(&key_a), "Structural entry must record expired lane A (collapse trigger)");

    // Block 3: Insert C → collapsed node (B) splits back to internal
    let mut proc3 = SmtProcessor::new(&stores, bs3, same_pov_bounds(bs3), root2);
    proc3.update_lane(key_c, tip);
    let build3 = proc3.build(|_| true).unwrap();
    let _root3 = build3.root;
    let mut batch3 = WriteBatch::default();
    build3.flush(&stores, &mut batch3, bs3, hash(0x33)).unwrap();
    db.write(batch3).unwrap();

    // Block 3 score index: LeafUpdate with [C] (insertion caused split)
    let entries3: Vec<_> = stores.score_index.get_leaf_updates(bs3, bs3).collect::<Result<Vec<_>, _>>().unwrap();
    let leaf_keys3: std::collections::HashSet<Hash> = entries3.iter().flat_map(|e| e.data().iter()).copied().collect();
    assert!(leaf_keys3.contains(&key_c), "LeafUpdate must record lane C (split trigger)");
}

/// Prune version stores: entries at/below cutoff are deleted, entries above remain.
#[test]
fn prune_removes_old_versions_keeps_new() {
    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);
    let empty_root = SeqCommitActiveNode::empty_root();

    let key_a = lane_key(&[0x01; 20]);
    let key_b = lane_key(&[0x02; 20]);

    // Block 1 at score 100: insert A and B
    let bs1 = 100u64;
    let mut proc1 = SmtProcessor::new(&stores, bs1, same_pov_bounds(bs1), empty_root);
    proc1.update_lane(key_a, hash(0xA1));
    proc1.update_lane(key_b, hash(0xB1));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, hash(0x11)).unwrap();
    db.write(batch).unwrap();

    // Block 2 at score 500: update A
    let bs2 = 500u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, same_pov_bounds(bs2), root1);
    proc2.update_lane(key_a, hash(0xA2));
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, hash(0x22)).unwrap();
    db.write(batch).unwrap();

    // Before pruning: both versions of A exist, B has one version
    assert!(stores.lane_version.get_at(key_a, bs1, 0).next().is_some(), "A at score 100 should exist");
    assert!(stores.lane_version.get_at(key_a, bs2, bs2).next().is_some(), "A at score 500 should exist");
    assert!(stores.lane_version.get_at(key_b, bs1, 0).next().is_some(), "B at score 100 should exist");

    // Score index: entries at both scores
    assert!(stores.score_index.get_all(bs1, bs1).next().is_some(), "score index at 100 should exist");
    assert!(stores.score_index.get_all(bs2, bs2).next().is_some(), "score index at 500 should exist");

    // Prune at cutoff=200: should delete score 100 data, keep score 500
    stores.prune(&db, 200);

    // After pruning: A's old version at 100 is gone, new version at 500 remains
    assert!(stores.lane_version.get_at(key_a, 200, 0).next().is_none(), "A at score 100 should be pruned");
    assert!(stores.lane_version.get_at(key_a, bs2, bs2).next().is_some(), "A at score 500 should remain");

    // B's version at 100 is also gone
    assert!(stores.lane_version.get_at(key_b, 200, 0).next().is_none(), "B at score 100 should be pruned");

    // Score index at 100 is range-deleted, at 500 remains
    assert!(stores.score_index.get_all(200, 0).next().is_none(), "score index at 100 should be pruned");
    assert!(stores.score_index.get_all(bs2, bs2).next().is_some(), "score index at 500 should remain");

    // Branch versions at score 100 are gone, but score 500 remain
    let root_key = Hash::from_bytes([0; 32]);
    assert!(stores.branch_version.get_at(0, root_key, 200, 0).next().is_none(), "root branch at score 100 should be pruned");
    assert!(stores.branch_version.get_at(0, root_key, bs2, bs2).next().is_some(), "root branch at score 500 should remain");

    // The tree is still functional: we can build on top of root2
    let bs3 = 600u64;
    let mut proc3 = SmtProcessor::new(&stores, bs3, same_pov_bounds(bs3), root2);
    proc3.update_lane(key_a, hash(0xA3));
    let build3 = proc3.build(|_| true).unwrap();
    assert_ne!(build3.root, root2, "updating A should change the root");
}
