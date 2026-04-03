//! Proving a single lane's activity across a chain of blocks.
//!
//! A verifier who knows only the final `seq_commit` from a block header
//! can verify the exact transactions a lane processed across N blocks.
//!
//! ## Verification pipeline
//!
//! 1. **`compute_lane_tip`** — chain `lane_tip_next` from `parent_ref`
//!    through each block's transactions and context (the recursive core).
//!
//! 2. **`compute_seq_commit_for_lane`** — from the computed tip, derive
//!    the `seq_commit` implied by the SMT proof and block decomposition:
//!    `smt_leaf → proof.compute_root → lanes_root → state_root → seq_commit`.
//!
//! 3. **`verify_lane_activity`** — calls 1 and 2, then checks the result
//!    against the expected `seq_commit` from the block header.

use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_consensus_core::tx::Transaction;
use kaspa_hashes::{Hash, SeqCommitActiveNode};
use kaspa_seq_commit::hashing::*;
use kaspa_seq_commit::types::*;
use kaspa_smt::proof::OwnedSmtProof;
use kaspa_smt::tree::SparseMerkleTree;

type Smt = SparseMerkleTree<SeqCommitActiveNode>;

/// A single transaction's identity within a block's accepted tx list.
struct TxActivity {
    tx_id: Hash,
    version: u16,
    merge_idx: u32,
}

/// Per-block lane activity — transactions and context fields.
struct BlockActivity {
    txs: Vec<TxActivity>,
    timestamp: u64,
    daa_score: u64,
    blue_score: u64,
}

/// Witness for computing the `seq_commit` from a lane tip.
struct CommitmentWitness {
    payload_and_ctx_digest: Hash,
    parent_seq_commit: Hash,
    smt_proof: OwnedSmtProof,
    blue_score: u64,
}

/// Chain `lane_tip_next` across blocks, starting from `parent_ref`.
fn compute_lane_tip(parent_ref: &Hash, lane_key: &Hash, blocks: &[BlockActivity]) -> Hash {
    let mut tip = *parent_ref;
    for block in blocks {
        let mut builder = ActivityDigestBuilder::new();
        for tx in &block.txs {
            let digest = seq_commit_tx_digest(&tx.tx_id, tx.version);
            builder.add_leaf(activity_leaf(&digest, tx.merge_idx));
        }
        let ctx_hash = mergeset_context_hash(&MergesetContext {
            timestamp: block.timestamp,
            daa_score: block.daa_score,
            blue_score: block.blue_score,
        });
        tip =
            lane_tip_next(&LaneTipInput { parent_ref: &tip, lane_key, activity_digest: &builder.finalize(), context_hash: &ctx_hash });
    }
    tip
}

/// Compute `seq_commit` from a lane tip: `smt_leaf → proof.compute_root
/// → lanes_root → state_root → seq_commit`.
fn compute_seq_commit_for_lane(lane_key: &Hash, lane_tip: &Hash, witness: &CommitmentWitness) -> Hash {
    let leaf = smt_leaf_hash(&SmtLeafInput { lane_key, lane_tip, blue_score: witness.blue_score });
    let lanes_root = witness.smt_proof.as_proof().compute_root::<SeqCommitActiveNode>(lane_key, Some(leaf)).unwrap();
    let state_root = seq_state_root(&SeqState { lanes_root: &lanes_root, payload_and_ctx_digest: &witness.payload_and_ctx_digest });
    seq_commit(&SeqCommitInput { parent_seq_commit: &witness.parent_seq_commit, state_root: &state_root })
}

/// Verify a lane's activity chain against the expected `seq_commit`.
fn verify_lane_activity(
    parent_ref: &Hash,
    lane_key: &Hash,
    blocks: &[BlockActivity],
    witness: &CommitmentWitness,
    expected_seq_commit: Hash,
) -> bool {
    let tip = compute_lane_tip(parent_ref, lane_key, blocks);
    compute_seq_commit_for_lane(lane_key, &tip, witness) == expected_seq_commit
}

fn build_commitment(
    smt: &Smt,
    parent_seq_commit: &Hash,
    context: &MergesetContext,
    block_hash: &Hash,
    blue_work: u64,
    coinbase: &[u8],
) -> (Hash, Hash, Hash) {
    let ctx = mergeset_context_hash(context);
    let bw = blue_work.to_le_bytes();
    let pl = miner_payload_leaf(&MinerPayloadLeafInput { block_hash, blue_work_bytes: &bw, payload: coinbase });
    let payload_root = miner_payload_root(core::iter::once(pl));
    let lanes_root = smt.root();
    let pd = payload_and_context_digest(&ctx, &payload_root);
    let sr = seq_state_root(&SeqState { lanes_root: &lanes_root, payload_and_ctx_digest: &pd });
    let sc = seq_commit(&SeqCommitInput { parent_seq_commit, state_root: &sr });
    (sc, lanes_root, pd)
}

