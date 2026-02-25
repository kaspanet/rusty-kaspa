use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{
    AlignedBytes,
    action::{Action, EntryAction, ExitAction, TransferAction},
    bytes_to_words_ref, perm_leaf_hash,
    permission_tree::StreamingPermTreeBuilder,
    prev_tx::parse_first_input_outpoint,
    seq_commit::{StreamingMerkleBuilder, seq_commitment_leaf},
};

use crate::{auth, input, state, tx, witness::EntryWitness, witness::ExitWitness, witness::TransferWitness};

// ANCHOR: process_block
/// Process all transactions in a block, updating state and building merkle tree
pub fn process_block(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    covenant_id: &[u32; 8],
    perm_builder: &mut StreamingPermTreeBuilder,
) -> [u32; 8] {
    let tx_count = input::read_u32(stdin);
    let mut merkle_builder = StreamingMerkleBuilder::new();

    for _ in 0..tx_count {
        let (tx_id, version) = process_transaction(stdin, state_root, covenant_id, perm_builder);
        let leaf = seq_commitment_leaf(&tx_id, version);
        merkle_builder.add_leaf(leaf);
    }

    merkle_builder.finalize()
}
// ANCHOR_END: process_block

/// Process a single transaction
fn process_transaction(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    covenant_id: &[u32; 8],
    perm_builder: &mut StreamingPermTreeBuilder,
) -> ([u32; 8], u16) {
    let version = input::read_u32(stdin) as u16;

    let tx_id = match version {
        0 => tx::read_v0_tx(stdin),
        _ => process_v1_transaction(stdin, state_root, covenant_id, perm_builder),
    };

    (tx_id, version)
}

/// Process a V1+ transaction (may contain action payload)
fn process_v1_transaction(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    covenant_id: &[u32; 8],
    perm_builder: &mut StreamingPermTreeBuilder,
) -> [u32; 8] {
    let tx_data = tx::read_v1_tx_data(stdin);

    // Guest determines if this is an action based on cryptographic data
    // If it's a valid action, host MUST provide witness data
    if let Some(action) = tx_data.action {
        process_action(stdin, state_root, action, &tx_data.rest_preimage, covenant_id, perm_builder);
    }

    tx_data.tx_id
}

// ANCHOR: process_action
/// Process a valid action transaction
///
/// Called only when guest has cryptographically determined this is a valid action.
/// Host must provide witness data for verification.
///
/// The rest_preimage of the current transaction is used to:
/// - Extract first input outpoint (for transfer/exit source verification)
/// - Parse output data (for entry deposit amount)
fn process_action(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    action: Action,
    rest_preimage: &AlignedBytes,
    covenant_id: &[u32; 8],
    perm_builder: &mut StreamingPermTreeBuilder,
) {
    match action {
        Action::Transfer(transfer) => process_transfer(stdin, state_root, transfer, rest_preimage),
        Action::Entry(entry) => process_entry(stdin, state_root, entry, rest_preimage, covenant_id),
        Action::Exit(exit) => process_exit(stdin, state_root, exit, rest_preimage, perm_builder),
    }
}
// ANCHOR_END: process_action

// ANCHOR: process_transfer
/// Process a transfer action
fn process_transfer(stdin: &mut impl WordRead, state_root: &mut [u32; 8], transfer: TransferAction, rest_preimage: &AlignedBytes) {
    // Extract first input outpoint from current tx's rest_preimage.
    // This is committed via rest_digest → tx_id, so tamper-proof.
    let (first_input_prev_tx_id, first_input_output_index) =
        parse_first_input_outpoint(rest_preimage.as_bytes()).expect("action tx must have at least one input");
    let first_input_prev_tx_id = bytes_to_words_ref(&first_input_prev_tx_id);

    let witness = TransferWitness::read_from_stdin(stdin, first_input_output_index);

    // Verify source authorization.
    // Asserts prev_tx matches first input (host cheating → proof fails).
    // Skips if pubkey mismatch (user error → action rejected).
    if auth::verify_source(&transfer.source, &witness.prev_tx, &first_input_prev_tx_id).is_none() {
        return;
    }

    // Process the transfer and update state if successful
    if let Some(new_root) = state::process_transfer(&transfer, &witness, state_root) {
        *state_root = new_root;
    }
}
// ANCHOR_END: process_transfer

// ANCHOR: process_entry
/// Process an entry (deposit) action
///
/// Entry actions credit a destination account with the deposit amount.
/// The amount is extracted from the transaction output (verified via rest_preimage
/// which is now read at the V1TxData level, not as part of the entry witness).
fn process_entry(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    entry: EntryAction,
    rest_preimage: &AlignedBytes,
    covenant_id: &[u32; 8],
) {
    let witness = EntryWitness::read_from_stdin(stdin);

    // rest_preimage is already verified (guest computed rest_digest from it in read_v1_tx_data).

    // Reject if input 0 is a permission script. This prevents delegate change
    // outputs (from withdrawal transactions) from being counted as new deposits.
    if zk_covenant_rollup_core::prev_tx::input0_has_permission_suffix(rest_preimage.as_bytes()) {
        return;
    }

    // Parse the first output (index 0) to extract the deposit value.
    // Deposit output is always at index 0. tx_version=1 because entry txs are always V1.
    let output = match zk_covenant_rollup_core::prev_tx::parse_output_at_index(rest_preimage.as_bytes(), 0, 1) {
        Some(o) => o,
        None => return, // Parse failure
    };

    // Verify the output SPK is P2SH of the delegate/entry script for this covenant.
    if !zk_covenant_rollup_core::p2sh::verify_entry_output_spk(&output.spk, covenant_id) {
        return;
    }

    let amount = output.value;
    if amount == 0 {
        return; // Zero-value deposit — skip
    }

    // Credit the destination account
    if let Some(new_root) = state::process_entry(&entry, &witness.dest, amount, state_root) {
        *state_root = new_root;
    }
}
// ANCHOR_END: process_entry

// ANCHOR: process_exit
/// Process an exit (withdrawal) action
///
/// Exits debit the source account and accumulate a permission tree leaf.
fn process_exit(
    stdin: &mut impl WordRead,
    state_root: &mut [u32; 8],
    exit: ExitAction,
    rest_preimage: &AlignedBytes,
    perm_builder: &mut StreamingPermTreeBuilder,
) {
    // Extract first input outpoint from current tx's rest_preimage.
    let (first_input_prev_tx_id, first_input_output_index) =
        parse_first_input_outpoint(rest_preimage.as_bytes()).expect("action tx must have at least one input");
    let first_input_prev_tx_id = bytes_to_words_ref(&first_input_prev_tx_id);

    let witness = ExitWitness::read_from_stdin(stdin, first_input_output_index);

    // Verify source authorization (asserts on host cheating, skips on user error)
    if auth::verify_source(&exit.source, &witness.prev_tx, &first_input_prev_tx_id).is_none() {
        return;
    }

    // Debit source account and add permission leaf
    if let Some(new_root) = state::process_exit(&exit, &witness.source, state_root) {
        *state_root = new_root;
        perm_builder.add_leaf(perm_leaf_hash(exit.destination_spk_bytes(), exit.amount));
    }
}
// ANCHOR_END: process_exit
