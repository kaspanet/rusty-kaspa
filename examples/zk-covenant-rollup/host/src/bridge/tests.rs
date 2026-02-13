use kaspa_consensus_core::{
    constants::SOMPI_PER_KASPA,
    tx::{CovenantBinding, Transaction, UtxoEntry},
};
use kaspa_hashes::Hash;
use kaspa_txscript::{pay_to_script_hash_script, script_builder::ScriptBuilder, seq_commit_accessor::SeqCommitAccessor};
use zk_covenant_rollup_core::{perm_empty_leaf_hash, perm_leaf_hash, permission_tree::PermissionTree};

use crate::bridge::{build_delegate_entry_script, build_permission_redeem_converged, build_permission_sig_script, ScriptDomain};
use crate::tx::{make_multi_input_mock_transaction, try_verify_tx_input, verify_tx_input};

// ─────────────────────────────────────────────────────────────────
//  Dummy SeqCommitAccessor (permission scripts don't use seq_commit)
// ─────────────────────────────────────────────────────────────────

struct NullAccessor;

impl SeqCommitAccessor for NullAccessor {
    fn is_chain_ancestor_from_pov(&self, _block_hash: Hash) -> Option<bool> {
        None
    }
    fn seq_commitment_within_depth(&self, _block_hash: Hash) -> Option<Hash> {
        None
    }
}

// ─────────────────────────────────────────────────────────────────
//  Test constants
// ─────────────────────────────────────────────────────────────────

const COV_ID_BYTES: [u8; 32] = [0xFF; 32];

fn cov_id() -> Hash {
    Hash::from_bytes(COV_ID_BYTES)
}

/// A simple 34-byte P2PK-like SPK for testing.
fn test_spk_p2pk(seed: u8) -> Vec<u8> {
    let mut spk = vec![0u8; 34];
    spk[0] = 0x20; // OpData32
    spk[1] = seed;
    spk[33] = 0xac; // OpCheckSig
    spk
}

/// A 35-byte P2SH-like SPK for testing.
fn test_spk_p2sh(seed: u8) -> Vec<u8> {
    let mut spk = vec![0u8; 35];
    spk[0] = 0xaa; // OpBlake2b
    spk[1] = 0x20; // OpData32
    spk[2] = seed;
    spk[34] = 0x87; // OpEqual
    spk
}

// ─────────────────────────────────────────────────────────────────
//  Permission test helpers
// ─────────────────────────────────────────────────────────────────

/// Build a complete permission test transaction from tree leaves.
///
/// Returns (tx, utxos, old_redeem) — the old_redeem is needed for delegate tests.
fn build_perm_test_tx(leaves: Vec<(Vec<u8>, u64)>, leaf_idx: usize, deduct: u64) -> (Transaction, Vec<UtxoEntry>, Vec<u8>) {
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;

    let (spk, amount) = tree.get_leaf(leaf_idx).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(leaf_idx);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);

    // Compute new state
    let new_amount = amount - deduct;
    let is_zero = new_amount == 0;
    let new_unclaimed = if is_zero { old_unclaimed - 1 } else { old_unclaimed };
    let is_done = new_unclaimed == 0;

    let new_leaf_hash = if is_zero { perm_empty_leaf_hash() } else { perm_leaf_hash(&spk, new_amount) };
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let id = cov_id();
    let input_spk = pay_to_script_hash_script(&old_redeem);

    let outputs = if is_done {
        vec![]
    } else {
        let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);
        let output_spk = pay_to_script_hash_script(&new_redeem);
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))]
    };

    let (mut tx, utxos) = make_multi_input_mock_transaction(vec![(input_spk, Some(id))], outputs);

    let sig_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);
    tx.inputs[0].signature_script = sig_script;

    (tx, utxos, old_redeem)
}

// ═══════════════════════════════════════════════════════════════════
//  Permission script — happy paths
// ═══════════════════════════════════════════════════════════════════

#[test]
fn partial_deduct_depth1() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 300);
    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    if let Err(ref e) = result {
        eprintln!("partial_deduct_depth1 error: {e}");
    }
    result.unwrap();
}

#[test]
fn partial_deduct_depth2() {
    let leaves =
        vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64), (test_spk_p2pk(3), 2000u64), (test_spk_p2pk(4), 750u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 2, 500);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

