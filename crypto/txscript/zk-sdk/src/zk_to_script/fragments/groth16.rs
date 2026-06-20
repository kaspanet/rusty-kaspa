use ark_bn254::Bn254;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, Groth16Receipt, Groth16ReceiptVerifierParameters};

use super::prepare::prepare_r0_groth16_proof;
use crate::error::Error;
use crate::result::Result;
use crate::zk_to_script::builder::R0_SERIALIZED_UNCOMPRESSED_VK;
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

/// Pushes the r0 groth16 witness material into a caller-owned builder.
///
/// Pre-stack:  `[..., journal_hash]`
/// Post-stack: `[..., journal_hash, compressed_proof]`
///
/// Only the compressed proof is pushed, the `journal_hash` is caller-owned and
/// must already be on the stack (the caller decides how it gets there: a
/// constant for a one-time covenant, or a runtime in-script computation).
pub fn push_r0_groth16_witness<Claim: Digestible + Clone>(builder: &mut ScriptBuilder, receipt: Groth16Receipt<Claim>) -> Result<()> {
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
    let params = Groth16ReceiptVerifierParameters::default();
    let (a0, a1) = split_digest_bytes(params.control_root);
    let id_bn254: [u8; 32] = params.bn254_control_id.into();

    // Bake the VK in. R0's Groth16 VK is fixed, but pinning it here means
    // a malicious spender can't swap it for a VK they control.
    let mut serialized_vk = Vec::new();
    let verifying_key = ark_groth16::VerifyingKey::<Bn254>::deserialize_uncompressed(R0_SERIALIZED_UNCOMPRESSED_VK.as_slice())?;
    verifying_key.serialize_compressed(&mut serialized_vk).map_err(|_| Error::VkSerialization)?;

    // Redeem-script entry leaves us with: [..., journal_hash, proof]  (top = proof)

    // Park `proof` on the alt stack so it doesn't clutter the working area.
    // `journal_hash` is left for the digest reconstruction below. We'll bring
    // the proof back later.
    builder.add_op(OpToAltStack)?; // alt:[..., proof]   main:[..., journal_hash]

    //  arrange [id_bn254, image_id, journal_hash] on top.
    builder.add_data(&id_bn254)?; // [..., journal_hash, id_bn254]
    builder.add_data(&image_id)?; // [..., journal_hash, id_bn254, image_id]
    builder.add_op(OpRot)?; // [..., id_bn254, image_id, journal_hash]

    // Recompute Output digest in a UTXO script.
    // SHA256( SHA256("risc0.Output") || journal_hash || ZERO || u16_le(2) )
    builder.add_data(
        [
            119, 234, 254, 179, 102, 167, 139, 71, 116, 125, 224, 215, 187, 23, 98, 132, 8, 95, 245, 86, 72, 135, 0, 154, 91, 230, 61,
            163, 45, 53, 89, 212,
        ]
        .as_slice(),
    )?;
    builder.add_op(OpSwap)?; // [..., image_id, tag_hash, journal_hash]
    builder.add_op(OpCat)?; // [..., image_id, tag_hash || journal_hash]
    builder.add_data(&[0u8; 32])?; // ZERO assumptions [..., image_id, tag||journal, ZERO]
    builder.add_op(OpCat)?; // [..., image_id, tag||journal||ZERO]
    builder.add_data(&2u16.to_le_bytes())?; // down count = 2 [..., image_id, tag||journal||ZERO||2] as per r0 hash construct
    builder.add_op(OpCat)?; // [..., image_id, output_digest_prehash]
    builder.add_op(OpSHA256)?; // [..., image_id, output_digest]

    // Recompute the ReceiptClaim digest in a UTXO script.
    // SHA256( SHA256("risc0.ReceiptClaim") || ZERO_input || image_id || post_digest
    //       || output_digest || u32_le(0) || u32_le(0) || u16_le(4) )
    builder.add_data(
        [
            203, 31, 239, 205, 31, 45, 154, 100, 151, 92, 187, 191, 110, 22, 30, 41, 20, 67, 75, 12, 187, 153, 96, 184, 77, 245, 215,
            23, 232, 107, 72, 175,
        ]
        .as_slice(),
    )?;
    builder.add_data(&[0u8; 32])?; // ZERO input
    builder.add_op(OpCat)?; // [..., image_id, output_digest, tag||ZERO]
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
    builder.add_data(&0u32.to_le_bytes())?; // sys_exit
    builder.add_op(OpCat)?; // [..., preamble||output||sys_exit]
    builder.add_data(&0u32.to_le_bytes())?; // user_exit
    builder.add_op(OpCat)?; // [..., preamble||output||sys_exit||user_exit]
    builder.add_data(&4u16.to_le_bytes())?; // [..., preamble||output||sys_exit||user_exit||4 (down_count)]
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
    builder.add_data(&serialized_vk)?;
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
    use risc0_binfmt::Digestible;
    use risc0_zkvm::{SystemState, sha};
    use sha2::Digest;

    #[test]
    fn test_post_digest_halted_zero() {
        let digest = SystemState { pc: 0, merkle_root: risc0_zkvm::Digest::ZERO }.digest::<sha::Impl>();
        assert_eq!(digest.as_bytes(), super::POST_DIGEST_HALTED_ZERO);
    }

    #[test]
    fn verify_tagged_struct_hashes() {
        let output = "risc0.Output";
        let receipt_claim = "risc0.ReceiptClaim";
        let output_hash = sha2::Sha256::digest(output.as_bytes());
        let receipt_claim_hash = sha2::Sha256::digest(receipt_claim.as_bytes());

        let output_hash_bytes: [u8; 32] = output_hash.into();
        let receipt_claim_hash_bytes: [u8; 32] = receipt_claim_hash.into();
        assert_eq!(
            output_hash_bytes,
            [
                119, 234, 254, 179, 102, 167, 139, 71, 116, 125, 224, 215, 187, 23, 98, 132, 8, 95, 245, 86, 72, 135, 0, 154, 91, 230,
                61, 163, 45, 53, 89, 212
            ]
        );
        assert_eq!(
            receipt_claim_hash_bytes,
            [
                203, 31, 239, 205, 31, 45, 154, 100, 151, 92, 187, 191, 110, 22, 30, 41, 20, 67, 75, 12, 187, 153, 96, 184, 77, 245,
                215, 23, 232, 107, 72, 175
            ]
        );
    }
}
