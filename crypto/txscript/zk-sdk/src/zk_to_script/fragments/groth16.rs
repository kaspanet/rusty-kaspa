use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, Groth16Receipt, Groth16ReceiptVerifierParameters};

use super::prepare::prepare_r0_groth16_proof;
use crate::result::Result;
use kaspa_txscript::{
    opcodes::codes::{OpCat, OpDup, OpFromAltStack, OpRot, OpSHA256, OpSubstr, OpSwap, OpToAltStack, OpZkPrecompile},
    script_builder::ScriptBuilder,
    zk_precompiles::tags::ZkTag,
};

/// Splits a r0 digest into two 32-byte BN254-field-friendly halves.
fn split_digest_bytes(d: Digest) -> ([u8; 32], [u8; 32]) {
    let bytes = d.as_bytes();
    let mut lo = [0u8; 32];
    let mut hi = [0u8; 32];
    lo[..16].copy_from_slice(&bytes[..16]);
    hi[..16].copy_from_slice(&bytes[16..32]);
    (lo, hi)
}

/// SHA256 digest of `SystemState { pc: 0, merkle_root: Digest::ZERO }`.
/// Constant for any RISC Zero ReceiptClaim with `ExitCode::Halted(0)`.
const POST_DIGEST_HALTED_ZERO: [u8; 32] = [
    0xa3, 0xac, 0xc2, 0x71, 0x17, 0x41, 0x89, 0x96, 0x34, 0x0b, 0x84, 0xe5, 0xa9, 0x0f, 0x3e, 0xf4, 0xc4, 0x9d, 0x22, 0xc7, 0x9e,
    0x44, 0xaa, 0xd8, 0x22, 0xec, 0x9c, 0x31, 0x3e, 0x1e, 0xb8, 0xe2,
];

/// SHA256 digest of `"risc0.Output"`.
const OUTPUT_TAG_DIGEST: [u8; 32] = [
    0x77, 0xea, 0xfe, 0xb3, 0x66, 0xa7, 0x8b, 0x47, 0x74, 0x7d, 0xe0, 0xd7, 0xbb, 0x17, 0x62, 0x84, 0x08, 0x5f, 0xf5, 0x56, 0x48,
    0x87, 0x00, 0x9a, 0x5b, 0xe6, 0x3d, 0xa3, 0x2d, 0x35, 0x59, 0xd4,
];

/// ZERO assumptions digest || u16_le(2), completing `risc0.Output` hashing.
const OUTPUT_DIGEST_SUFFIX: [u8; 34] = [
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
];

/// SHA256("risc0.ReceiptClaim") || ZERO input digest.
const RECEIPT_CLAIM_PREFIX: [u8; 64] = [
    0xcb, 0x1f, 0xef, 0xcd, 0x1f, 0x2d, 0x9a, 0x64, 0x97, 0x5c, 0xbb, 0xbf, 0x6e, 0x16, 0x1e, 0x29, 0x14, 0x43, 0x4b, 0x0c, 0xbb,
    0x99, 0x60, 0xb8, 0x4d, 0xf5, 0xd7, 0x17, 0xe8, 0x6b, 0x48, 0xaf, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00,
];

/// u32_le(sys_exit = 0) || u32_le(user_exit = 0) || u16_le(4).
const RECEIPT_CLAIM_SUFFIX: [u8; 10] = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00];

