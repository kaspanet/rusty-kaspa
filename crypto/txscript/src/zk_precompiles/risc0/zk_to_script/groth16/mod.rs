pub mod vk;
use super::super::result::Result;
use crate::{
    opcodes::codes::{
        OpCat, OpDup, OpFromAltStack, OpRot, OpSHA256, OpSubstr, OpSwap, OpToAltStack,
        OpZkPrecompile,
    },
    script_builder::ScriptBuilder,
    zk_precompiles::{
        points::{G1, G2, PointFromBytes},
        risc0::{
            R0Error,
            zk_to_script::{R0ScriptBuilder, groth16::vk::R0_SERIALIZED_UNCOMPRESSED_VK},
        },
        tags::ZkTag,
    },
};
use ark_bn254::Bn254;
use ark_groth16::Proof;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use risc0_binfmt::Digestible;
use risc0_groth16::Seal;
use risc0_zkvm::{Digest, Groth16Receipt, Groth16ReceiptVerifierParameters};

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
    0xa3, 0xac, 0xc2, 0x71, 0x17, 0x41, 0x89, 0x96,
    0x34, 0x0b, 0x84, 0xe5, 0xa9, 0x0f, 0x3e, 0xf4,
    0xc4, 0x9d, 0x22, 0xc7, 0x9e, 0x44, 0xaa, 0xd8,
    0x22, 0xec, 0x9c, 0x31, 0x3e, 0x1e, 0xb8, 0xe2,
];

// Pinned by being part of these script bytes:
//   - id_bn254          (selects which Groth16 verifier / trusted setup)
//   - control_root      (verifier-side root, split into a0/a1)
//   - image_id          (the RISC Zero program being attested to)
//   - serialized VK     (the Groth16 verifying key)
//   - num_inputs (5)
//   - ZkTag::Groth16    (so a different ZK system can't be swapped in)
//   - OpZkPrecompile    (so the precompile actually runs)
//   - All claim-digest recomputation logic (Stages C–E)
//
// Spender supplies (via the spending script):
//   - proof
//   - journal_hash
//
// Stack contract at start of locking-script execution (top -> bottom):
//   journal_hash
//   proof

/// Append the Groth16 RISC Zero locking-script logic to `builder`.
///
/// The caller may have already pushed locking-side prelude logic (e.g.
/// covenant checks, timelocks); this function appends the binding +
/// verification logic on top. After this returns, the script ends with
/// a truthy `[1]` left by `OpZkPrecompile` on success.
pub fn append_locking_groth16<'a>(
    builder: &'a mut ScriptBuilder,
    image_id: &[u8; 32],
) -> Result<&'a mut ScriptBuilder> {
    let params = Groth16ReceiptVerifierParameters::default();
    let (a0, a1) = split_digest_bytes(params.control_root);
    let id_bn254: [u8; 32] = params.bn254_control_id.into();

    // Bake the VK in. R0's Groth16 VK is fixed, but pinning it here means
    // a malicious spender can't swap it for a VK they control.
    let mut serialized_vk = Vec::new();
    let verifying_key = ark_groth16::VerifyingKey::<Bn254>::deserialize_uncompressed(
        R0_SERIALIZED_UNCOMPRESSED_VK.as_slice(),
    )?;
    verifying_key
        .serialize_compressed(&mut serialized_vk)
        .map_err(|_| R0Error::BincodeVkSerialization)?;

    // Spending script left us with: [..., proof, journal_hash]   (top = journal_hash)

    // Stage A: park `proof` on the alt stack so it doesn't clutter the
    // working area. We'll bring it back at Stage F.
    builder.add_op(OpToAltStack)?; // alt:[..., proof]   main:[..., journal_hash]

    // Stage B: arrange [id_bn254, image_id, journal_hash] on top.
    builder.add_data(&id_bn254)?; // [..., journal_hash, id_bn254]
    builder.add_data(image_id)?;  // [..., journal_hash, id_bn254, image_id]
    builder.add_op(OpRot)?; // [..., id_bn254, image_id, journal_hash]

    // Recompute Output digest in a UTXO script.
    // SHA256( SHA256("risc0.Output") || journal_hash || ZERO || u16_le(2) )
    builder.add_data(b"risc0.Output")?;
    builder.add_op(OpSHA256)?; // [..., image_id, journal_hash, tag_hash]
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
    builder.add_data(b"risc0.ReceiptClaim")?;
    builder.add_op(OpSHA256)?; // [..., image_id, output_digest, tag_hash]
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

    // Stage F: assemble the precompile inputs and dispatch.
    // id_bn254, c1, c0, a1, a0, 5, proof, vk, tag
    builder.add_data(&a1)?;
    builder.add_data(&a0)?;
    builder.add_i64(5)?;
    builder.add_op(OpFromAltStack)?; // [..., 5, proof]
    builder.add_data(&serialized_vk)?;
    builder.add_data(&[ZkTag::Groth16 as u8])?;
    builder.add_op(OpZkPrecompile)?; // [..., true (hopefully)]

    Ok(builder)
}

/// Spending script: push journal_hash first, then proof. After execution
/// the stack is [journal_hash, proof] with proof on top — what
/// `append_locking_groth16` expects.
pub fn append_spending_groth16<'a, Claim: Digestible + Clone>(
    builder: &'a mut ScriptBuilder,
    receipt: &Groth16Receipt<Claim>,
    journal_hash: &[u8; 32],
) -> Result<&'a mut ScriptBuilder> {
    let seal = Seal::decode(&receipt.seal).map_err(|e| R0Error::SealDecoding(e.to_string()))?;
    let g1 = G1::from_r0_bytes(&seal.a)?;
    let g1_c = G1::from_r0_bytes(&seal.c)?;
    let g2 = G2::from_r0_bytes(&seal.b)?;

    let proof: Proof<ark_ec::bn::Bn<ark_bn254::Config>> =
        Proof::<Bn254> { a: g1.0, b: g2.0, c: g1_c.0 };
    let mut encoded_proof = Vec::new();
    proof.serialize_compressed(&mut encoded_proof)?;

    builder.add_data(journal_hash)?; // push the journal hash, i.e. what we claim to be
    builder.add_data(&encoded_proof)?; // push the proof that asserts the claim
    Ok(builder)
}