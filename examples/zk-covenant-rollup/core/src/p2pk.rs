//! P2PK (Pay-to-Public-Key) SPK helpers.
//!
//! A p2pk script public key has the format:
//! OpData32 (0x20) || pubkey (32 bytes) || OpCheckSig (0xac)
//! Total: 34 bytes
//!
//! Pubkeys are returned as `[u32; 8]` for zkVM efficiency.

/// OpData32 opcode
pub const OP_DATA_32: u8 = 0x20;

/// OpCheckSig opcode
pub const OP_CHECK_SIG: u8 = 0xac;

/// P2PK SPK size
pub const P2PK_SPK_SIZE: usize = 34;

/// Create a pay-to-pubkey script public key
///
/// Format: OpData32 || pubkey || OpCheckSig
pub fn pay_to_pubkey_spk(pubkey: &[u8; 32]) -> [u8; P2PK_SPK_SIZE] {
    let mut spk = [0u8; P2PK_SPK_SIZE];
    spk[0] = OP_DATA_32;
    spk[1..33].copy_from_slice(pubkey);
    spk[33] = OP_CHECK_SIG;
    spk
}

/// Create a pay-to-pubkey script public key from [u32; 8]
pub fn pay_to_pubkey_spk_words(pubkey: &[u32; 8]) -> [u8; P2PK_SPK_SIZE] {
    pay_to_pubkey_spk(bytemuck::bytes_of(pubkey).try_into().unwrap())
}

/// Extract the pubkey from a p2pk script public key
///
/// Returns None if the SPK is not a valid p2pk format.
/// Returns the pubkey as `[u32; 8]` for zkVM efficiency.
pub fn extract_pubkey_from_spk(spk: &[u8]) -> Option<[u32; 8]> {
    if spk.len() != P2PK_SPK_SIZE {
        return None;
    }
    if spk[0] != OP_DATA_32 || spk[33] != OP_CHECK_SIG {
        return None;
    }
    let pubkey_bytes: [u8; 32] = spk[1..33].try_into().unwrap();
    Some(crate::bytes_to_words(pubkey_bytes))
}

/// Verify that a script public key is a p2pk for the expected pubkey
pub fn verify_p2pk_spk(spk: &[u8], expected_pubkey: &[u32; 8]) -> bool {
    match extract_pubkey_from_spk(spk) {
        Some(pk) => pk == *expected_pubkey,
        None => false,
    }
}

/// Check if a script public key is a valid p2pk format
pub fn is_p2pk_spk(spk: &[u8]) -> bool {
    spk.len() == P2PK_SPK_SIZE && spk[0] == OP_DATA_32 && spk[33] == OP_CHECK_SIG
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pay_to_pubkey_spk() {
        let pubkey = [0x42u8; 32];
        let spk = pay_to_pubkey_spk(&pubkey);

        assert_eq!(spk.len(), P2PK_SPK_SIZE);
        assert_eq!(spk[0], OP_DATA_32);
        assert_eq!(&spk[1..33], &pubkey);
        assert_eq!(spk[33], OP_CHECK_SIG);
    }

    #[test]
    fn test_extract_pubkey() {
        let pubkey_words: [u32; 8] = [0x42424242; 8];
        let pubkey_bytes: [u8; 32] = bytemuck::cast(pubkey_words);
        let spk = pay_to_pubkey_spk(&pubkey_bytes);

        let extracted = extract_pubkey_from_spk(&spk);
        assert_eq!(extracted, Some(pubkey_words));
    }

    #[test]
    fn test_extract_invalid_length() {
        let short_spk = [0u8; 33];
        assert_eq!(extract_pubkey_from_spk(&short_spk), None);

        let long_spk = [0u8; 35];
        assert_eq!(extract_pubkey_from_spk(&long_spk), None);
    }

    #[test]
    fn test_extract_invalid_format() {
        let mut bad_spk = [0u8; 34];
        bad_spk[0] = 0x21; // Wrong opcode
        bad_spk[33] = OP_CHECK_SIG;
        assert_eq!(extract_pubkey_from_spk(&bad_spk), None);

        let mut bad_spk2 = [0u8; 34];
        bad_spk2[0] = OP_DATA_32;
        bad_spk2[33] = 0xab; // Wrong checksig
        assert_eq!(extract_pubkey_from_spk(&bad_spk2), None);
    }

    #[test]
    fn test_verify_p2pk_spk() {
        let pubkey_words: [u32; 8] = [0x42424242; 8];
        let pubkey_bytes: [u8; 32] = bytemuck::cast(pubkey_words);
        let spk = pay_to_pubkey_spk(&pubkey_bytes);

        assert!(verify_p2pk_spk(&spk, &pubkey_words));

        let other_pubkey: [u32; 8] = [0x43434343; 8];
        assert!(!verify_p2pk_spk(&spk, &other_pubkey));
    }

    #[test]
    fn test_is_p2pk_spk() {
        let pubkey = [0x42u8; 32];
        let spk = pay_to_pubkey_spk(&pubkey);
        assert!(is_p2pk_spk(&spk));

        let not_p2pk = [0u8; 34];
        assert!(!is_p2pk_spk(&not_p2pk));
    }
}