/// Ark-serialized compressed Groth16 VK for the fixed R0 circuit. Precomputed
/// offchain so the verifier fragment can embed it directly in the script.
/// `verify_upstream_vk_roundtrip` pins this constant to
/// `risc0_groth16::verifying_key()`.
const R0_SERIALIZED_COMPRESSED_VK: [u8; 424] = [
    0xe2, 0xf2, 0x6d, 0xbe, 0xa2, 0x99, 0xf5, 0x22, 0x3b, 0x64, 0x6c, 0xb1, 0xfb, 0x33, 0xea, 0xdb, 0x05, 0x9d, 0x94, 0x07, 0x55,
    0x9d, 0x74, 0x41, 0xdf, 0xd9, 0x02, 0xe3, 0xa7, 0x9a, 0x4d, 0x2d, 0xab, 0xb7, 0x3d, 0xc1, 0x7f, 0xbc, 0x13, 0x02, 0x1e, 0x24,
    0x71, 0xe0, 0xc0, 0x8b, 0xd6, 0x7d, 0x84, 0x01, 0xf5, 0x2b, 0x73, 0xd6, 0xd0, 0x74, 0x83, 0x79, 0x4c, 0xad, 0x47, 0x78, 0x18,
    0x0e, 0x0c, 0x06, 0xf3, 0x3b, 0xbc, 0x4c, 0x79, 0xa9, 0xca, 0xde, 0xf2, 0x53, 0xa6, 0x80, 0x84, 0xd3, 0x82, 0xf1, 0x77, 0x88,
    0xf8, 0x85, 0xc9, 0xaf, 0xd1, 0x76, 0xf7, 0xcb, 0x2f, 0x03, 0x67, 0x89, 0xed, 0xf6, 0x92, 0xd9, 0x5c, 0xbd, 0xde, 0x46, 0xdd,
    0xda, 0x5e, 0xf7, 0xd4, 0x22, 0x43, 0x67, 0x79, 0x44, 0x5c, 0x5e, 0x66, 0x00, 0x6a, 0x42, 0x76, 0x1e, 0x1f, 0x12, 0xef, 0xde,
    0x00, 0x18, 0xc2, 0x12, 0xf3, 0xae, 0xb7, 0x85, 0xe4, 0x97, 0x12, 0xe7, 0xa9, 0x35, 0x33, 0x49, 0xaa, 0xf1, 0x25, 0x5d, 0xfb,
    0x31, 0xb7, 0xbf, 0x60, 0x72, 0x3a, 0x48, 0x0d, 0x92, 0x93, 0x93, 0x8e, 0x19, 0x33, 0x03, 0x3e, 0x7f, 0xea, 0x1f, 0x40, 0x60,
    0x4e, 0xaa, 0xcf, 0x69, 0x9d, 0x4b, 0xe9, 0xaa, 0xcc, 0x57, 0x70, 0x54, 0xa0, 0xdb, 0x22, 0xd9, 0x12, 0x9a, 0x17, 0x28, 0xff,
    0x85, 0xa0, 0x1a, 0x1c, 0x3a, 0xf8, 0x29, 0xb6, 0x2b, 0xf4, 0x91, 0x4c, 0x0b, 0xcf, 0x2c, 0x81, 0xa4, 0xbd, 0x57, 0x71, 0x90,
    0xef, 0xf5, 0xf1, 0x94, 0xee, 0x9b, 0xac, 0x95, 0xfa, 0xef, 0xd5, 0x3c, 0xb0, 0x03, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0xe4, 0x3b, 0xdc, 0x65, 0x5d, 0x0f, 0x9d, 0x73, 0x05, 0x35, 0x55, 0x4d, 0x9c, 0xaa, 0x61, 0x1d, 0xdd, 0x15, 0x2c, 0x08,
    0x1a, 0x06, 0xa9, 0x32, 0xa8, 0xe1, 0xd5, 0xdc, 0x25, 0x9a, 0xac, 0x12, 0x3f, 0x42, 0xa1, 0x88, 0xf6, 0x83, 0xd8, 0x69, 0x87,
    0x3c, 0xcc, 0x4c, 0x11, 0x94, 0x42, 0xe5, 0x7b, 0x05, 0x6e, 0x03, 0xe2, 0xfa, 0x92, 0xf2, 0x02, 0x8c, 0x97, 0xbc, 0x20, 0xb9,
    0x07, 0x87, 0x47, 0xc3, 0x0f, 0x85, 0x44, 0x46, 0x97, 0xfd, 0xf4, 0x36, 0xe3, 0x48, 0x71, 0x1c, 0x01, 0x11, 0x15, 0x96, 0x3f,
    0x85, 0x51, 0x97, 0x24, 0x3e, 0x4b, 0x39, 0xe6, 0xcb, 0xe2, 0x36, 0xca, 0x8b, 0xa7, 0xf2, 0x04, 0x2e, 0x11, 0xf9, 0x25, 0x5a,
    0xfb, 0xb6, 0xc6, 0xe2, 0xc3, 0xac, 0xcb, 0x88, 0xe4, 0x01, 0xf2, 0xaa, 0xc2, 0x1c, 0x09, 0x7c, 0x92, 0xb3, 0xfb, 0xdb, 0x99,
    0xf9, 0x8a, 0x9b, 0x0d, 0xcd, 0x6c, 0x07, 0x5a, 0xda, 0x6e, 0xd0, 0xdd, 0xfe, 0xce, 0x1d, 0x4a, 0x2d, 0x00, 0x5f, 0x61, 0xa7,
    0xd5, 0xdf, 0x0b, 0x75, 0xc1, 0x8a, 0x5b, 0x23, 0x74, 0xd6, 0x4e, 0x49, 0x5f, 0xab, 0x93, 0xd4, 0xc4, 0xb1, 0x20, 0x03, 0x94,
    0xd5, 0x25, 0x3c, 0xce, 0x2f, 0x25, 0xa5, 0x9b, 0x86, 0x2e, 0xe8, 0xe4, 0xcd, 0x43, 0x68, 0x66, 0x03, 0xfa, 0xa0, 0x9d, 0x5d,
    0x0d, 0x3c, 0x1c, 0x8f,
];

