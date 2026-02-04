#![no_std]

extern crate alloc;
extern crate core;

use alloc::vec::Vec;

pub mod action;
pub mod p2pk;
pub mod prev_tx;
pub mod seq_commit;
pub mod smt;
pub mod state;

/// Word-aligned byte buffer. Stores data as `Vec<u32>` for alignment,
/// provides `&[u8]` view without extra allocation.
#[derive(Clone, Debug, Default)]
pub struct AlignedBytes {
    words: Vec<u32>,
    byte_len: usize,
}

impl AlignedBytes {
    /// Create from words and byte length
    pub fn new(words: Vec<u32>, byte_len: usize) -> Self {
        Self { words, byte_len }
    }

    /// Create from a byte slice by copying into word-aligned storage
    pub fn from_bytes(bytes: &[u8]) -> Self {
        if bytes.is_empty() {
            return Self::empty();
        }
        let byte_len = bytes.len();
        let num_words = byte_len.div_ceil(4);
        let mut words = alloc::vec![0u32; num_words];
        bytemuck::cast_slice_mut::<u32, u8>(&mut words)[..byte_len].copy_from_slice(bytes);
        Self { words, byte_len }
    }

    /// Create empty
    pub fn empty() -> Self {
        Self { words: Vec::new(), byte_len: 0 }
    }

    /// View the data as a byte slice (trimmed to actual length)
    pub fn as_bytes(&self) -> &[u8] {
        if self.byte_len == 0 {
            return &[];
        }
        let bytes: &[u8] = bytemuck::cast_slice(&self.words);
        &bytes[..self.byte_len]
    }

    /// Get the byte length
    pub fn len(&self) -> usize {
        self.byte_len
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.byte_len == 0
    }

    /// Consume and return the underlying words
    pub fn into_words(self) -> Vec<u32> {
        self.words
    }
}

pub use action::{ACTION_VERSION, Action, ActionHeader, OP_TRANSFER, TransferAction};
pub use p2pk::{P2PK_SPK_SIZE, extract_pubkey_from_spk, is_p2pk_spk, pay_to_pubkey_spk, verify_p2pk_spk};
pub use prev_tx::{
    CovenantBinding, OutputData, PrevTxV0Witness, PrevTxV1Witness, PrevTxWitness, parse_output_at_index, verify_output_in_tx,
};
pub use smt::{SMT_DEPTH, SmtProof, branch_hash, empty_leaf_hash, key_to_index, leaf_hash};
pub use state::{Account, AccountWitness, StateRoot, empty_tree_root};

/// Safely convert `[u8; 32]` to `[u32; 8]` by copying into aligned memory.
///
/// IMPORTANT: Direct casting via `bytemuck::cast` from `[u8; 32]` to `[u32; 8]`
/// is unsafe because `[u32; 8]` requires 4-byte alignment while `[u8; 32]` only
/// has 1-byte alignment. This function copies bytes into properly aligned memory.
#[inline]
pub fn bytes_to_words(bytes: [u8; 32]) -> [u32; 8] {
    let mut words = [0u32; 8];
    bytemuck::bytes_of_mut(&mut words).copy_from_slice(&bytes);
    words
}

/// Safely convert `&[u8; 32]` to `[u32; 8]` by copying into aligned memory.
#[inline]
pub fn bytes_to_words_ref(bytes: &[u8; 32]) -> [u32; 8] {
    let mut words = [0u32; 8];
    bytemuck::bytes_of_mut(&mut words).copy_from_slice(bytes);
    words
}

/// Convert `[u32; 8]` to `[u8; 32]` (always safe - going to lower alignment).
#[inline]
pub fn words_to_bytes(words: [u32; 8]) -> [u8; 32] {
    bytemuck::cast(words)
}

/// Convert `&[u32; 8]` to `&[u8; 32]` (always safe - going to lower alignment).
#[inline]
pub fn words_to_bytes_ref(words: &[u32; 8]) -> &[u8; 32] {
    bytemuck::cast_ref(words)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, align(4))]
pub struct PublicInput {
    pub prev_state_hash: [u32; 8],

