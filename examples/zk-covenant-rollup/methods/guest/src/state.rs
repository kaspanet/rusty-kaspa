use zk_covenant_rollup_core::{
    action::EntryAction,
    empty_leaf_hash, key_to_index, leaf_hash,
    state::AccountWitness,
};

/// Process an entry (deposit): credit destination only (no source debit).
///
/// `amount` comes from the verified transaction output, not from the payload.
pub fn process_entry(entry: &EntryAction, dest_witness: &AccountWitness, amount: u64, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    verify_and_update_dest(&entry.destination, dest_witness, amount, current_root)
}

// ANCHOR: verify_and_debit_source
/// Verify the source account and compute the intermediate root after debit.
///
/// Handles both existing accounts and empty leaves (unknown accounts).
/// Returns `None` when the balance is insufficient (always true for empty leaves),
/// signalling the caller to skip the rest of the action (no auth/dest reads).
///
/// Asserts (host cheating — proof fails):
/// - SMT proof doesn't verify against root
/// - existing account: witness pubkey doesn't match source
///
/// Skips (user error):
/// - insufficient balance (including empty leaf with balance=0)
pub fn verify_and_debit_source(
    source: &[u32; 8],
    source_witness: &AccountWitness,
    amount: u64,
    current_root: &[u32; 8],
) -> Option<[u32; 8]> {
    let key = key_to_index(source);
    let is_empty = source_witness.balance == 0 && source_witness.pubkey == [0u32; 8];

    if is_empty {
        // Empty leaf — the source key has never been inserted.
        let empty_leaf = empty_leaf_hash();
        assert!(
            source_witness.proof.verify(current_root, key, &empty_leaf),
            "host cheating: source empty slot SMT proof invalid"
        );
        return None; // balance=0 always insufficient
    }

    // Existing account — witness pubkey must match source.
    assert_eq!(source_witness.pubkey, *source, "host cheating: source witness pubkey mismatch");

    let leaf = leaf_hash(source, source_witness.balance);
    assert!(source_witness.proof.verify(current_root, key, &leaf), "host cheating: source SMT proof invalid");

    if source_witness.balance < amount {
        return None;
    }

    let new_balance = source_witness.balance - amount;
    let new_leaf = leaf_hash(source, new_balance);
    Some(source_witness.proof.compute_root(&new_leaf, key))
}
// ANCHOR_END: verify_and_debit_source

// ANCHOR: verify_and_update_dest
/// Verify destination account and compute new root after credit.
///
/// Used by both transfers and entries. For new accounts, the witness has
/// pubkey=[0;8] and balance=0, and the SMT slot must be empty.
///
/// Asserts (host cheating):
/// - existing account: witness pubkey doesn't match destination
/// - SMT proof doesn't verify (empty slot or existing account)
pub fn verify_and_update_dest(
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