/// Converts an R0 Groth16 receipt into the compressed proof bytes expected by
/// the Kaspa Groth16 verifier and pushes them onto the provided builder.
///
/// This is typically called while building a signature script. The script that
/// invokes [`append_r0_groth16_verifier`] is responsible for producing or
/// placing `journal_hash` under this proof before the verifier runs.
///
/// Pre-stack:  `[...]`
/// Post-stack: `[..., compressed_proof]`
///
/// When paired with [`append_r0_groth16_verifier`], execution should reach the
/// verifier with:
///
/// Pre-stack:  `[..., journal_hash, compressed_proof]`
/// Post-stack: `[..., true]`
pub fn push_r0_groth16_proof<Claim: Digestible + Clone>(builder: &mut ScriptBuilder, receipt: Groth16Receipt<Claim>) -> Result<()> {
    let encoded_proof = prepare_r0_groth16_proof(&receipt)?;
    builder.add_data(&encoded_proof)?; // push the proof that asserts the claim
    Ok(())
}

/// Appends the r0-over-groth16 verifier fragment into a caller-owned builder.
///
/// Pre-stack:  `[..., journal_hash, compressed_proof]`
/// Post-stack: `[..., true]`
///
/// Embeds the image id, the fixed r0 groth16 verifier params / vk, the r0
/// receipt-claim reconstruction, the groth16 public-input shaping, and the
/// groth16 zk precompile call. This follows the convention of groth16 input
/// setup as per Risc0, but the verification itself is done by the generic
/// Arkworks implementation.
pub fn append_r0_groth16_verifier(builder: &mut ScriptBuilder, image_id: [u8; 32]) -> Result<()> {
    builder.add_data(&image_id)?; // [..., journal_hash, compressed_proof, image_id]
    append_r0_groth16_verifier_dynamic_image_id(builder)
}