    pub prev_seq_commitment: [u32; 8],
}

impl PublicInput {
    pub fn as_words(&self) -> &[u32] {
        bytemuck::cast_slice(bytemuck::bytes_of(self))
    }

    pub fn as_words_mut(&mut self) -> &mut [u32] {
        bytemuck::cast_slice_mut(bytemuck::bytes_of_mut(self))
    }
}

/// Single byte prefix for action transaction IDs (0x41 = 'A')
/// Using a single byte makes it ~256x easier to find valid nonces for testing
pub const ACTION_TX_ID_PREFIX: u8 = b'A';

/// Check if a tx_id represents an action transaction (first byte matches prefix)
#[inline]
pub fn is_action_tx_id(tx_id: &[u32; 8]) -> bool {
    (tx_id[0] as u8) == ACTION_TX_ID_PREFIX
}

pub fn payload_digest(payload: &[u32]) -> [u32; 8] {
    payload_digest_bytes(bytemuck::cast_slice(payload))
}

pub fn payload_digest_bytes(payload: &[u8]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"PayloadDigest";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(blake3::keyed_hash(&KEY, payload).as_bytes());
    out
}

pub fn tx_id_v1(payload_digest: &[u32; 8], rest_digest: &[u32; 8]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"TransactionV1Id";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut hasher = blake3::Hasher::new_keyed(&KEY);
    hasher.update(bytemuck::cast_slice(payload_digest));
    hasher.update(bytemuck::cast_slice(rest_digest));
    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hasher.finalize().as_bytes());
    out
}

/// Compute rest_digest for transaction data.
///
/// In kaspa's tx_id_v1 hashing, rest_digest includes all transaction data
/// except the payload. Importantly, it includes OUTPUT SPKs but NOT input SPKs.
/// The input's previous SPK is committed via sighash for signature verification.
///
/// In our design, source authorization is handled differently:
/// - Source pubkey is IN the payload (committed via payload_digest)
/// - Guest verifies payload.source == extract_pubkey(first_input_spk)
///
/// This function computes rest_digest from "other_data" representing
/// outputs, locktime, and other tx fields (but not input SPKs).
pub fn rest_digest(other_data: &[u32]) -> [u32; 8] {
    rest_digest_bytes(bytemuck::cast_slice(other_data))
}

pub fn rest_digest_bytes(rest_preimage: &[u8]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"TransactionRest";
    const KEY: [u8; blake3::KEY_LEN] = domain_to_key(DOMAIN_SEP);

    let mut hasher = blake3::Hasher::new_keyed(&KEY);
    hasher.update(rest_preimage);
    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hasher.finalize().as_bytes());
    out
}

pub const fn domain_to_key(domain: &[u8]) -> [u8; blake3::KEY_LEN] {
    let mut key = [0u8; blake3::KEY_LEN];
    let mut i = 0usize;
    while i < domain.len() {
        key[i] = domain[i];
        i += 1;
    }
    key
}

