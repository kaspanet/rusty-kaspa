//! Integration tests for SmtProcessor + ComposedSmtStore + DbSmtView.

use std::sync::Arc;

use kaspa_database::create_temp_db;
use kaspa_database::prelude::{ConnBuilder, DB};
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::{lane_key, smt_leaf_hash};
use kaspa_seq_commit::types::SmtLeafInput;
use kaspa_smt::SmtHasher;
use kaspa_smt::tree::SparseMerkleTree;
use rocksdb::WriteBatch;

use kaspa_smt_store::processor::{SmtProcessor, SmtStores};

fn hash(v: u8) -> Hash {
    Hash::from_bytes([v; 32])
}

fn make_stores(db: &Arc<DB>) -> SmtStores {
    SmtStores::new(db.clone())
}

/// A1/A2: walk_up without remove_branch produces the same roots as insertion-only paths.
#[test]
fn walk_up_always_insert_same_root() {
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();

    let k1 = lane_key(&[0x01; 20]);
    let k2 = lane_key(&[0x02; 20]);
    let k3 = lane_key(&[0x03; 20]);

    let l1 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x01; 20], lane_tip: &hash(0xA1), blue_score: 100 });
    let l2 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x02; 20], lane_tip: &hash(0xA2), blue_score: 100 });
    let l3 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x03; 20], lane_tip: &hash(0xA3), blue_score: 100 });

    tree.insert(k1, l1).unwrap();
    tree.insert(k2, l2).unwrap();
    tree.insert(k3, l3).unwrap();
    let root_3 = tree.root();

    // Update l1 to new value
    let l1_new = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x01; 20], lane_tip: &hash(0xB1), blue_score: 200 });
    tree.insert(k1, l1_new).unwrap();
    let root_updated = tree.root();

    assert_ne!(root_3, root_updated);

    // Build same tree from scratch to verify determinism
    let mut tree2 = SparseMerkleTree::<SeqCommitActiveNode>::new();
    tree2.insert(k1, l1_new).unwrap();
    tree2.insert(k2, l2).unwrap();
    tree2.insert(k3, l3).unwrap();

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
    let mut proc = SmtProcessor::new(&stores, blue_score, empty_root);
    proc.update_lane(key_a, lane_id_a, tip_a);
    proc.update_lane(key_b, lane_id_b, tip_b);
    let build = proc.build(|_| true).unwrap();
    let proc_root = build.root;

    // Same via in-memory SMT
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_a, lane_tip: &tip_a, blue_score });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_b, lane_tip: &tip_b, blue_score });
    smt.insert(key_a, leaf_a).unwrap();
    smt.insert(key_b, leaf_b).unwrap();
    let mem_root = smt.root();

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
    let mut proc1 = SmtProcessor::new(&stores, bs1, empty_root);
    proc1.update_lane(key_a, lane_id_a, tip_a1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: insert lane B (lane A should be read from DB)
    let tip_b = hash(0xB1);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, root1);
    proc2.update_lane(key_b, lane_id_b, tip_b);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // Verify: rebuild from scratch in memory
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_a, lane_tip: &tip_a1, blue_score: bs1 });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_b, lane_tip: &tip_b, blue_score: bs2 });
    smt.insert(key_a, leaf_a).unwrap();
    smt.insert(key_b, leaf_b).unwrap();

    assert_eq!(root2, smt.root());
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, empty_root);
    proc1.update_lane(key, lane_id, tip1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: update same lane with new tip
    let tip2 = hash(0xB2);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, root1);
    proc2.update_lane(key, lane_id, tip2);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // Verify against in-memory SMT with only the final state
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let leaf2 = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id, lane_tip: &tip2, blue_score: bs2 });
    smt.insert(key, leaf2).unwrap();

    assert_eq!(root2, smt.root());
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

    let mut proc = SmtProcessor::new(&stores, blue_score, empty_root);
    proc.update_lane(key, lane_id, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, blue_score, block_hash).unwrap();
    db.write(batch).unwrap();

    // Verify lane version was written
    let lane_ver = stores.lane_version.get(key, 0, |bh| bh == block_hash).unwrap().unwrap();
    assert_eq!(lane_ver.data().lane_id, lane_id);
    assert_eq!(lane_ver.data().lane_tip_hash, tip);
    assert_eq!(lane_ver.blue_score(), blue_score);

    // Verify score index entry
    let si_entry = stores.score_index.get_at(blue_score, 0).next().unwrap().unwrap();
    assert_eq!(si_entry.block_hash(), block_hash);
    assert_eq!(si_entry.data(), &vec![key]);

    // Verify root-level branch version was written
    let root_branch = stores.branch_version.get(255, Hash::from_bytes([0; 32]), 0, |_| true).unwrap();
    assert!(root_branch.is_some());
}