/// Appends the r0-over-groth16 verifier fragment into a caller-owned builder.
///
/// Pre-stack:  `[..., journal_hash, compressed_proof, image_id]`
/// Post-stack: `[..., true]`
///
/// Embeds the image id, the fixed r0 groth16 verifier params / vk, the r0
/// receipt-claim reconstruction, the groth16 public-input shaping, and the
/// groth16 zk precompile call. This follows the convention of groth16 input
/// setup as per Risc0, but the verification itself is done by the generic
/// Arkworks implementation.
pub fn append_r0_groth16_verifier_dynamic_image_id(builder: &mut ScriptBuilder) -> Result<()> {
    let params = Groth16ReceiptVerifierParameters::default();
    let (a0, a1) = split_digest_bytes(params.control_root);
    let id_bn254: [u8; 32] = params.bn254_control_id.into();

    // Stack: [..., journal_hash, proof, image_id]  (top = image_id)

    builder.add_op(OpSwap)?; // [..., journal_hash, image_id, proof]

    // Park `proof` on the alt stack so it doesn't clutter the working area.
    // `journal_hash` is left for the digest reconstruction below. We'll bring
    // the proof back later.
    builder.add_op(OpToAltStack)?; // alt:[..., proof]   main:[..., journal_hash, image_id]

    //  arrange [id_bn254, image_id, journal_hash] on top.
    builder.add_data(&id_bn254)?; // [..., journal_hash, image_id, id_bn254]
    builder.add_op(OpSwap)?; // [..., journal_hash, id_bn254, image_id]
    builder.add_op(OpRot)?; // [..., id_bn254, image_id, journal_hash]

    // Recompute Output digest in a UTXO script.
    // SHA256( SHA256("risc0.Output") || journal_hash || ZERO || u16_le(2) )
    builder.add_data(&OUTPUT_TAG_DIGEST)?;
    builder.add_op(OpSwap)?; // [..., image_id, tag_hash, journal_hash]
    builder.add_op(OpCat)?; // [..., image_id, tag_hash || journal_hash]
    builder.add_data(&OUTPUT_DIGEST_SUFFIX)?;
    builder.add_op(OpCat)?; // [..., image_id, output_digest_prehash]
    builder.add_op(OpSHA256)?; // [..., image_id, output_digest]

    // Recompute the ReceiptClaim digest in a UTXO script.
    // SHA256( SHA256("risc0.ReceiptClaim") || ZERO_input || image_id || post_digest
    //       || output_digest || u32_le(0) || u32_le(0) || u16_le(4) )
    builder.add_data(&RECEIPT_CLAIM_PREFIX)?; // [..., image_id, output_digest, tag||ZERO]
    builder.add_op(OpRot)?; // [..., output_digest, tag||ZERO, image_id]
    builder.add_op(OpCat)?; // [..., output_digest, ...||image_id]
    builder.add_data(&POST_DIGEST_HALTED_ZERO)?; // [..., output_digest, ...||image_id, post_digest]
    builder.add_op(OpCat)?; // [..., output_digest, preamble_for_claim_hash]
    // Naively we would compute output digest before ths receiptclaim digest, but the
    // receipt digest depends on output, and so therefore we are forced to do this swap.
    builder.add_op(OpSwap)?; // [..., preamble_for_claim_hash, output_digest]
    builder.add_op(OpCat)?; // [..., preamble_for_claim_hash || output_digest]
    // R0 at the moment hardcodes exit codes in their construction of the receipt
    // claim digest, since this code is not consensus critical, we might as well do the same.
    builder.add_data(&RECEIPT_CLAIM_SUFFIX)?;
    builder.add_op(OpCat)?; // [..., concatenated_data_for_hash]
    builder.add_op(OpSHA256)?; // [..., id_bn254, computed_claim_digest]

    // Since g16 cant hold full 256 values due to operation on smaller field
    // we should split the digests and zero pad them. As such:
    // c0 = digest[0..16]  || zero_pad[16]
    // c1 = digest[16..32] || zero_pad[16]
    builder.add_op(OpDup)?; // [..., computed_claim_digest, computed_claim_digest]
    builder.add_i64(16)?;
    builder.add_i64(32)?;
    builder.add_op(OpSubstr)?; // [..., computed_claim_digest, hi16]
    builder.add_data(&[0u8; 16])?;
    builder.add_op(OpCat)?; // [..., computed_claim_digest, c1]
    builder.add_op(OpSwap)?; // [..., c1, computed_claim_digest]
    // compute the c0
    builder.add_i64(0)?;
    builder.add_i64(16)?;
    // take the lower 16 bits of the hash
    builder.add_op(OpSubstr)?; // [..., c1, lo16]
    builder.add_data(&[0u8; 16])?;
    builder.add_op(OpCat)?; // [..., c1, c0]

    // assemble the precompile inputs and dispatch.
    // id_bn254, c1, c0, a1, a0, 5, proof, vk, tag
    builder.add_data(&a1)?;
    builder.add_data(&a0)?;
    builder.add_i64(5)?;
    builder.add_op(OpFromAltStack)?; // [..., 5, proof]
    // R0's Groth16 VK is fixed, but pinning it here means a malicious spender
    // can't swap it for a VK they control.
    builder.add_data(&R0_SERIALIZED_COMPRESSED_VK)?;
    builder.add_data(&[ZkTag::Groth16 as u8])?;
    builder.add_op(OpZkPrecompile)?; // [..., true (hopefully)]

    Ok(())
}