#[test]
fn partial_deduct_depth3() {
    let leaves: Vec<(Vec<u8>, u64)> = (0..8).map(|i| (test_spk_p2pk(i as u8 + 10), 1000u64 * (i as u64 + 1))).collect();
    let (tx, utxos, _) = build_perm_test_tx(leaves, 5, 1000);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

#[test]
fn full_deduct_not_last() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 1000);
    // is_zero=true, unclaimed 2→1, continuation exists
    assert_eq!(tx.outputs.len(), 1);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

#[test]
fn full_deduct_last_leaf() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 1000);
    // is_done=true, no continuation output
    assert_eq!(tx.outputs.len(), 0);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

#[test]
fn various_amounts() {
    // Small amount
    let leaves = vec![(test_spk_p2pk(1), 1u64), (test_spk_p2pk(2), 100u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves.clone(), 0, 1);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);

    // Medium amount
    let leaves = vec![(test_spk_p2pk(1), 10000u64), (test_spk_p2pk(2), 100u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 5000);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);

    // Large amount (10^15)
    let leaves = vec![(test_spk_p2pk(1), 1_000_000_000_000_000u64), (test_spk_p2pk(2), 100u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 500_000_000_000_000);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

#[test]
fn various_spk_sizes() {
    // P2PK (34B)
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 300);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);

    // P2SH (35B)
    let leaves = vec![(test_spk_p2sh(1), 1000u64), (test_spk_p2sh(2), 500u64)];
    let (tx, utxos, _) = build_perm_test_tx(leaves, 0, 300);
    verify_tx_input(&tx, &utxos, 0, &NullAccessor);
}

// ═══════════════════════════════════════════════════════════════════
//  Permission script — error cases
// ═══════════════════════════════════════════════════════════════════

#[test]
fn wrong_sibling() {
    let tree = PermissionTree::from_leaves(vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)]);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let mut proof = tree.prove(0);

    // Corrupt sibling
    proof.siblings[0][0] ^= 0xDEAD;

    let deduct = 300u64;
    let new_amount = amount - deduct;
    let new_unclaimed = old_unclaimed; // partial deduct, same unclaimed
    let new_leaf_hash = perm_leaf_hash(&spk, new_amount);
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);
    let output_spk = pay_to_script_hash_script(&new_redeem);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: corrupted sibling");
}

#[test]
fn wrong_amount() {
    let tree = PermissionTree::from_leaves(vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)]);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, _amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    // Provide wrong amount (999 instead of 1000)
    let wrong_amount = 999u64;
    let deduct = 300u64;
    let new_amount = wrong_amount - deduct;
    let new_unclaimed = old_unclaimed;
    let new_leaf_hash = perm_leaf_hash(&spk, new_amount);
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);
    let output_spk = pay_to_script_hash_script(&new_redeem);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, wrong_amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: wrong amount → root mismatch");
}

#[test]
fn deduct_exceeds_amount() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    let deduct = amount + 1; // exceeds
    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    // We still need a valid-looking output for the tx structure, but the script
    // should fail before reaching the output check. Use a dummy output.
    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(
            SOMPI_PER_KASPA,
            pay_to_script_hash_script(&[0u8; 35]),
            Some(CovenantBinding { authorizing_input: 0, covenant_id: id }),
        )],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: deduct > amount → GreaterThanOrEqual fail");
}

#[test]
fn zero_deduct() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    let deduct = 0u64;
    let new_unclaimed = old_unclaimed;
    let new_leaf_hash = perm_leaf_hash(&spk, amount); // amount unchanged
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);
    let output_spk = pay_to_script_hash_script(&new_redeem);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: deduct=0 → GreaterThan fail");
}

#[test]
fn wrong_new_unclaimed() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    // Full deduct: unclaimed should go from 2→1, but we provide 2 (wrong)
    let deduct = amount;
    let wrong_new_unclaimed = old_unclaimed; // should be old_unclaimed - 1
    let new_leaf_hash = perm_empty_leaf_hash();
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let new_redeem = build_permission_redeem_converged(&new_root, wrong_new_unclaimed, depth);
    let output_spk = pay_to_script_hash_script(&new_redeem);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: wrong new_unclaimed in output redeem → P6 SPK mismatch");
}

