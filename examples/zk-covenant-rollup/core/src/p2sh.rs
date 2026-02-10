//! P2SH (Pay-to-Script-Hash) SPK helpers.
//!
//! A p2sh script public key has the format (script bytes only, no version prefix):
//! OpBlake2b (0xaa) || OpData32 (0x20) || blake2b_hash (32 bytes) || OpEqual (0x87)
//! Total: 35 bytes

use crate::p2pk::OP_DATA_32;

/// OpBlake2b opcode
pub const OP_BLAKE2B: u8 = 0xaa;

/// OpEqual opcode
pub const OP_EQUAL: u8 = 0x87;

/// P2SH SPK size (1 + 1 + 32 + 1 = 35 bytes)
pub const P2SH_SPK_SIZE: usize = 35;

/// Compute unkeyed blake2b-256 hash of a redeem script.
///
/// This matches the hashing used by `pay_to_script_hash_script` in
/// `crypto/txscript/src/standard.rs` — unkeyed blake2b with 32-byte output.
pub fn blake2b_script_hash(redeem_script: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(redeem_script);
    let mut out = [0u8; 32];
    out.copy_from_slice(hash.as_bytes());
    out
}

/// Create a P2SH script public key from a pre-computed script hash.
///
/// Format: OpBlake2b || OpData32 || script_hash (32 bytes) || OpEqual
pub fn pay_to_script_hash_spk(script_hash: &[u8; 32]) -> [u8; P2SH_SPK_SIZE] {
    let mut spk = [0u8; P2SH_SPK_SIZE];
    spk[0] = OP_BLAKE2B;
    spk[1] = OP_DATA_32;
    spk[2..34].copy_from_slice(script_hash);
    spk[34] = OP_EQUAL;
    spk
}

/// Create a P2SH script public key directly from a redeem script.
///
/// Hashes the redeem script with blake2b-256, then wraps in P2SH SPK format.
pub fn pay_to_script_hash_spk_from_script(redeem_script: &[u8]) -> [u8; P2SH_SPK_SIZE] {
    let hash = blake2b_script_hash(redeem_script);
    pay_to_script_hash_spk(&hash)
}

/// Check if a script public key is a valid P2SH format.
pub fn is_p2sh_spk(spk: &[u8]) -> bool {
    spk.len() == P2SH_SPK_SIZE && spk[0] == OP_BLAKE2B && spk[1] == OP_DATA_32 && spk[34] == OP_EQUAL
}

/// Extract the script hash from a P2SH script public key.
///
/// Returns `None` if the SPK is not a valid P2SH format.
pub fn extract_script_hash(spk: &[u8]) -> Option<[u8; 32]> {
    if !is_p2sh_spk(spk) {
        return None;
    }
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&spk[2..34]);
    Some(hash)
}

/// Verify that a script public key is a P2SH wrapping the given redeem script.
pub fn verify_p2sh_spk(spk: &[u8], redeem_script: &[u8]) -> bool {
    match extract_script_hash(spk) {
        Some(hash) => hash == blake2b_script_hash(redeem_script),
        None => false,
    }
}