/// Appends a fixed-journal r0-over-groth16 verifier fragment into a caller-owned
/// builder, binding the verification to a single `journal_hash` baked into the
/// script
///
/// Pre-stack:  `[..., compressed_proof]`
/// Post-stack: `[..., true]`
///
pub fn append_r0_groth16_verifier_with_fixed_journal(
    builder: &mut ScriptBuilder,
    image_id: [u8; 32],
    journal_hash: [u8; 32],
) -> Result<()> {
    // [..., proof] -> [..., proof, journal_hash] -> [..., journal_hash, proof]
    builder.add_data(&journal_hash)?;
    builder.add_op(OpSwap)?;
    append_r0_groth16_verifier(builder, image_id)
}

#[cfg(test)]
mod tests {
    use ark_bn254::Bn254;
    use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
    use risc0_binfmt::Digestible;
    use risc0_zkvm::{SystemState, sha};
    use sha2::Digest;

    use crate::zk_to_script::fragments::groth16::R0_SERIALIZED_COMPRESSED_VK;

    #[test]
    fn test_post_digest_halted_zero() {
        let digest = SystemState { pc: 0, merkle_root: risc0_zkvm::Digest::ZERO }.digest::<sha::Impl>();
        assert_eq!(digest.as_bytes(), super::POST_DIGEST_HALTED_ZERO);
    }

    #[test]
    fn verify_upstream_vk_roundtrip() {
        let upstream_vk = risc0_groth16::verifying_key();

        // R0 uses a custom serialization format for the VK which at
        // the end state uses bincode for serialization.
        // See: risc0_groth16::verifier
        // mod serde_ark
        let upstream_serialized_vk = bincode::serialize(&upstream_vk).unwrap();

        // Decode the static VK and ensure it can be deserialized
        let deserialized = ark_groth16::VerifyingKey::<Bn254>::deserialize_compressed(R0_SERIALIZED_COMPRESSED_VK.as_slice()).unwrap();

        // Now serialize it uncompressed and ensure it matches the upstream serialized VK
        let mut uncompressed_serialized_key = Vec::new();
        deserialized.serialize_uncompressed(&mut uncompressed_serialized_key).unwrap();

        assert_eq!(bincode::serialize(&uncompressed_serialized_key).unwrap(), upstream_serialized_vk);
    }

    #[test]
    fn verify_tagged_struct_hashes() {
        let output = "risc0.Output";
        let receipt_claim = "risc0.ReceiptClaim";
        let output_hash = sha2::Sha256::digest(output.as_bytes());
        let receipt_claim_hash = sha2::Sha256::digest(receipt_claim.as_bytes());

        let output_hash_bytes: [u8; 32] = output_hash.into();
        let receipt_claim_hash_bytes: [u8; 32] = receipt_claim_hash.into();
        assert_eq!(output_hash_bytes, super::OUTPUT_TAG_DIGEST);
        assert_eq!(receipt_claim_hash_bytes, super::RECEIPT_CLAIM_PREFIX[..32]);
        assert_eq!(super::RECEIPT_CLAIM_PREFIX[32..], [0u8; 32]);
    }
}