#[test]
fn wrong_output_spk() {
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    let deduct = 300u64;

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    // Use a wrong output SPK (not matching expected continuation)
    let wrong_output_spk = pay_to_script_hash_script(&[0xDE, 0xAD]);

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, wrong_output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: wrong output SPK → P6 EqualVerify fail");
}

#[test]
fn empty_tree_with_cov_output() {
    // 1 leaf, deduct all → is_done=true
    let leaves = vec![(test_spk_p2pk(1), 1000u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    let deduct = amount;

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);
    let input_spk = pay_to_script_hash_script(&old_redeem);
    let id = cov_id();

    // V3 fix test: is_done but tx has a covenant output (should fail)
    let bogus_output_spk = pay_to_script_hash_script(&[0xBB; 35]);
    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input_spk, Some(id))],
        vec![(SOMPI_PER_KASPA, bogus_output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: is_done but CovOutCount != 0");
}

// ═══════════════════════════════════════════════════════════════════
//  Delegate script tests
// ═══════════════════════════════════════════════════════════════════

/// Build a 2-input tx: permission at input 0, delegate at input 1.
/// Returns (tx, utxos).
fn build_delegate_test_tx(
    perm_cov_id: Hash,
    delegate_cov_id_check: &[u8; 32],
    perm_leaves: Vec<(Vec<u8>, u64)>,
    leaf_idx: usize,
    deduct: u64,
) -> (Transaction, Vec<UtxoEntry>) {
    // Build permission input (input 0)
    let tree = PermissionTree::from_leaves(perm_leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(leaf_idx).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(leaf_idx);

    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);

    let new_amount = amount - deduct;
    let is_zero = new_amount == 0;
    let new_unclaimed = if is_zero { old_unclaimed - 1 } else { old_unclaimed };

    let new_leaf_hash = if is_zero { perm_empty_leaf_hash() } else { perm_leaf_hash(&spk, new_amount) };
    let new_root = proof.compute_new_root(&new_leaf_hash);

    let input0_spk = pay_to_script_hash_script(&old_redeem);

    // Build delegate redeem (input 1)
    let delegate_redeem = build_delegate_entry_script(delegate_cov_id_check);
    let input1_spk = pay_to_script_hash_script(&delegate_redeem);

    // Build continuation output
    let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);
    let output_spk = pay_to_script_hash_script(&new_redeem);

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input0_spk, Some(perm_cov_id)), (input1_spk, None)],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: perm_cov_id }))],
    );

    // Set input 0 sig_script (permission)
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &old_redeem);

    // Set input 1 sig_script (delegate: just push the redeem)
    tx.inputs[1].signature_script = ScriptBuilder::new().add_data(&delegate_redeem).unwrap().drain();

    (tx, utxos)
}

#[test]
fn delegate_happy_path() {
    let id = cov_id();
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let (tx, utxos) = build_delegate_test_tx(id, &COV_ID_BYTES, leaves, 0, 300);
    // Verify delegate (input 1) passes
    verify_tx_input(&tx, &utxos, 1, &NullAccessor);
}

#[test]
fn delegate_wrong_covenant_id() {
    let id = cov_id();
    let wrong_cov_id = [0xAA; 32]; // delegate checks for wrong ID
    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let (tx, utxos) = build_delegate_test_tx(id, &wrong_cov_id, leaves, 0, 300);

    let result = try_verify_tx_input(&tx, &utxos, 1, &NullAccessor);
    assert!(result.is_err(), "should fail: delegate checks wrong covenant ID");
}

