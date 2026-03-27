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
    SmtStores::new(db.clone(), 1, 1)
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

    let l1 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x01; 20], lane_tip: &hash(0xA1), blue_score: 100 });
    let l2 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x02; 20], lane_tip: &hash(0xA2), blue_score: 100 });
    let l3 = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x03; 20], lane_tip: &hash(0xA3), blue_score: 100 });

    tree.insert(k1, l1);
    tree.insert(k2, l2);
    tree.insert(k3, l3);
    let root_3 = tree.root();

    // Update l1 to new value
    let l1_new = smt_leaf_hash(&SmtLeafInput { lane_id: &[0x01; 20], lane_tip: &hash(0xB1), blue_score: 200 });
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
    let mut proc = SmtProcessor::new(&stores, blue_score, TEST_THRESHOLD, empty_root);
    proc.update_lane(key_a, lane_id_a, tip_a);
    proc.update_lane(key_b, lane_id_b, tip_b);
    let build = proc.build(|_| true).unwrap();
    let proc_root = build.root;

    // Same via in-memory SMT
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let leaf_a = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_a, lane_tip: &tip_a, blue_score });
    let leaf_b = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id_b, lane_tip: &tip_b, blue_score });
    smt.insert(key_a, leaf_a);
    smt.insert(key_b, leaf_b);
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, TEST_THRESHOLD, empty_root);
    proc1.update_lane(key_a, lane_id_a, tip_a1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: insert lane B (lane A should be read from DB)
    let tip_b = hash(0xB1);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, TEST_THRESHOLD, root1);
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
    smt.insert(key_a, leaf_a);
    smt.insert(key_b, leaf_b);

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
    let mut proc1 = SmtProcessor::new(&stores, bs1, TEST_THRESHOLD, empty_root);
    proc1.update_lane(key, lane_id, tip1);
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: update same lane with new tip
    let tip2 = hash(0xB2);
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, TEST_THRESHOLD, root1);
    proc2.update_lane(key, lane_id, tip2);
    let build2 = proc2.build(|_| true).unwrap();
    let root2 = build2.root;
    let mut batch = WriteBatch::default();
    build2.flush(&stores, &mut batch, bs2, block2_hash).unwrap();
    db.write(batch).unwrap();

    // Verify against in-memory SMT with only the final state
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    let leaf2 = smt_leaf_hash(&SmtLeafInput { lane_id: &lane_id, lane_tip: &tip2, blue_score: bs2 });
    smt.insert(key, leaf2);

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

    let mut proc = SmtProcessor::new(&stores, blue_score, TEST_THRESHOLD, empty_root);
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
    let si_entry = stores.score_index.get_updated(blue_score, 0).next().unwrap().unwrap();
    assert_eq!(si_entry.block_hash(), block_hash);
    assert_eq!(si_entry.data(), &vec![key]);

    // Verify root-level branch version was written
    let root_branch = stores.branch_version.get(255, Hash::from_bytes([0; 32]), 0, |_| true).unwrap();
    assert!(root_branch.is_some());
}