fn build_chain(n: usize) -> (Hash, Hash, Hash, Vec<BlockActivity>, CommitmentWitness) {
    assert!(n >= 2);

    let subnet_x = SubnetworkId::from_byte(42);
    let lane_x = subnet_x.into_bytes();
    let key_x = lane_key(&lane_x);

    let subnet_y = SubnetworkId::from_byte(99);
    let lane_y = subnet_y.into_bytes();
    let key_y = lane_key(&lane_y);

    let grandparent_commit = Hash::from_bytes([0xAA; 32]);
    let base_blue_score = 50_000u64;

    let mut smt = Smt::new();
    let mut block_activities = Vec::with_capacity(n);
    let mut prev_seq_commit = grandparent_commit;
    let mut prev_tip_x = grandparent_commit;

    let mut last_sc = Hash::default();
    let mut last_pd = Hash::default();
    let mut last_parent_sc = Hash::default();
    let mut last_blue_score = 0u64;
    let mut last_smt_proof = None;

    for i in 0..n {
        let blue_score = base_blue_score + i as u64;
        let ctx = MergesetContext { timestamp: 1_700_000_000 + (i as u64) * 10, daa_score: 100_000 + i as u64, blue_score };
        let ctx_hash = mergeset_context_hash(&ctx);

        let txs: Vec<Transaction> =
            (0..=i).map(|j| Transaction::new(0, vec![], vec![], 0, subnet_x, 0, vec![i as u8, j as u8])).collect();

        let mut merge_idx: u32 = 0;
        let mut tx_activities = Vec::new();
        let mut builder = ActivityDigestBuilder::new();
        for tx in &txs {
            let tx_id = tx.id();
            let digest = seq_commit_tx_digest(&tx_id, tx.version);
            builder.add_leaf(activity_leaf(&digest, merge_idx));
            tx_activities.push(TxActivity { tx_id, version: tx.version, merge_idx });
            merge_idx += 1;
        }
        let ad_x = builder.finalize();

        let tip_x = lane_tip_next(&LaneTipInput {
            parent_ref: &prev_tip_x,
            lane_key: &key_x,
            activity_digest: &ad_x,
            context_hash: &ctx_hash,
        });

        if i == 0 {
            let y_tx = Transaction::new(0, vec![], vec![], 0, subnet_y, 0, vec![0xFF]);
            let y_digest = seq_commit_tx_digest(&y_tx.id(), y_tx.version);
            let y_leaf = activity_leaf(&y_digest, merge_idx);
            let ad_y = activity_digest_lane(core::iter::once(y_leaf));
            let tip_y = lane_tip_next(&LaneTipInput {
                parent_ref: &grandparent_commit,
                lane_key: &key_y,
                activity_digest: &ad_y,
                context_hash: &ctx_hash,
            });
            smt.insert(key_y, smt_leaf_hash(&SmtLeafInput { lane_key: &key_y, lane_tip: &tip_y, blue_score }));
        }

        smt.insert(key_x, smt_leaf_hash(&SmtLeafInput { lane_key: &key_x, lane_tip: &tip_x, blue_score }));

        let block_hash = {
            let mut b = [0u8; 32];
            b[0] = (i + 1) as u8;
            Hash::from_bytes(b)
        };
        let (sc, _, pd) =
            build_commitment(&smt, &prev_seq_commit, &ctx, &block_hash, 40_000 + i as u64, format!("pool-{i}").as_bytes());

        block_activities.push(BlockActivity {
            txs: tx_activities,
            timestamp: ctx.timestamp,
            daa_score: ctx.daa_score,
            blue_score: ctx.blue_score,
        });

        last_sc = sc;
        last_pd = pd;
        last_parent_sc = prev_seq_commit;
        last_blue_score = blue_score;
        last_smt_proof = Some(smt.prove(&key_x).unwrap());

        prev_seq_commit = sc;
        prev_tip_x = tip_x;
    }

    let witness = CommitmentWitness {
        payload_and_ctx_digest: last_pd,
        parent_seq_commit: last_parent_sc,
        smt_proof: last_smt_proof.unwrap(),
        blue_score: last_blue_score,
    };

    (last_sc, key_x, grandparent_commit, block_activities, witness)
}

fn run_chain_test(n: usize) {
    let (expected_sc, lane_key, parent_ref, blocks, witness) = build_chain(n);

    assert!(verify_lane_activity(&parent_ref, &lane_key, &blocks, &witness, expected_sc));

    // Negative: tampered activity → wrong tip → verification fails
    let mid = blocks.len() / 2;
    let mut bad_blocks: Vec<BlockActivity> = blocks
        .iter()
        .map(|b| BlockActivity {
            txs: b.txs.iter().map(|t| TxActivity { tx_id: t.tx_id, version: t.version, merge_idx: t.merge_idx }).collect(),
            timestamp: b.timestamp,
            daa_score: b.daa_score,
            blue_score: b.blue_score,
        })
        .collect();
    bad_blocks[mid].txs[0].tx_id = Hash::from_bytes([0xFF; 32]);
    assert!(!verify_lane_activity(&parent_ref, &lane_key, &bad_blocks, &witness, expected_sc));

    // Negative: wrong header commitment
    assert!(!verify_lane_activity(&parent_ref, &lane_key, &blocks, &witness, Hash::from_bytes([0xCC; 32])));
}

#[test]
fn chain_2_blocks() {
    run_chain_test(2);
}

#[test]
fn chain_5_blocks() {
    run_chain_test(5);
}

#[test]
fn chain_10_blocks() {
    run_chain_test(10);
}
