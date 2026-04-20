//! Example: computing a sequencing commitment for a block.
//!
//! Simulates a selected block B that merges 2 blocks (selected parent + one anticone block).
//! Each merged block has its own transactions and coinbase payload.
//!
//! Key points demonstrated:
//! - Transactions from all merged blocks are collected into a single AcceptedTxList(B),
//!   ordered by mergeset traversal (selected parent first, then by ascending blue_work).
//! - `merge_idx` is a global counter across ALL accepted transactions.
//! - Activity digests are computed once per lane across the entire AcceptedTxList.
//! - `parent_ref` for lane tip: existing lane → previous lane_tip_hash;
//!   new lane → SeqCommit(parent(B)) (the global anchor).
//! - Miner payload: one leaf per merged block in mergeset order.
//! - Active lanes are committed via a 256-bit Sparse Merkle Tree.

use kaspa_consensus_core::BlueWorkType;
use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::Transaction;
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::*;
use kaspa_seq_commit::types::*;
use kaspa_smt::tree::SparseMerkleTree;

/// A merged block with its metadata and accepted transactions.
struct MergedBlock {
    hash: Hash,
    blue_work: u64,
    coinbase_payload: Vec<u8>,
    /// Accepted transactions in block order.
    accepted_txs: Vec<Transaction>,
}

