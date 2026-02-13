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

/// Size of the delegate/entry script in bytes.
///
/// Layout (53 bytes):
/// - 4 bytes:  index check (`OpTxInputIndex, Op0, OpGreaterThan, OpVerify`)
/// - 36 bytes: covenant ID check (`Op0, OpInputCovenantId, OpData32, cov_id(32), OpEqualVerify`)
/// - 12 bytes: domain suffix check (`Op0, Op0, OpTxInputScriptSigLen, OpDup, Op2, OpSub,
///             OpSwap, OpTxInputScriptSigSubstr, push-2, 0x51, 0x75, OpEqualVerify`)
/// - 1 byte:  `OpTrue`
pub const DELEGATE_SCRIPT_LEN: usize = 53;

/// Build the delegate/entry script as raw bytes (`no_std` compatible).
///
/// This is the `no_std` equivalent of `host::bridge::build_delegate_entry_script`.
/// It produces identical bytes without requiring `ScriptBuilder`.
///
/// The script verifies:
/// 1. Self is not at input index 0 (reserved for permission script)
/// 2. Input 0 carries the expected `covenant_id`
/// 3. Input 0's sig_script ends with `[0x51, 0x75]` (permission domain suffix)
pub fn build_delegate_entry_script_bytes(covenant_id: &[u32; 8]) -> [u8; DELEGATE_SCRIPT_LEN] {
    let mut s = [0u8; DELEGATE_SCRIPT_LEN];

    // Step 1: verify self not at input 0
    s[0] = 0xb9; // OpTxInputIndex
    s[1] = 0x00; // Op0
    s[2] = 0xa0; // OpGreaterThan
    s[3] = 0x69; // OpVerify

    // Step 2: check covenant ID of input 0
    s[4] = 0x00; // Op0
    s[5] = 0xcf; // OpInputCovenantId
    s[6] = 0x20; // OpData32
    s[7..39].copy_from_slice(crate::words_to_bytes_ref(covenant_id));
    s[39] = 0x88; // OpEqualVerify

    // Step 3: verify input 0's sig_script ends with permission domain suffix
    s[40] = 0x00; // Op0 (idx for Substr)
    s[41] = 0x00; // Op0 (idx for SigLen)
    s[42] = 0xc9; // OpTxInputScriptSigLen
    s[43] = 0x76; // OpDup
    s[44] = 0x52; // Op2
    s[45] = 0x94; // OpSub
    s[46] = 0x7c; // OpSwap
    s[47] = 0xbc; // OpTxInputScriptSigSubstr
    s[48] = 0x02; // push-2-bytes
    s[49] = 0x51; // OP_TRUE (permission suffix byte 1)
    s[50] = 0x75; // OP_DROP (permission suffix byte 2)
    s[51] = 0x88; // OpEqualVerify

    // Final result
    s[52] = 0x51; // OpTrue

    s
}

/// Verify that an entry (deposit) transaction output SPK is a P2SH wrapping the
/// correct delegate/entry script for the given covenant.
///
/// This ensures deposited funds are actually locked in the covenant, preventing
/// a malicious host from crediting L2 accounts for funds sent to arbitrary addresses.
pub fn verify_entry_output_spk(spk: &[u8], covenant_id: &[u32; 8]) -> bool {
    let delegate = build_delegate_entry_script_bytes(covenant_id);
    verify_p2sh_spk(spk, &delegate)
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
    fn test_verify_entry_output_spk() {
        let cov_id = [0xABu32; 8];
        let delegate = build_delegate_entry_script_bytes(&cov_id);
        let spk = pay_to_script_hash_spk_from_script(&delegate);

        // Valid SPK for this covenant_id
        assert!(verify_entry_output_spk(&spk, &cov_id));

        // Wrong covenant_id should fail
        let wrong_cov_id = [0xCDu32; 8];
        assert!(!verify_entry_output_spk(&spk, &wrong_cov_id));

        // Arbitrary SPK should fail
        assert!(!verify_entry_output_spk(&[0u8; 35], &cov_id));
        assert!(!verify_entry_output_spk(&[], &cov_id));
    }

    #[test]
    fn test_build_delegate_entry_script_bytes_length() {
        let cov_id = [0x42u32; 8];
        let script = build_delegate_entry_script_bytes(&cov_id);
        assert_eq!(script.len(), DELEGATE_SCRIPT_LEN);
        // Verify covenant_id bytes are embedded at offset 7..39
        assert_eq!(&script[7..39], crate::words_to_bytes_ref(&cov_id));
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