#[test]
fn delegate_wrong_domain() {
    // Input 0 uses a state verification domain suffix [0x00, 0x75] instead of
    // permission [0x51, 0x75]. We simulate this by building a "fake" redeem script
    // whose last 2 bytes are the state verification suffix.
    let id = cov_id();

    let leaves = vec![(test_spk_p2pk(1), 1000u64), (test_spk_p2pk(2), 500u64)];
    let tree = PermissionTree::from_leaves(leaves);
    let depth = tree.depth();
    let old_root = tree.root();
    let old_unclaimed = tree.len() as u64;
    let (spk, amount) = tree.get_leaf(0).unwrap();
    let spk = spk.to_vec();
    let proof = tree.prove(0);

    let deduct = 300u64;
    let new_amount = amount - deduct;
    let new_unclaimed = old_unclaimed;

    // Build legitimate permission redeem, then replace domain suffix bytes
    let old_redeem = build_permission_redeem_converged(&old_root, old_unclaimed, depth);

    // Create a fake redeem with state verification domain suffix [0x00, 0x75]
    let mut fake_redeem = old_redeem.clone();
    let len = fake_redeem.len();
    fake_redeem[len - 2] = ScriptDomain::StateVerification.suffix_bytes()[0]; // 0x00 instead of 0x51

    let new_leaf_hash = perm_leaf_hash(&spk, new_amount);
    let new_root = proof.compute_new_root(&new_leaf_hash);
    let new_redeem = build_permission_redeem_converged(&new_root, new_unclaimed, depth);

    // Use fake_redeem for input 0's SPK and sig_script
    let input0_spk = pay_to_script_hash_script(&fake_redeem);

    // Build delegate that checks for the correct covenant ID
    let delegate_redeem = build_delegate_entry_script(&COV_ID_BYTES);
    let input1_spk = pay_to_script_hash_script(&delegate_redeem);

    let output_spk = pay_to_script_hash_script(&new_redeem);

    let (mut tx, utxos) = make_multi_input_mock_transaction(
        vec![(input0_spk, Some(id)), (input1_spk, None)],
        vec![(SOMPI_PER_KASPA, output_spk, Some(CovenantBinding { authorizing_input: 0, covenant_id: id }))],
    );

    // Input 0 sig_script uses the fake_redeem
    tx.inputs[0].signature_script = build_permission_sig_script(&spk, amount, deduct, &proof, &fake_redeem);

    // Input 1 sig_script: delegate
    tx.inputs[1].signature_script = ScriptBuilder::new().add_data(&delegate_redeem).unwrap().drain();

    let result = try_verify_tx_input(&tx, &utxos, 1, &NullAccessor);
    assert!(result.is_err(), "should fail: input 0 has state verification domain, not permission");
}

#[test]
fn delegate_at_index_zero() {
    // Put delegate at input 0 — V4 should reject
    let id = cov_id();

    let delegate_redeem = build_delegate_entry_script(&COV_ID_BYTES);
    let input_spk = pay_to_script_hash_script(&delegate_redeem);

    // Single-input tx with delegate at index 0
    let (mut tx, utxos) = make_multi_input_mock_transaction(vec![(input_spk, Some(id))], vec![]);

    tx.inputs[0].signature_script = ScriptBuilder::new().add_data(&delegate_redeem).unwrap().drain();

    let result = try_verify_tx_input(&tx, &utxos, 0, &NullAccessor);
    assert!(result.is_err(), "should fail: delegate at index 0 → V4 GreaterThan fail");
}

// ═══════════════════════════════════════════════════════════════════
//  Cross-validation: raw bytes vs ScriptBuilder
// ═══════════════════════════════════════════════════════════════════

#[test]
fn delegate_script_cross_validation() {
    let cov_id_bytes = [0xAB; 32];
    let cov_id_words = zk_covenant_rollup_core::bytes_to_words(cov_id_bytes);
    let from_builder = build_delegate_entry_script(&cov_id_bytes);
    let from_raw = zk_covenant_rollup_core::p2sh::build_delegate_entry_script_bytes(&cov_id_words);
    assert_eq!(from_builder, from_raw.as_slice(), "raw byte builder must match ScriptBuilder output");
}

#[test]
fn delegate_script_cross_validation_various_ids() {
    for seed in [0x00u8, 0x42, 0xFF, 0xDE] {
        let cov_id_bytes = [seed; 32];
        let cov_id_words = zk_covenant_rollup_core::bytes_to_words(cov_id_bytes);
        let from_builder = build_delegate_entry_script(&cov_id_bytes);
        let from_raw = zk_covenant_rollup_core::p2sh::build_delegate_entry_script_bytes(&cov_id_words);
        assert_eq!(from_builder, from_raw.as_slice(), "mismatch for seed {:#04x}", seed);
    }
}