#[test]
fn example_seq_commit_for_block() {
    // --- Two subnetworks (lanes) ---
    let subnet_a = SubnetworkId::from_byte(10);
    let subnet_b = SubnetworkId::from_byte(20);
    let lane_a = subnet_a.into_bytes();
    let lane_b = subnet_b.into_bytes();

    // --- Previous state ---
    // SeqCommit(parent(B)) — the sequencing commitment of B's selected parent.
    // Used as the global anchor for new/reactivated lanes.
    let parent_seq_commit = Hash::from_bytes([0xAA; 32]);

    // Lane A is already active from a previous block — it has an existing tip and blue_score.
    let lane_a_prev_tip = Hash::from_bytes([0xDD; 32]);
    let lane_a_prev_blue_score = 49_990u64;

    // Lane B is new (not in ActiveLanes) — its parent_ref will be parent_seq_commit.

    // --- SMT carries forward from previous state (lane A already present) ---
    let key_a = lane_key(&lane_a);
    let prev_leaf_a = smt_leaf_hash(&SmtLeafInput { lane_tip: &lane_a_prev_tip, blue_score: lane_a_prev_blue_score });
    let mut smt = SparseMerkleTree::<SeqCommitActiveNode>::new();
    smt.insert(key_a, prev_leaf_a);

    // --- Selected parent block: 2 txs on lane A ---
    let selected_parent = MergedBlock {
        hash: Hash::from_bytes([0x11; 32]),
        blue_work: 40_000,
        coinbase_payload: b"pool-alpha".to_vec(),
        accepted_txs: vec![
            Transaction::new(0, vec![], vec![], 0, subnet_a, 0, vec![]),
            Transaction::new(0, vec![], vec![], 0, subnet_a, 0, vec![1, 2, 3]),
        ],
    };

    // --- Anticone block: 1 tx on lane A, 1 tx on lane B ---
    // Higher blue_work → ordered after selected parent in mergeset traversal.
    let anticone_block = MergedBlock {
        hash: Hash::from_bytes([0x22; 32]),
        blue_work: 41_000,
        coinbase_payload: b"pool-beta".to_vec(),
        accepted_txs: vec![
            Transaction::new(0, vec![], vec![], 0, subnet_a, 0, vec![4]),
            Transaction::new(0, vec![], vec![], 0, subnet_b, 0, vec![5, 6]),
        ],
    };

    // --- Mergeset ordering: selected parent first, then by ascending blue_work ---
    let mergeset = [&selected_parent, &anticone_block];

    // --- Collect AcceptedTxList(B): all txs in mergeset order, global merge_idx ---
    let mut merge_idx: u32 = 0;
    let mut lane_a_leaves = Vec::new();
    let mut lane_b_leaves = Vec::new();

    let mut lane_a_activity_builder = ActivityDigestBuilder::new();
    let mut lane_b_activity_builder = ActivityDigestBuilder::new();

    for block in &mergeset {
        for tx in &block.accepted_txs {
            let leaf = activity_leaf(&tx.id(), tx.version, merge_idx);

            let lid = tx.subnetwork_id.into_bytes();
            if lid == lane_a {
                lane_a_activity_builder.add_leaf(leaf);
                lane_a_leaves.push(leaf);
            } else {
                lane_b_activity_builder.add_leaf(leaf);
                lane_b_leaves.push(leaf);
            }
            merge_idx += 1;
        }
    }

    assert_eq!(merge_idx, 4); // 2 from selected parent + 2 from anticone
    assert_eq!(lane_a_leaves.len(), 3); // txs at merge_idx 0, 1, 2
    assert_eq!(lane_b_leaves.len(), 1); // tx at merge_idx 3

    // --- One activity digest per lane (across the entire AcceptedTxList) ---
    let ad_a = activity_digest_lane(lane_a_leaves.into_iter());
    let ad_b = activity_digest_lane(lane_b_leaves.into_iter());
    let ad_streaming_a = lane_a_activity_builder.finalize();
    let ad_streaming_b = lane_b_activity_builder.finalize();
    assert_eq!(ad_a, ad_streaming_a);
    assert_eq!(ad_b, ad_streaming_b);

    // --- Mergeset context (properties of the accepting block B) ---
    let timestamp = 1_700_000_000u64;
    let daa_score = 100_000u64;
    let blue_score = 50_000u64;
    let ctx = mergeset_context_hash(&MergesetContext { timestamp, daa_score, blue_score });

    // --- Lane tips ---
    // Lane A: already active → parent_ref = previous lane_tip_hash
    let tip_a =
        lane_tip_next(&LaneTipInput { parent_ref: &lane_a_prev_tip, lane_key: &key_a, activity_digest: &ad_a, context_hash: &ctx });

    // Lane B: new lane → parent_ref = SeqCommit(parent(B)), the global anchor
    let key_b = lane_key(&lane_b);
    let tip_b =
        lane_tip_next(&LaneTipInput { parent_ref: &parent_seq_commit, lane_key: &key_b, activity_digest: &ad_b, context_hash: &ctx });
    let new_leaf_a = smt_leaf_hash(&SmtLeafInput { lane_tip: &tip_a, blue_score });
    let new_leaf_b = smt_leaf_hash(&SmtLeafInput { lane_tip: &tip_b, blue_score });

    smt.insert(key_a, new_leaf_a); // update existing lane A
    smt.insert(key_b, new_leaf_b); // insert new lane B
    let lanes_root = smt.root();

    // --- Miner payload: one leaf per merged block, in mergeset order ---
    let payload_leaves: Vec<Hash> = mergeset
        .iter()
        .map(|block| {
            let bw = BlueWorkType::from_u64(block.blue_work);
            miner_payload_leaf(MinerPayloadLeafInput {
                block_hash: &block.hash,
                blue_work_bytes: &bw,
                payload: &block.coinbase_payload,
            })
        })
        .collect();
    let payload_root = miner_payload_root(payload_leaves.into_iter());

    // --- State root and final commitment ---
    let pd = kaspa_seq_commit::hashing::payload_and_context_digest(&ctx, &payload_root);
    let state_root = seq_state_root(&SeqState { lanes_root: &lanes_root, payload_and_ctx_digest: &pd });
    let commitment = seq_commit(&SeqCommitInput { parent_seq_commit: &parent_seq_commit, state_root: &state_root });

    // Recomputing with the same inputs yields the same result
    let commitment2 = seq_commit(&SeqCommitInput { parent_seq_commit: &parent_seq_commit, state_root: &state_root });
    assert_eq!(commitment, commitment2);
}
