use zk_covenant_rollup_core::{extract_pubkey_from_spk, prev_tx::PrevTxWitness};

use crate::witness::PrevTxV1WitnessData;

// ANCHOR: verify_source
/// Verify source authorization for transfer and exit actions.
///
/// Takes the first input's prev_tx_id from the current action transaction's
/// rest_preimage (committed via rest_digest → tx_id). This ensures the host
/// cannot substitute a fake prev_tx — the prev_tx_id is derived from committed data.
///
/// Asserts (host cheating, proof fails):
/// - prev_tx witness doesn't hash to the committed prev_tx_id
///
/// Skips (user error, action rejected but tx_id still committed):
/// - SPK is not Schnorr P2PK
/// - SPK pubkey doesn't match action source
///
/// Returns the verified source pubkey if authorization succeeds.
pub fn verify_source(source: &[u32; 8], prev_tx: &PrevTxV1WitnessData, first_input_prev_tx_id: &[u32; 8]) -> Option<[u32; 8]> {
    // Step 1: Compute prev_tx_id from the witness preimage and ASSERT it matches
    // the first input outpoint from the current tx. If mismatch, host is cheating.
    let wrapped = PrevTxWitness::V1(prev_tx.witness.clone());
    let computed_tx_id = wrapped.compute_tx_id();
    assert_eq!(computed_tx_id, *first_input_prev_tx_id, "host cheating: prev_tx witness does not match first input outpoint");

    // Step 2: Extract output from the verified prev_tx
    let output_data = wrapped.extract_output()?;

    // Step 3: Get SPK as Schnorr P2PK (34 bytes). Rejects P2SH and ECDSA.
    let verified_spk = output_data.spk_as_p2pk()?;

    // Step 4: Extract 32-byte pubkey from the verified SPK
    let spk_pubkey = extract_pubkey_from_spk(&verified_spk)?;

    // Step 5: Verify action source matches the SPK's pubkey (skip if user error)
    if spk_pubkey != *source {
        return None;
    }

    Some(spk_pubkey)
}
// ANCHOR_END: verify_source
