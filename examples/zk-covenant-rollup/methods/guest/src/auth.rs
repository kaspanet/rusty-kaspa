use zk_covenant_rollup_core::extract_pubkey_from_spk;

use crate::witness::PrevTxV1WitnessData;

/// Verify source authorization for transfer and exit actions.
///
/// Only Schnorr P2PK source SPKs are supported. The guest uses 32-byte pubkeys
/// internally, so P2SH sources (which have a script hash, not a pubkey) cannot
/// be mapped to an account. ECDSA P2PK (33-byte compressed pubkey) could work
/// in theory but is not enabled for simplicity.
///
/// If the source SPK is not Schnorr P2PK, `spk_as_p2pk()` returns `None` and
/// the action is silently skipped (tx_id is still committed to the seq tree).
///
/// Returns the verified source pubkey if authorization succeeds.
pub fn verify_source(source: &[u32; 8], prev_tx: &PrevTxV1WitnessData) -> Option<[u32; 8]> {
    // Step 1: Verify the SPK is cryptographically committed to prev_tx_id
    let output_data = prev_tx.verify_output()?;

    // Step 2: Get SPK as Schnorr P2PK (34 bytes). Rejects P2SH and ECDSA.
    let verified_spk = output_data.spk_as_p2pk()?;

    // Step 3: Extract 32-byte pubkey from the verified SPK
    let spk_pubkey = extract_pubkey_from_spk(&verified_spk)?;

    // Step 4: Verify action source matches the SPK's pubkey
    if spk_pubkey != *source {
        return None;
    }

    Some(spk_pubkey)
}