/// Inactivity threshold — lane not touched within threshold -> treated as empty.
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
    let mut proc = SmtProcessor::new(&stores, old_blue_score, TEST_THRESHOLD, empty_root);
    proc.update_lane(key, lane_id, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, old_blue_score, block_hash).unwrap();
    db.write(batch).unwrap();

    // At blue_score=100: the root-level branch should be visible (min_blue_score=0)
    let root_branch = stores.branch_version.get(255, Hash::from_bytes([0; 32]), 0, |_| true).unwrap();
    assert!(root_branch.is_some(), "branch should be visible at same blue_score");

    // At blue_score=100 + THRESHOLD + 1: beyond inactivity window
    let far_future_min = (old_blue_score + TEST_THRESHOLD + 1).saturating_sub(TEST_THRESHOLD);
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

    let mut proc = SmtProcessor::new(&stores, blue_score, TEST_THRESHOLD, empty_root);
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, TEST_THRESHOLD, empty_root);
    proc1.update_lane(key, lane_id, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1_hash).unwrap();
    db.write(batch).unwrap();

    assert_ne!(root1, empty_root);

    // Block 2: expire the lane
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, TEST_THRESHOLD, root1);
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, TEST_THRESHOLD, empty_root);
    proc1.update_lane(key, lane_id, hash(0xAA));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block_hash).unwrap();
    db.write(batch).unwrap();

    // Block 2: no updates, no expirations
    let bs2 = 200u64;
    let proc2 = SmtProcessor::new(&stores, bs2, TEST_THRESHOLD, root1);
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

    let mut proc = SmtProcessor::new(&stores, bs, TEST_THRESHOLD, empty_root);
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
    let mut proc1 = SmtProcessor::new(&stores, bs1, TEST_THRESHOLD, empty_root);
    proc1.update_lane(key_a, lane_a, hash(0xAA));
    proc1.update_lane(key_b, lane_b, hash(0xBB));
    let build1 = proc1.build(|_| true).unwrap();
    let root1 = build1.root;
    let mut batch = WriteBatch::default();
    build1.flush(&stores, &mut batch, bs1, block1).unwrap();
    db.write(batch).unwrap();

    // Block 2: only touch lane A
    let bs2 = 200u64;
    let mut proc2 = SmtProcessor::new(&stores, bs2, TEST_THRESHOLD, root1);
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
    let mut proc = SmtProcessor::new(&stores, bs, TEST_THRESHOLD, empty_root);
    proc.update_lane(key_a, lane_a, hash(0xAA));
    proc.update_lane(key_b, lane_b, hash(0xBB));
    let build = proc.build(|_| true).unwrap();
    let original_root = build.root;
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, block_hash).unwrap();
    db.write(batch).unwrap();

    // Rebuild with no changes — root should match
    let proc2 = SmtProcessor::new(&stores, bs + 10, TEST_THRESHOLD, original_root);
    let build2 = proc2.build(|_| true).unwrap();
    assert_eq!(build2.root, original_root, "no-change rebuild should produce same root");
}

