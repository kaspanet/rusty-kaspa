use zk_covenant_rollup_core::{action::TransferAction, extract_pubkey_from_spk};

use crate::witness::PrevTxV1WitnessData;

/// Verify source authorization for a transfer
///
/// Returns the verified source pubkey if authorization succeeds
pub fn verify_source(transfer: &TransferAction, prev_tx: &PrevTxV1WitnessData) -> Option<[u32; 8]> {
    // Step 1: Verify the SPK is cryptographically committed to prev_tx_id
    let output_data = prev_tx.verify_output()?;

    // Step 2: Get SPK as p2pk format (34 bytes)
    let verified_spk = output_data.spk_as_p2pk()?;

    // Step 3: Extract pubkey from the verified SPK
    let spk_pubkey = extract_pubkey_from_spk(&verified_spk)?;

    // Step 4: Verify transfer.source matches the SPK's pubkey
    if spk_pubkey != transfer.source {
        return None;
    }

    Some(spk_pubkey)
}