/// Verify that an entry (deposit) transaction output SPK is valid.
///
/// # Current behavior
///
/// **Stub — always returns `true`.**
///
/// # Intended design
///
/// When fully implemented, this function will verify that `spk` is a P2SH script
/// wrapping the correct **delegate/entry redeem script** for the rollup covenant.
/// This ensures that deposited funds are actually locked in the covenant, preventing
/// a malicious host from crediting L2 accounts for funds sent to arbitrary addresses.
///
/// ## Script dependency chain
///
/// Kaspa covenants use a layered script architecture:
///
/// 1. **State script** — encodes the current covenant state (e.g. the rollup state root).
///    Changes every transition.
/// 2. **Permission script** — defines what operations are allowed (spend, delegate, etc.).
///    Typically contains `OpCheckCovenantVerify` and permission-specific logic.
/// 3. **Delegate/entry script** — a specific permission script that authorizes deposits
///    into the rollup. This is the script whose hash appears in the deposit output SPK.
///
/// The deposit output SPK must be: `P2SH(delegate_entry_redeem_script)` where
/// `delegate_entry_redeem_script` is constructed from the covenant's permission rules
/// and includes the covenant ID.
///
/// ## Parameters needed (future)
///
/// - `covenant_id`: identifies which covenant the deposit targets. The delegate/entry
///   redeem script embeds the covenant ID so that deposits are locked to the correct
///   covenant instance.
/// - Possibly `image_id`: the zkVM image ID, if the entry script encodes which proof
///   system is authorized to process deposits.
///
/// ## Why this matters
///
/// Without SPK verification, the guest trusts the host's claim about which output
/// constitutes a deposit. A malicious host could point to an output paying an
/// unrelated address, causing the guest to credit L2 funds that aren't actually
/// locked in the covenant. Verifying the SPK is P2SH of the correct delegate/entry
/// script closes this attack vector.
///
/// ## Signature
///
/// When implemented, the signature will likely change to accept additional parameters:
/// ```ignore
/// fn verify_entry_output_spk(spk: &[u8], covenant_id: &[u8], ...) -> bool
/// ```
pub fn verify_entry_output_spk(spk: &[u8]) -> bool {
    // TODO: Verify spk is P2SH of the delegate/entry redeem script.
    // For now, accept any SPK to avoid blocking progress on other features.
    let _ = spk;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blake2b_script_hash() {
        let script = b"test redeem script";
        let hash = blake2b_script_hash(script);
        // Should produce a 32-byte hash
        assert_eq!(hash.len(), 32);
        // Same input should produce same output
        assert_eq!(hash, blake2b_script_hash(script));
        // Different input should produce different output
        assert_ne!(hash, blake2b_script_hash(b"other script"));
    }

    #[test]
    fn test_pay_to_script_hash_spk() {
        let hash = [0x42u8; 32];
        let spk = pay_to_script_hash_spk(&hash);

        assert_eq!(spk.len(), P2SH_SPK_SIZE);
        assert_eq!(spk[0], OP_BLAKE2B);
        assert_eq!(spk[1], OP_DATA_32);
        assert_eq!(&spk[2..34], &hash);
        assert_eq!(spk[34], OP_EQUAL);
    }

    #[test]
    fn test_pay_to_script_hash_spk_from_script_roundtrip() {
        let redeem_script = b"my redeem script";
        let spk = pay_to_script_hash_spk_from_script(redeem_script);

        assert!(is_p2sh_spk(&spk));
        let extracted = extract_script_hash(&spk).unwrap();
        assert_eq!(extracted, blake2b_script_hash(redeem_script));
    }

    #[test]
    fn test_is_p2sh_spk() {
        let hash = [0x42u8; 32];
        let spk = pay_to_script_hash_spk(&hash);
        assert!(is_p2sh_spk(&spk));

        // Wrong length
        assert!(!is_p2sh_spk(&[0u8; 34]));
        assert!(!is_p2sh_spk(&[0u8; 36]));

        // Wrong opcodes
        let mut bad = spk;
        bad[0] = 0x00;
        assert!(!is_p2sh_spk(&bad));

        let mut bad2 = spk;
        bad2[1] = 0x00;
        assert!(!is_p2sh_spk(&bad2));

        let mut bad3 = spk;
        bad3[34] = 0x00;
        assert!(!is_p2sh_spk(&bad3));
    }

    #[test]
    fn test_extract_script_hash() {
        let hash = [0x42u8; 32];
        let spk = pay_to_script_hash_spk(&hash);
        assert_eq!(extract_script_hash(&spk), Some(hash));

        // Invalid SPK returns None
        assert_eq!(extract_script_hash(&[0u8; 35]), None);
        assert_eq!(extract_script_hash(&[0u8; 34]), None);
    }

    #[test]
    fn test_verify_p2sh_spk() {
        let redeem_script = b"my redeem script";
        let spk = pay_to_script_hash_spk_from_script(redeem_script);

        assert!(verify_p2sh_spk(&spk, redeem_script));

        // Wrong redeem script should fail
        assert!(!verify_p2sh_spk(&spk, b"wrong script"));

        // Invalid SPK format should fail
        assert!(!verify_p2sh_spk(&[0u8; 35], redeem_script));
    }

    #[test]
    fn test_verify_entry_output_spk_stub() {
        // Stub always returns true
        assert!(verify_entry_output_spk(&[]));
        assert!(verify_entry_output_spk(&[0u8; 35]));
        assert!(verify_entry_output_spk(&[1, 2, 3]));
    }

    /// Test that our P2SH SPK construction matches kaspa_txscript::pay_to_script_hash_script.
    ///
    /// kaspa_txscript returns a ScriptPublicKey with version + script bytes.
    /// Our function returns only the script bytes (no version prefix).
    /// This test verifies both the hash and the full SPK byte layout are identical.
    #[test]
    fn test_p2sh_spk_matches_kaspa() {
        let test_scripts: &[&[u8]] = &[
            b"",
            b"\x51", // OP_TRUE
            b"simple redeem script",
            &[0xaa; 64], // 64 bytes of 0xaa
            &[0u8; 256], // 256 zero bytes
        ];

        for redeem_script in test_scripts {
            let kaspa_spk = kaspa_txscript::pay_to_script_hash_script(redeem_script);
            let our_spk = pay_to_script_hash_spk_from_script(redeem_script);

            assert_eq!(our_spk.as_slice(), kaspa_spk.script(), "P2SH SPK mismatch for redeem script: {:?}", redeem_script,);
        }
    }
}