/// Simulate block-by-block processing, export lane state, import into fresh
/// stores via ImportLaneChanges, and verify the roots match.
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
        let mut proc = SmtProcessor::new(&stores, block.blue_score, TEST_THRESHOLD, current_root);
        for (lid, tip) in &block.lanes {
            proc.update_lane(lane_key(lid), *lid, *tip);
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
        if let Some(v) = stores.get_lane(lk, 0, |_| true) {
            exported.push((*lid, v.data().lane_tip_hash, v.blue_score()));
        }
    }
    assert_eq!(exported.len(), 5, "all 5 lanes should have canonical versions");

    // Import: fresh stores, rebuild via ImportLaneChanges with per-lane blue_scores
    let (_lt2, db2) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let import_stores = make_stores(&db2);

    let pp_blue_score = 500; // latest block's blue_score
    let mut import_proc = SmtProcessor::new_import(&import_stores, pp_blue_score, TEST_THRESHOLD, empty_root);
    for (lid, tip, bs) in &exported {
        import_proc.update_lane(lane_key(lid), *lid, *tip, *bs);
    }
    let import_build = import_proc.build(|_| true).unwrap();
    assert_eq!(import_build.root, final_root, "import must reproduce the same root");

    // Flush import to DB with ZERO_HASH (IBD sentinel)
    let mut batch = WriteBatch::default();
    import_build.flush(&import_stores, &mut batch, pp_blue_score, ZERO_HASH).unwrap();
    db2.write(batch).unwrap();

    // Verify lanes are readable with correct blue_scores
    for (lid, tip, bs) in &exported {
        let lk = lane_key(lid);
        let v = import_stores.get_lane(lk, 0, |bh| bh == ZERO_HASH);
        assert!(v.is_some(), "lane {:02x} must be readable", lid[0]);
        assert_eq!(v.as_ref().unwrap().data().lane_tip_hash, *tip);
        assert_eq!(v.unwrap().blue_score(), *bs);
    }

    // Build the same next block on both original and imported stores — roots must match
    let next_bs = pp_blue_score + 1;
    let new_lane_id = [0xFE; 20];
    let new_tip = hash(0xFE);

    let mut orig_next = SmtProcessor::new(&stores, next_bs, TEST_THRESHOLD, final_root);
    orig_next.update_lane(lane_key(&new_lane_id), new_lane_id, new_tip);
    let orig_next_build = orig_next.build(|_| true).unwrap();

    let mut import_next = SmtProcessor::new(&import_stores, next_bs, TEST_THRESHOLD, final_root);
    import_next.update_lane(lane_key(&new_lane_id), new_lane_id, new_tip);
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

    let mut proc = SmtProcessor::new(&stores, bs, TEST_THRESHOLD, SeqCommitActiveNode::empty_root());
    proc.update_lane(lk, lane_id, tip);
    let build = proc.build(|_| true).unwrap();
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, ZERO_HASH).unwrap();
    db.write(batch).unwrap();

    let result = stores.get_lane(lk, 0, |bh| bh == kaspa_hashes::ZERO_HASH);
    assert!(result.is_some());
    assert_eq!(result.unwrap().data().lane_tip_hash, tip);
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

    let mut proc = SmtProcessor::new(&stores, bs, TEST_THRESHOLD, SeqCommitActiveNode::empty_root());
    proc.update_lane(lk, lane_id, tip);
    let build = proc.build(|_| true).unwrap();
    let root = build.root;
    let mut batch = WriteBatch::default();
    build.flush(&stores, &mut batch, bs, ZERO_HASH).unwrap();
    db.write(batch).unwrap();

    let root_branch = stores.branch_version.get(255, Hash::from_bytes([0; 32]), 0, |bh| bh == kaspa_hashes::ZERO_HASH).unwrap();
    assert!(root_branch.is_some());

    let children = root_branch.unwrap();
    let derived = kaspa_smt::hash_node::<SeqCommitActiveNode>(children.data().left, children.data().right);
    assert_eq!(derived, root);
}

/// ImportLaneChanges produces the correct root when lanes have different blue_scores.
#[test]
fn import_lane_changes_per_lane_blue_score() {
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

    let mut ref_tree = SparseMerkleTree::<SeqCommitActiveNode>::new();
    ref_tree.insert(lk_a, smt_leaf_hash(&SmtLeafInput { lane_id: &lane_a_id, lane_tip: &tip_a, blue_score: bs_a }));
    ref_tree.insert(lk_b, smt_leaf_hash(&SmtLeafInput { lane_id: &lane_b_id, lane_tip: &tip_b, blue_score: bs_b }));
    let expected_root = ref_tree.root();

    // BlockLaneChanges (uniform) produces a different root
    let mut block_proc = SmtProcessor::new(&stores, 1000, TEST_THRESHOLD, SeqCommitActiveNode::empty_root());
    block_proc.update_lane(lk_a, lane_a_id, tip_a);
    block_proc.update_lane(lk_b, lane_b_id, tip_b);
    let block_build = block_proc.build(|_| true).unwrap();
    assert_ne!(block_build.root, expected_root);

    // ImportLaneChanges (per-lane) matches the reference
    let mut import_proc = SmtProcessor::new_import(&stores, 1000, TEST_THRESHOLD, SeqCommitActiveNode::empty_root());
    import_proc.update_lane(lk_a, lane_a_id, tip_a, bs_a);
    import_proc.update_lane(lk_b, lane_b_id, tip_b, bs_b);
    let import_build = import_proc.build(|_| true).unwrap();
    assert_eq!(import_build.root, expected_root);
}
