//! Previous transaction output verification.
//!
//! This module provides structures and functions for verifying that an output
//! is committed in a previous transaction by its tx_id.
//!
//! For proper verification, the host provides the full transaction preimage
//! and the guest:
//! 1. Hashes it to compute tx_id
//! 2. Verifies computed tx_id matches claimed prev_tx_id
//! 3. Parses the preimage to extract output SPK at claimed index

use alloc::vec::Vec;

use crate::AlignedBytes;

/// Covenant binding (optional in outputs for V1+ transactions)
#[derive(Clone, Copy, Debug, Eq, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct CovenantBinding {
    /// Authorizing input index (u16)
    pub authorizing_input: u16,
    /// Padding for alignment
    pub _padding: [u8; 2],
    /// Covenant ID (32 bytes)
    pub covenant_id: [u8; 32],
}

impl CovenantBinding {
    pub const SIZE: usize = core::mem::size_of::<Self>();

    pub fn new(authorizing_input: u16, covenant_id: [u8; 32]) -> Self {
        Self { authorizing_input, _padding: [0; 2], covenant_id }
    }
}

/// Witness for V0 previous transaction.
///
/// V0 tx_id = blake2b(preimage)
/// The preimage is the full serialized transaction.
#[derive(Clone, Debug)]
pub struct PrevTxV0Witness {
    /// The output index to verify
    pub output_index: u32,
    /// Full serialized transaction (hashed for tx_id, parsed for output)
    pub preimage: Vec<u8>,
}

impl PrevTxV0Witness {
    pub fn new(output_index: u32, preimage: Vec<u8>) -> Self {
        Self { output_index, preimage }
    }

    /// Compute the tx_id from the preimage
    pub fn compute_tx_id(&self) -> [u32; 8] {
        crate::tx_id_v0(&self.preimage)
    }

    /// Parse the preimage to extract the output at output_index
    pub fn extract_output(&self) -> Option<OutputData> {
        parse_output_at_index(&self.preimage, self.output_index, 0)
    }
}

/// Witness for V1 previous transaction.
///
/// V1 tx_id = blake3(payload_digest || rest_digest)
/// where rest_digest = blake3(rest_preimage)
///
/// For V1, we only need the payload_digest (32 bytes), not the full payload.
/// The rest_preimage is needed to compute rest_digest and parse output SPK.
#[derive(Clone, Debug)]
pub struct PrevTxV1Witness {
    /// The output index to verify
    pub output_index: u32,
    /// The "rest" part of the transaction (without payload)
    /// Used to compute rest_digest and parse output SPK
    pub rest_preimage: AlignedBytes,
    /// Pre-computed payload digest (host computes this from payload bytes)
    pub payload_digest: [u32; 8],
}

impl PrevTxV1Witness {
    pub fn new(output_index: u32, rest_preimage: AlignedBytes, payload_digest: [u32; 8]) -> Self {
        Self { output_index, rest_preimage, payload_digest }
    }

    /// Compute the tx_id from rest_preimage and payload_digest
    pub fn compute_tx_id(&self) -> [u32; 8] {
        let rest_digest = crate::rest_digest_bytes(self.rest_preimage.as_bytes());
        crate::tx_id_v1(&self.payload_digest, &rest_digest)
    }

    /// Parse the rest_preimage to extract the output at output_index
    pub fn extract_output(&self) -> Option<OutputData> {
        parse_output_at_index(self.rest_preimage.as_bytes(), self.output_index, 1)
    }
}

/// Combined witness for previous transaction (either V0 or V1)
#[derive(Clone, Debug)]
pub enum PrevTxWitness {
    V0(PrevTxV0Witness),
    V1(PrevTxV1Witness),
}

impl PrevTxWitness {
    /// Compute the tx_id from the witness data
    pub fn compute_tx_id(&self) -> [u32; 8] {
        match self {
            PrevTxWitness::V0(w) => w.compute_tx_id(),
            PrevTxWitness::V1(w) => w.compute_tx_id(),
        }
    }

    /// Parse the preimage to extract the output SPK at output_index
    pub fn extract_output(&self) -> Option<OutputData> {
        match self {
            PrevTxWitness::V0(w) => w.extract_output(),
            PrevTxWitness::V1(w) => w.extract_output(),
        }
    }

    /// Get the output index
    pub fn output_index(&self) -> u32 {
        match self {
            PrevTxWitness::V0(w) => w.output_index,
            PrevTxWitness::V1(w) => w.output_index,
        }
    }
}

/// Parsed output data
#[derive(Clone, Debug)]
pub struct OutputData {
    pub value: u64,
    pub spk_version: u16,
    pub spk: Vec<u8>,
    pub covenant: Option<CovenantBinding>,
}