/// Inactivity threshold — lane not touched within threshold -> treated as empty.
#[test]
fn inactivity_threshold_hides_stale_branches() {
    use kaspa_smt_store::LANE_INACTIVITY_THRESHOLD;

    let (_lt, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let stores = make_stores(&db);

    let lane_id = [0x42; 20];
    let key = lane_key(&lane_id);
    let block_hash = hash(0x11);
    let tip = hash(0xAA);
    let old_blue_score = 100u64;
    let empty_root = SeqCommitActiveNode::empty_root();

    // Write a lane at blue_score=100
    let mut proc = SmtProcessor::new(&stores, old_blue_score, empty_root);
    proc.update_lane(key, lane_id, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, old_blue_score, block_hash).unwrap();
    db.write(batch).unwrap();

    // At blue_score=100: the root-level branch should be visible (min_blue_score=0)
    let root_branch = stores.branch_version.get(255, Hash::from_bytes([0; 32]), 0, |_| true).unwrap();
    assert!(root_branch.is_some(), "branch should be visible at same blue_score");

    // At blue_score=100 + THRESHOLD + 1: beyond inactivity window
    let far_future_min = (old_blue_score + LANE_INACTIVITY_THRESHOLD + 1).saturating_sub(LANE_INACTIVITY_THRESHOLD);
    let root_branch_far = stores.branch_version.get(255, Hash::from_bytes([0; 32]), far_future_min, |_| true).unwrap();
    assert!(root_branch_far.is_none(), "branch should be hidden beyond inactivity threshold");
}

/// from_root constructor works with in-memory store.
#[test]
fn from_root_constructor() {
    let mut tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let lid = [0x01; 20];
    let key = lane_key(&lid);
    let leaf = smt_leaf_hash(&SmtLeafInput { lane_id: &lid, lane_tip: &hash(0xAA), blue_score: 100 });
    tree.insert(key, leaf).unwrap();
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

    let mut proc = SmtProcessor::new(&stores, blue_score, empty_root);
    proc.update_lane(lane_key(&[0x01; 20]), [0x01; 20], hash(0xAA));
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, empty_root);
    proc1.update_lane(key, lane_id, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    assert_ne!(root1, empty_root);

    // Block 2: expire the lane
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, root1);
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, empty_root);
    proc1.update_lane(key, lane_id, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: no updates, no expirations
    let bs2 = 200u64;
    let proc2 = SmtProcessor::new(&stores, bs2, root1);
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

    let mut proc = SmtProcessor::new(&stores, bs, empty_root);
    proc.update_lane(lane_key(&[0x01; 20]), [0x01; 20], hash(0xAA));
    let build = proc.build(|_| true).unwrap();

    // A single lane insert walks 256 levels.
    // Each level produces one branch in the diff.
    assert_eq!(build.diff_branch_count(), 256, "single leaf should produce 256 branch changes");
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, empty_root);
    proc1.update_lane(key_a, lane_a, hash(0xAA));
    proc1.update_lane(key_b, lane_b, hash(0xBB));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1).unwrap();
    db.write(batch).unwrap();

    // Block 2: only touch lane A
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, root1);
    proc2.update_lane(key_a, lane_a, hash(0xCC));
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
    let mut proc = SmtProcessor::new(&stores, bs, empty_root);
    proc.update_lane(key_a, lane_a, hash(0xAA));
    proc.update_lane(key_b, lane_b, hash(0xBB));
    let build = proc.build(|_| true).unwrap();
    let original_root = build.root;
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, block_hash).unwrap();
    db.write(batch).unwrap();

    // Rebuild with no changes — root should match
    let proc2 = SmtProcessor::new(&stores, bs + 10, original_root);
    let build2 = proc2.build(|_| true).unwrap();
    assert_eq!(build2.root, original_root, "no-change rebuild should produce same root");
}