/// Compute V0 transaction ID using blake2b.
/// This uses the same domain separator as Kaspa's TransactionID hasher.
pub fn tx_id_v0(preimage: &[u8]) -> [u32; 8] {
    const DOMAIN_SEP: &[u8] = b"TransactionID";

    let hash = blake2b_simd::Params::new().hash_length(32).key(DOMAIN_SEP).hash(preimage);

    let mut out = [0u32; 8];
    bytemuck::bytes_of_mut(&mut out).copy_from_slice(hash.as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::{Hasher, HasherBase};

    /// Test that our blake2b hashing matches Kaspa's TransactionID hasher
    #[test]
    fn test_tx_id_v0_matches_kaspa() {
        let test_data: &[&[u8]] = &[&[], &[1], &[1, 2, 3, 4, 5], &[42; 64], &[0; 32]];

        for data in test_data {
            let our_result = tx_id_v0(data);
            let kaspa_result = kaspa_hashes::TransactionID::hash(data);
            let our_bytes: [u8; 32] = bytemuck::cast(our_result);

            assert_eq!(our_bytes, kaspa_result.as_bytes(), "tx_id_v0 mismatch for data: {:?}", data);
        }
    }

    /// Test that our blake3 PayloadDigest matches Kaspa's
    #[test]
    fn test_payload_digest_matches_kaspa() {
        let test_payloads: &[[u32; 4]] = &[[0, 0, 0, 0], [1, 2, 3, 4], [0xDEADBEEF, 0xCAFEBABE, 0x12345678, 0x9ABCDEF0]];

        for payload_words in test_payloads {
            let our_result = payload_digest(payload_words);
            let payload_bytes: &[u8] = bytemuck::cast_slice(payload_words);
            let kaspa_result = kaspa_hashes::PayloadDigest::hash(payload_bytes);
            let our_bytes: [u8; 32] = bytemuck::cast(our_result);

            assert_eq!(our_bytes, kaspa_result.as_bytes(), "payload_digest mismatch for payload: {:?}", payload_words);
        }
    }

    /// Test that our TransactionRest (rest_digest) matches Kaspa's
    #[test]
    fn test_rest_digest_matches_kaspa() {
        let test_data: &[[u32; 8]] = &[[0; 8], [1, 2, 3, 4, 5, 6, 7, 8], [0xDEADBEEF; 8]];

        for data_words in test_data {
            let our_result = rest_digest(data_words);
            let data_bytes: &[u8] = bytemuck::cast_slice(data_words);
            let kaspa_result = kaspa_hashes::TransactionRest::hash(data_bytes);
            let our_bytes: [u8; 32] = bytemuck::cast(our_result);

            assert_eq!(our_bytes, kaspa_result.as_bytes(), "rest_digest mismatch for data: {:?}", data_words);
        }
    }

    /// Test that our TransactionV1Id (tx_id_v1) matches Kaspa's
    #[test]
    fn test_tx_id_v1_matches_kaspa() {
        let test_cases: &[([u32; 8], [u32; 8])] =
            &[([0; 8], [0; 8]), ([1, 2, 3, 4, 5, 6, 7, 8], [8, 7, 6, 5, 4, 3, 2, 1]), ([0xDEADBEEF; 8], [0xCAFEBABE; 8])];

        for (payload_digest_words, rest_digest_words) in test_cases {
            let our_result = tx_id_v1(payload_digest_words, rest_digest_words);

            let mut kaspa_hasher = kaspa_hashes::TransactionV1Id::new();
            kaspa_hasher.update(bytemuck::cast_slice::<_, u8>(payload_digest_words));
            kaspa_hasher.update(bytemuck::cast_slice::<_, u8>(rest_digest_words));
            let kaspa_result = kaspa_hasher.finalize();

            let our_bytes: [u8; 32] = bytemuck::cast(our_result);

            assert_eq!(
                our_bytes,
                kaspa_result.as_bytes(),
                "tx_id_v1 mismatch for payload_digest={:?}, rest_digest={:?}",
                payload_digest_words,
                rest_digest_words
            );
        }
    }

    /// Helper to build a V1 rest_preimage (tx without payload) for testing
    fn build_v1_rest_preimage(output_value: u64, spk: &[u8; 34]) -> alloc::vec::Vec<u8> {
        let mut rest = alloc::vec::Vec::new();
        // version
        rest.extend_from_slice(&1u16.to_le_bytes());
        // 0 inputs
        rest.extend_from_slice(&0u64.to_le_bytes());
        // 1 output
        rest.extend_from_slice(&1u64.to_le_bytes());
        // output: value
        rest.extend_from_slice(&output_value.to_le_bytes());
        // output: spk_version
        rest.extend_from_slice(&0u16.to_le_bytes());
        // output: spk_len
        rest.extend_from_slice(&34u64.to_le_bytes());
        // output: spk
        rest.extend_from_slice(spk);
        // output: has_covenant = false
        rest.push(0);
        // locktime
        rest.extend_from_slice(&0u64.to_le_bytes());
        // subnetwork_id
        rest.extend_from_slice(&[0u8; 20]);
        // gas
        rest.extend_from_slice(&0u64.to_le_bytes());
        // empty_payload_len
        rest.extend_from_slice(&0u64.to_le_bytes());
        // mass
        rest.extend_from_slice(&0u64.to_le_bytes());
        rest
    }

    /// Test full output verification accepts valid witnesses
    #[test]
    fn test_output_verification_full_accepts_valid() {
        let spk = pay_to_pubkey_spk(&[0x42u8; 32]);
        let output_value = 1000u64;

        // Build full rest_preimage
        let rest_preimage = build_v1_rest_preimage(output_value, &spk);

        // Create V1 witness with rest_preimage and empty payload_digest
        let v1_witness = PrevTxV1Witness::new(0, AlignedBytes::from_bytes(&rest_preimage), [0u32; 8]);
        let witness = PrevTxWitness::V1(v1_witness);

        // Compute expected tx_id
        let expected_tx_id = witness.compute_tx_id();

        // Verify
        let result = verify_output_in_tx(&expected_tx_id, &witness);
        assert!(result.is_some(), "Verification should succeed");
        let output = result.unwrap();
        assert_eq!(output.value, output_value);
        assert_eq!(output.spk_as_p2pk().unwrap(), spk);
    }

    /// Test full output verification rejects tampered witnesses
    #[test]
    fn test_output_verification_full_rejects_tampered() {
        let spk = pay_to_pubkey_spk(&[0x42u8; 32]);
        let output_value = 1000u64;

        // Build full rest_preimage
        let rest_preimage = build_v1_rest_preimage(output_value, &spk);
        let v1_witness = PrevTxV1Witness::new(0, AlignedBytes::from_bytes(&rest_preimage), [0u32; 8]);
        let witness = PrevTxWitness::V1(v1_witness);
        let valid_tx_id = witness.compute_tx_id();

        // Test with wrong tx_id
        let wrong_tx_id = [0xDEADBEEFu32; 8];
        let result = verify_output_in_tx(&wrong_tx_id, &witness);
        assert!(result.is_none(), "Verification should fail for wrong tx_id");

        // Test with tampered SPK (different witness, same claimed tx_id)
        let tampered_spk = pay_to_pubkey_spk(&[0x43u8; 32]);
        let tampered_preimage = build_v1_rest_preimage(output_value, &tampered_spk);
        let tampered_v1 = PrevTxV1Witness::new(0, AlignedBytes::from_bytes(&tampered_preimage), [0u32; 8]);
        let tampered_witness = PrevTxWitness::V1(tampered_v1);

        // Try to verify tampered witness against original tx_id - should fail
        let result = verify_output_in_tx(&valid_tx_id, &tampered_witness);
        assert!(result.is_none(), "Verification should fail for tampered SPK");
    }

    /// Test V0 output verification
    #[test]
    fn test_output_verification_v0() {
        let spk = pay_to_pubkey_spk(&[0x42u8; 32]);
        let output_value = 500u64;

        // Build V0 full tx bytes
        let mut tx = alloc::vec::Vec::new();
        // version
        tx.extend_from_slice(&0u16.to_le_bytes());
        // 0 inputs
        tx.extend_from_slice(&0u64.to_le_bytes());
        // 1 output
        tx.extend_from_slice(&1u64.to_le_bytes());
        // output
        tx.extend_from_slice(&output_value.to_le_bytes());
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&34u64.to_le_bytes());
        tx.extend_from_slice(&spk);
        // locktime
        tx.extend_from_slice(&0u64.to_le_bytes());
        // subnetwork_id
        tx.extend_from_slice(&[0u8; 20]);
        // gas
        tx.extend_from_slice(&0u64.to_le_bytes());
        // payload_len
        tx.extend_from_slice(&0u64.to_le_bytes());

        let v0_witness = PrevTxV0Witness::new(0, tx);
        let witness = PrevTxWitness::V0(v0_witness);
        let expected_tx_id = witness.compute_tx_id();

        let result = verify_output_in_tx(&expected_tx_id, &witness);
        assert!(result.is_some(), "V0 verification should succeed");
        let output = result.unwrap();
        assert_eq!(output.value, output_value);
        assert_eq!(output.spk_as_p2pk().unwrap(), spk);
    }
}
