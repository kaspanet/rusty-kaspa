use zk_covenant_rollup_core::{
    action::{EntryAction, ExitAction, TransferAction},
    empty_leaf_hash, key_to_index, leaf_hash,
    state::AccountWitness,
};

use crate::witness::TransferWitness;

/// Verify and process a transfer, returning the new state root
pub fn process_transfer(transfer: &TransferAction, witness: &TransferWitness, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    // Verify source account
    let new_root = verify_and_update_source(transfer, &witness.source, current_root)?;

    // Verify and update destination account
    verify_and_update_dest(&transfer.destination, &witness.dest, transfer.amount, &new_root)
}

// ANCHOR: process_exit_state
/// Process an exit (withdrawal): debit source account only.
///
/// Returns the new state root after debiting the source by `exit.amount`.
///
/// Asserts (host cheating):
/// - witness.pubkey != exit.source (host provided wrong witness)
/// - SMT proof doesn't verify (host provided invalid proof)
///
/// Skips (user error):
/// - insufficient balance
pub fn process_exit(exit: &ExitAction, source_witness: &AccountWitness, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    // Assert: witness pubkey must match action source (host cheating if not)
    assert_eq!(source_witness.pubkey, exit.source, "host cheating: exit witness pubkey mismatch");

    let key = key_to_index(&exit.source);
    let leaf = leaf_hash(&exit.source, source_witness.balance);

    // Assert: SMT proof must verify (host cheating if not — every pubkey has a valid proof)
    assert!(source_witness.proof.verify(current_root, key, &leaf), "host cheating: exit source SMT proof invalid");

    if source_witness.balance < exit.amount {
        return None;
    }

    let new_balance = source_witness.balance - exit.amount;
    let new_leaf = leaf_hash(&exit.source, new_balance);
    Some(source_witness.proof.compute_root(&new_leaf, key))
}
// ANCHOR_END: process_exit_state

/// Process an entry (deposit): credit destination only (no source debit).
///
/// `amount` comes from the verified transaction output, not from the payload.
pub fn process_entry(entry: &EntryAction, dest_witness: &AccountWitness, amount: u64, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    verify_and_update_dest(&entry.destination, dest_witness, amount, current_root)
}

/// Verify source account and compute intermediate root after debit.
///
/// Asserts (host cheating):
/// - witness pubkey doesn't match transfer source
/// - SMT proof doesn't verify
///
/// Skips (user error):
/// - insufficient balance
fn verify_and_update_source(transfer: &TransferAction, source_witness: &AccountWitness, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    // Assert: witness pubkey must match transfer source (host cheating if not)
    assert_eq!(source_witness.pubkey, transfer.source, "host cheating: transfer source witness pubkey mismatch");

    // Assert: SMT proof must verify (host cheating if not)
    let source_key = key_to_index(&transfer.source);
    let source_leaf = leaf_hash(&transfer.source, source_witness.balance);
    assert!(source_witness.proof.verify(current_root, source_key, &source_leaf), "host cheating: transfer source SMT proof invalid");

    // Skip: insufficient balance (user error)
    if source_witness.balance < transfer.amount {
        return None;
    }

    // Compute new root after debit
    let new_balance = source_witness.balance - transfer.amount;
    let new_leaf = leaf_hash(&transfer.source, new_balance);
    Some(source_witness.proof.compute_root(&new_leaf, source_key))
}

// ANCHOR: verify_and_update_dest
/// Verify destination account and compute new root after credit.
///
/// Used by both transfers and entries. For new accounts, the witness has
/// pubkey=[0;8] and balance=0, and the SMT slot must be empty.
///
/// Asserts (host cheating):
/// - existing account: witness pubkey doesn't match destination
/// - SMT proof doesn't verify (empty slot or existing account)
fn verify_and_update_dest(
    destination: &[u32; 8],
    dest_witness: &AccountWitness,
    amount: u64,
    intermediate_root: &[u32; 8],
) -> Option<[u32; 8]> {
    let dest_key = key_to_index(destination);
    let is_new_account = dest_witness.balance == 0 && dest_witness.pubkey == [0u32; 8];

    if is_new_account {
        // Assert: empty slot proof must verify (host cheating if not)
        let empty_leaf = empty_leaf_hash();
        assert!(
            dest_witness.proof.verify(intermediate_root, dest_key, &empty_leaf),
            "host cheating: dest empty slot SMT proof invalid"
        );
    } else {
        // Assert: witness pubkey must match destination (host cheating if not)
        assert_eq!(dest_witness.pubkey, *destination, "host cheating: dest witness pubkey mismatch");

        // Assert: existing account proof must verify (host cheating if not)
        let dest_leaf = leaf_hash(&dest_witness.pubkey, dest_witness.balance);
        assert!(
            dest_witness.proof.verify(intermediate_root, dest_key, &dest_leaf),
            "host cheating: dest existing account SMT proof invalid"
        );
    }

    // Compute final root after credit
    let new_balance = dest_witness.balance + amount;
    let new_leaf = leaf_hash(destination, new_balance);
    Some(dest_witness.proof.compute_root(&new_leaf, dest_key))
}
// ANCHOR_END: verify_and_update_dest
