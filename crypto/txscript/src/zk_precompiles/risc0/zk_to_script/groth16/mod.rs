pub mod vk;
use super::super::result::Result;
use crate::{
    opcodes::codes::OpZkPrecompile,
    script_builder::ScriptBuilder,
    zk_precompiles::{
        fields::Fr,
        points::{G1, G2, PointFromBytes},
        risc0::{
            R0Error,
            zk_to_script::{R0ScriptBuilder, groth16::vk::try_verifying_key},
        },
        tags::ZkTag,
    },
};
use ark_bn254::{Bn254, Config};
use ark_ec::bn::Bn;
use ark_groth16::{Proof, VerifyingKey};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use risc0_binfmt::Digestible;
use risc0_groth16::Seal;
use risc0_zkvm::{Digest, Groth16Receipt, Groth16ReceiptVerifierParameters, MaybePruned, SuccinctReceipt, sha};
fn split_digest_bytes(d: Digest) -> ([u8; 32], [u8; 32]) {
    let bytes = d.as_bytes();
    let mut lo = [0u8; 32];
    let mut hi = [0u8; 32];
    lo[..16].copy_from_slice(&bytes[..16]);
    hi[..16].copy_from_slice(&bytes[16..32]);
    (lo, hi)
}

fn to_fixed_array(input: &[u8]) -> [u8; 32] {
    let mut fixed_array = [0u8; 32];
    let start = core::cmp::max(32, input.len()) - core::cmp::min(32, input.len());
    fixed_array[start..].copy_from_slice(&input[input.len().saturating_sub(32)..]);
    fixed_array
}
impl R0ScriptBuilder {
    /// Converts a Groth16Receipt into a Kaspa script.
    /// This script unlocks the UTXO if the verification of the receipt
    /// succeeds.
    pub fn from_groth<Claim: Digestible + Clone>(receipt: &Groth16Receipt<Claim>) -> Result<ScriptBuilder> {
        let mut params = Groth16ReceiptVerifierParameters::default();
        let seal = &receipt.seal;
        let digested_claim = receipt.claim.digest::<sha::Impl>();
        let (a0, a1) = split_digest_bytes(params.control_root);
        let (c0, c1) = split_digest_bytes(digested_claim);
        let id_bn254 = to_fixed_array(params.bn254_control_id.as_bytes());
        let seal = Seal::decode(seal).map_err(|e| R0Error::SealDecoding(e.to_string()))?;
        let verifying_key = try_verifying_key()?;

        let g1 = G1::from_bytes(&seal.a)?;
        let g1_c = G1::from_bytes(&seal.c)?;
        let g2 = G2::from_bytes(&seal.b)?;
        let mut encoded_proof = Vec::new();
        let proof: Proof<ark_ec::bn::Bn<ark_bn254::Config>> = Proof::<Bn254> { a: g1.0, b: g2.0, c: g1_c.0 };
        proof.serialize_compressed(&mut encoded_proof)?;
        // Serialize with serde_ark feature which under the hood is just
        // uncompressed serialization.
        // Re-serialize then deserialize to get the inner ark VK
        let mut serialized_vk = Vec::new();
        verifying_key.serialize_compressed(&mut serialized_vk).map_err(|_| R0Error::BincodeVkSerialization)?;
        let mut builder = ScriptBuilder::new();
        builder.add_data(&id_bn254)?;
        builder.add_data(&c1)?;
        builder.add_data(&c0)?;
        builder.add_data(&a1)?;
        builder.add_data(&a0)?;
        builder.add_i64(5)?;
        builder.add_data(&encoded_proof)?;
        builder.add_data(&serialized_vk)?;
        builder.add_data(&[ZkTag::Groth16 as u8])?;
        builder.add_op(OpZkPrecompile)?;
        Ok(builder)
    }
}
//    build_zk_script(&[seal, claim, hashfn, control_index, control_digests, journal, image_id, vec![stark_tag]]).unwrap()
