use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{
    action::{Action, TransferAction},
    seq_commit::{seq_commitment_leaf, StreamingMerkleBuilder},
};

use crate::{auth, input, state, tx, witness::TransferWitness};

/// Process all transactions in a block, updating state and building merkle tree
pub fn process_block(stdin: &mut impl WordRead, state_root: &mut [u32; 8]) -> [u32; 8] {
    let tx_count = input::read_u32(stdin);
    let mut merkle_builder = StreamingMerkleBuilder::new();

    for _ in 0..tx_count {
        let (tx_id, version) = process_transaction(stdin, state_root);
        let leaf = seq_commitment_leaf(&tx_id, version);
        merkle_builder.add_leaf(leaf);
    }

    merkle_builder.finalize()
}

/// Process a single transaction
fn process_transaction(stdin: &mut impl WordRead, state_root: &mut [u32; 8]) -> ([u32; 8], u16) {
    let version = input::read_u32(stdin) as u16;

    let tx_id = match version {
        0 => tx::read_v0_tx(stdin),
        _ => process_v1_transaction(stdin, state_root),
    };

    (tx_id, version)
}

/// Process a V1+ transaction (may contain action payload)
fn process_v1_transaction(stdin: &mut impl WordRead, state_root: &mut [u32; 8]) -> [u32; 8] {
    let tx_data = tx::read_v1_tx_data(stdin);

    // Guest determines if this is an action based on cryptographic data
    // If it's a valid action, host MUST provide witness data
    if let Some(action) = tx_data.action {
        process_action(stdin, state_root, action);
    }

    tx_data.tx_id
}

/// Process a valid action transaction
///
/// Called only when guest has cryptographically determined this is a valid action.
/// Host must provide witness data for verification.
fn process_action(stdin: &mut impl WordRead, state_root: &mut [u32; 8], action: Action) {
    match action {
        Action::Transfer(transfer) => process_transfer(stdin, state_root, transfer),
    }
}

/// Process a transfer action
fn process_transfer(stdin: &mut impl WordRead, state_root: &mut [u32; 8], transfer: TransferAction) {
    let witness = TransferWitness::read_from_stdin(stdin);

    // Verify source authorization (prev tx output proves ownership)
    if auth::verify_source(&transfer, &witness.prev_tx).is_none() {
        // Invalid authorization - action rejected but tx_id still committed
        return;
    }

    // Process the transfer and update state if successful
    if let Some(new_root) = state::process_transfer(&transfer, &witness, state_root) {
        *state_root = new_root;
    }
}