impl OutputData {
    /// Try to get SPK as fixed 34-byte array (p2pk format)
    pub fn spk_as_p2pk(&self) -> Option<[u8; 34]> {
        if self.spk.len() == 34 {
            let mut arr = [0u8; 34];
            arr.copy_from_slice(&self.spk);
            Some(arr)
        } else {
            None
        }
    }
}

/// Parse transaction bytes to extract output at specified index.
///
/// Transaction format (simplified):
/// - version: u16
/// - num_inputs: u64
/// - inputs: [input; num_inputs]
/// - num_outputs: u64
/// - outputs: [output; num_outputs]
/// - locktime: u64
/// - ... (rest depends on version)
///
/// Each input:
/// - prev_tx_id: [u8; 32]
/// - prev_index: u32
/// - sig_script_len: u64
/// - sig_script: [u8; sig_script_len]
/// - sequence: u64
///
/// Each output:
/// - value: u64
/// - spk_version: u16
/// - spk_len: u64
/// - spk: [u8; spk_len]
/// - (V1 only) has_covenant: u8
/// - (if has_covenant) authorizing_input: u16, covenant_id: [u8; 32]
pub fn parse_output_at_index(tx_bytes: &[u8], output_index: u32, tx_version: u16) -> Option<OutputData> {
    let mut cursor = 0;

    // Skip version (2 bytes)
    cursor += 2;
    if cursor > tx_bytes.len() {
        return None;
    }

    // Read num_inputs (u64)
    let num_inputs = read_u64(tx_bytes, &mut cursor)?;

    // Skip all inputs
    for _ in 0..num_inputs {
        // prev_tx_id (32 bytes)
        cursor += 32;
        // prev_index (4 bytes)
        cursor += 4;
        // sig_script_len (u64) + sig_script
        let sig_len = read_u64(tx_bytes, &mut cursor)?;
        cursor += sig_len as usize;
        // sequence (8 bytes)
        cursor += 8;

        if cursor > tx_bytes.len() {
            return None;
        }
    }

    // Read num_outputs (u64)
    let num_outputs = read_u64(tx_bytes, &mut cursor)?;

    if output_index as u64 >= num_outputs {
        return None;
    }

    // Parse outputs until we reach output_index
    for i in 0..num_outputs {
        let value = read_u64(tx_bytes, &mut cursor)?;
        let spk_version = read_u16(tx_bytes, &mut cursor)?;
        let spk_len = read_u64(tx_bytes, &mut cursor)?;

        if cursor + spk_len as usize > tx_bytes.len() {
            return None;
        }
        let spk = tx_bytes[cursor..cursor + spk_len as usize].to_vec();
        cursor += spk_len as usize;

        // V1 has optional covenant
        let covenant = if tx_version >= 1 {
            if cursor >= tx_bytes.len() {
                return None;
            }
            let has_covenant = tx_bytes[cursor];
            cursor += 1;

            if has_covenant != 0 {
                let auth_input = read_u16(tx_bytes, &mut cursor)?;
                if cursor + 32 > tx_bytes.len() {
                    return None;
                }
                let mut covenant_id = [0u8; 32];
                covenant_id.copy_from_slice(&tx_bytes[cursor..cursor + 32]);
                cursor += 32;
                Some(CovenantBinding::new(auth_input, covenant_id))
            } else {
                None
            }
        } else {
            None
        };

        if i == output_index as u64 {
            return Some(OutputData { value, spk_version, spk, covenant });
        }
    }

    None
}

/// Read u64 little-endian from buffer
fn read_u64(buf: &[u8], cursor: &mut usize) -> Option<u64> {
    if *cursor + 8 > buf.len() {
        return None;
    }
    let val = u64::from_le_bytes(buf[*cursor..*cursor + 8].try_into().ok()?);
    *cursor += 8;
    Some(val)
}

/// Read u16 little-endian from buffer
fn read_u16(buf: &[u8], cursor: &mut usize) -> Option<u16> {
    if *cursor + 2 > buf.len() {
        return None;
    }
    let val = u16::from_le_bytes(buf[*cursor..*cursor + 2].try_into().ok()?);
    *cursor += 2;
    Some(val)
}

