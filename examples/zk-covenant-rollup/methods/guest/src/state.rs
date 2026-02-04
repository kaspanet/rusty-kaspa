use zk_covenant_rollup_core::{action::TransferAction, empty_leaf_hash, key_to_index, leaf_hash, state::AccountWitness};

use crate::witness::TransferWitness;

/// Verify and process a transfer, returning the new state root
pub fn process_transfer(transfer: &TransferAction, witness: &TransferWitness, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    // Verify source account
    let new_root = verify_and_update_source(transfer, &witness.source, current_root)?;

    // Verify and update destination account
    verify_and_update_dest(transfer, &witness.dest, &new_root)
}

/// Verify source account and compute intermediate root after debit
fn verify_and_update_source(transfer: &TransferAction, source_witness: &AccountWitness, current_root: &[u32; 8]) -> Option<[u32; 8]> {
    // Verify witness pubkey matches transfer source
    if source_witness.pubkey != transfer.source {
        return None;
    }

    // Verify SMT proof
    let source_key = key_to_index(&transfer.source);
    let source_leaf = leaf_hash(&transfer.source, source_witness.balance);
    if !source_witness.proof.verify(current_root, source_key, &source_leaf) {
        return None;
    }

    // Verify sufficient balance
    if source_witness.balance < transfer.amount {
        return None;
    }

    // Compute new root after debit
    let new_balance = source_witness.balance - transfer.amount;
    let new_leaf = leaf_hash(&transfer.source, new_balance);
    Some(source_witness.proof.compute_root(&new_leaf, source_key))
}

/// Verify destination account and compute final root after credit
fn verify_and_update_dest(transfer: &TransferAction, dest_witness: &AccountWitness, intermediate_root: &[u32; 8]) -> Option<[u32; 8]> {
    let dest_key = key_to_index(&transfer.destination);
    let is_new_account = dest_witness.balance == 0 && dest_witness.pubkey == [0u32; 8];

    if is_new_account {
        // Verify slot is empty
        let empty_leaf = empty_leaf_hash();
        if !dest_witness.proof.verify(intermediate_root, dest_key, &empty_leaf) {
            return None;
        }
    } else {
        // Verify existing account
        if dest_witness.pubkey != transfer.destination {
            return None;
        }
        let dest_leaf = leaf_hash(&dest_witness.pubkey, dest_witness.balance);
        if !dest_witness.proof.verify(intermediate_root, dest_key, &dest_leaf) {
            return None;
        }
    }

    // Compute final root after credit
    let new_balance = dest_witness.balance + transfer.amount;
    let new_leaf = leaf_hash(&transfer.destination, new_balance);
    Some(dest_witness.proof.compute_root(&new_leaf, dest_key))
}
