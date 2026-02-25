use alloc::vec;
use risc0_zkvm::serde::WordRead;
use zk_covenant_rollup_core::{
    AlignedBytes,
    action::{Action, ActionHeader, EntryAction, ExitAction, OP_ENTRY, OP_EXIT, OP_TRANSFER, TransferAction},
    is_action_tx_id, payload_digest_bytes, rest_digest_bytes, tx_id_v1,
};

use crate::input;

/// Read and process a V0 transaction (no payload processing)
pub fn read_v0_tx(stdin: &mut impl WordRead) -> [u32; 8] {
    input::read_hash(stdin)
}

// ANCHOR: v1_tx_data
/// V1 transaction data after reading from stdin
pub struct V1TxData {
    pub tx_id: [u32; 8],
    /// Some if this is a valid action transaction (determined cryptographically)
    pub action: Option<Action>,
    /// Full rest_preimage for this transaction.
    /// The guest reads and hashes it (instead of trusting a host-provided rest_digest).
    /// Used by actions to verify inputs, extract outputs, etc.
    pub rest_preimage: AlignedBytes,
}
// ANCHOR_END: v1_tx_data

// ANCHOR: read_v1_tx_data
/// Read V1 transaction data and compute tx_id
///
/// The guest determines whether this is an action transaction based on:
/// 1. tx_id starts with ACTION_TX_ID_PREFIX
/// 2. header version is valid and operation is known
/// 3. action data is valid (e.g., non-zero amount for transfer)
///
/// This is computed from the cryptographic data, NOT from a host flag.
///
/// Payload handling:
/// - Host sends payload_len in BYTES (not words)
/// - We read ceil(payload_len/4) words (padded to word boundary)
/// - Compute payload_digest from actual bytes (trimmed to payload_len)
/// - Only parse as action if payload is 4-byte aligned (required for our format)
///
/// Rest preimage:
/// - Host sends the full rest_preimage (length-prefixed).
/// - Guest computes rest_digest = hash(rest_preimage) — never trusts host-provided digest.
/// - rest_preimage is stored for use by action processing (input verification, output parsing).
pub fn read_v1_tx_data(stdin: &mut impl WordRead) -> V1TxData {
    // Read payload length in BYTES
    let payload_byte_len = input::read_u32(stdin) as usize;

    // Calculate words needed (round up to word boundary)
    let payload_word_len = payload_byte_len.div_ceil(4);

    // Read as words (guaranteed 4-byte aligned)
    let mut payload_words = vec![0u32; payload_word_len];
    stdin.read_words(&mut payload_words).unwrap();

    // View as bytes for payload_digest
    let payload_bytes: &[u8] = bytemuck::cast_slice(&payload_words);
    let payload_bytes = &payload_bytes[..payload_byte_len]; // trim padding

    // Read rest_preimage (length-prefixed) and compute rest_digest
    let rest_preimage = input::read_aligned_bytes(stdin);
    let rest_digest = rest_digest_bytes(rest_preimage.as_bytes());

    // Compute tx_id from payload bytes and rest_digest
    let pd = payload_digest_bytes(payload_bytes);
    let tx_id = tx_id_v1(&pd, &rest_digest);

    // Only parse action if payload is 4-byte aligned (required for our action format)
    let action = if payload_byte_len.is_multiple_of(4) {
        parse_action(&payload_words)
    } else {
        None // Unaligned payload - not a valid action format
    };

    // Only valid if tx_id has action prefix AND action parsed successfully AND is valid
    let valid_action = if is_action_tx_id(&tx_id) { action.filter(|a| a.is_valid()) } else { None };

    V1TxData { tx_id, action: valid_action, rest_preimage }
}
// ANCHOR_END: read_v1_tx_data

// ANCHOR: parse_action
/// Parse action from payload words
fn parse_action(payload: &[u32]) -> Option<Action> {
    let (header_words, rest) = payload.split_first_chunk::<{ ActionHeader::WORDS }>()?;
    let header = ActionHeader::from_words_ref(header_words);

    if !header.is_valid_version() {
        return None;
    }

    // Parse action data based on operation
    match header.operation {
        OP_TRANSFER => {
            let transfer_words = rest.first_chunk()?;
            let transfer = TransferAction::from_words(*transfer_words);
            Some(Action::Transfer(transfer))
        }
        OP_ENTRY => {
            let entry_words = rest.first_chunk()?;
            let entry = EntryAction::from_words(*entry_words);
            Some(Action::Entry(entry))
        }
        OP_EXIT => {
            let exit_words = rest.first_chunk()?;
            let exit = ExitAction::from_words(*exit_words);
            Some(Action::Exit(exit))
        }
        _ => None, // Unknown operation
    }
}
// ANCHOR_END: parse_action