/// Verify an output is in a previous transaction.
///
/// 1. Computes tx_id from the witness preimage
/// 2. Verifies it matches claimed_tx_id
/// 3. Parses the preimage to extract the output SPK
///
/// Returns the verified output data if successful.
pub fn verify_output_in_tx(claimed_tx_id: &[u32; 8], witness: &PrevTxWitness) -> Option<OutputData> {
    // 1. Compute tx_id from preimage
    let computed_tx_id = witness.compute_tx_id();

    // 2. Verify tx_id matches
    if computed_tx_id != *claimed_tx_id {
        return None;
    }

    // 3. Parse and extract output
    witness.extract_output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_covenant_binding_size() {
        assert_eq!(CovenantBinding::SIZE, 36);
    }

    #[test]
    fn test_parse_output_v0() {
        let mut tx = Vec::new();
        tx.extend_from_slice(&0u16.to_le_bytes()); // version
        tx.extend_from_slice(&0u64.to_le_bytes()); // 0 inputs
        tx.extend_from_slice(&1u64.to_le_bytes()); // 1 output

        tx.extend_from_slice(&1000u64.to_le_bytes());
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&34u64.to_le_bytes());
        tx.extend_from_slice(&[0x42u8; 34]);

        let output = parse_output_at_index(&tx, 0, 0).expect("should parse");
        assert_eq!(output.value, 1000);
        assert_eq!(output.spk_version, 0);
        assert_eq!(output.spk.len(), 34);
        assert!(output.spk.iter().all(|&b| b == 0x42));
        assert!(output.covenant.is_none());
    }

    #[test]
    fn test_parse_output_v1_no_covenant() {
        let mut tx = Vec::new();
        tx.extend_from_slice(&1u16.to_le_bytes());
        tx.extend_from_slice(&0u64.to_le_bytes());
        tx.extend_from_slice(&1u64.to_le_bytes());

        tx.extend_from_slice(&2000u64.to_le_bytes());
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&34u64.to_le_bytes());
        tx.extend_from_slice(&[0x43u8; 34]);
        tx.push(0); // has_covenant = false

        let output = parse_output_at_index(&tx, 0, 1).expect("should parse");
        assert_eq!(output.value, 2000);
        assert_eq!(output.spk[0], 0x43);
        assert!(output.covenant.is_none());
    }

    #[test]
    fn test_parse_output_v1_with_covenant() {
        let mut tx = Vec::new();
        tx.extend_from_slice(&1u16.to_le_bytes());
        tx.extend_from_slice(&0u64.to_le_bytes());
        tx.extend_from_slice(&1u64.to_le_bytes());

        tx.extend_from_slice(&3000u64.to_le_bytes());
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&34u64.to_le_bytes());
        tx.extend_from_slice(&[0x44u8; 34]);
        tx.push(1); // has_covenant = true
        tx.extend_from_slice(&7u16.to_le_bytes());
        tx.extend_from_slice(&[0xBB; 32]);

        let output = parse_output_at_index(&tx, 0, 1).expect("should parse");
        assert_eq!(output.value, 3000);
        let cov = output.covenant.expect("should have covenant");
        assert_eq!(cov.authorizing_input, 7);
        assert_eq!(cov.covenant_id, [0xBB; 32]);
    }

    #[test]
    fn test_parse_multiple_outputs() {
        let mut tx = Vec::new();
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&0u64.to_le_bytes());
        tx.extend_from_slice(&3u64.to_le_bytes());

        for (i, val) in [100u64, 200, 300].iter().enumerate() {
            tx.extend_from_slice(&val.to_le_bytes());
            tx.extend_from_slice(&0u16.to_le_bytes());
            tx.extend_from_slice(&34u64.to_le_bytes());
            tx.extend_from_slice(&[(i + 1) as u8; 34]);
        }

        let out0 = parse_output_at_index(&tx, 0, 0).expect("should parse");
        assert_eq!(out0.value, 100);
        assert_eq!(out0.spk[0], 0x01);

        let out1 = parse_output_at_index(&tx, 1, 0).expect("should parse");
        assert_eq!(out1.value, 200);
        assert_eq!(out1.spk[0], 0x02);

        let out2 = parse_output_at_index(&tx, 2, 0).expect("should parse");
        assert_eq!(out2.value, 300);
        assert_eq!(out2.spk[0], 0x03);

        assert!(parse_output_at_index(&tx, 3, 0).is_none());
    }

    #[test]
    fn test_parse_with_inputs() {
        let mut tx = Vec::new();
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&1u64.to_le_bytes()); // 1 input

        // Input
        tx.extend_from_slice(&[0xAA; 32]); // prev_tx_id
        tx.extend_from_slice(&0u32.to_le_bytes()); // prev_index
        tx.extend_from_slice(&0u64.to_le_bytes()); // sig_script_len = 0
        tx.extend_from_slice(&0u64.to_le_bytes()); // sequence

        tx.extend_from_slice(&1u64.to_le_bytes()); // 1 output
        tx.extend_from_slice(&500u64.to_le_bytes());
        tx.extend_from_slice(&0u16.to_le_bytes());
        tx.extend_from_slice(&34u64.to_le_bytes());
        tx.extend_from_slice(&[0x55u8; 34]);

        let output = parse_output_at_index(&tx, 0, 0).expect("should parse");
        assert_eq!(output.value, 500);
        assert_eq!(output.spk[0], 0x55);
    }
}
