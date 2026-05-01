pub mod vk;
use super::super::result::Result;
use crate::{
    opcodes::codes::OpZkPrecompile,
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
use risc0_zkvm::{Digest, Groth16Receipt, Groth16ReceiptVerifierParameters, sha};

/// Splits a r0 digest into two 32 byte arrays
fn split_digest_bytes(d: Digest) -> ([u8; 32], [u8; 32]) {
    let bytes = d.as_bytes();
    let mut lo = [0u8; 32];
    let mut hi = [0u8; 32];
    lo[..16].copy_from_slice(&bytes[..16]);
    hi[..16].copy_from_slice(&bytes[16..32]);
    (lo, hi)
}

impl R0ScriptBuilder {
    /// Converts a Groth16Receipt into a Kaspa script.
    /// This script unlocks the UTXO if the verification of the receipt
    /// succeeds.
    pub fn from_groth<Claim: Digestible + Clone>(receipt: &Groth16Receipt<Claim>) -> Result<ScriptBuilder> {
        
        // Get the default parameters for the g16 verifier
        let params = Groth16ReceiptVerifierParameters::default();
        let seal = &receipt.seal;
        let digested_claim = receipt.claim.digest::<sha::Impl>();

        // Because a 32 byte sha256 digest can be too large 
        // for the bn254 field, we split the digest into two 16byte parts
        // and pad them. This way we can guarantee that we wont overflow.
        let (a0, a1) = split_digest_bytes(params.control_root);
        let (c0, c1) = split_digest_bytes(digested_claim);
        let id_bn254: [u8; 32] = params.bn254_control_id.into();

        // Decode the R0 seal.
        let seal = Seal::decode(seal).map_err(|e| R0Error::SealDecoding(e.to_string()))?;

        // Utilize the groth16 R0 VK, which is used to verify 
        // r0 circuits such as the lift program.
        let verifying_key = ark_groth16::VerifyingKey::<Bn254>::deserialize_uncompressed(R0_SERIALIZED_UNCOMPRESSED_VK.as_slice())?;
        
        // Create the groups
        let g1 = G1::from_r0_bytes(&seal.a)?;
        let g1_c = G1::from_r0_bytes(&seal.c)?;
        let g2 = G2::from_r0_bytes(&seal.b)?;

        // Create an ark proof from the R0 proof data
        let mut encoded_proof = Vec::new();

        // The proof itself is not to be used for script-specific logic
        // as such it should be serialized in its compressed form to save space. 
        let proof: Proof<ark_ec::bn::Bn<ark_bn254::Config>> = Proof::<Bn254> { a: g1.0, b: g2.0, c: g1_c.0 };
        proof.serialize_compressed(&mut encoded_proof)?;

        // Serialize the verifying key in compressed form as well, to save space
        // the verifying key here will never change, and as such it can be used
        // for script specific logic.
        let mut serialized_vk = Vec::new();
        verifying_key.serialize_compressed(&mut serialized_vk).map_err(|_| R0Error::BincodeVkSerialization)?;
        let mut builder = ScriptBuilder::new();

        // push all public inputs to the script.
        builder.add_data(&id_bn254)?; // control id
        builder.add_data(&c1)?;  // claim digest hi
        builder.add_data(&c0)?; // claim digest lo
        builder.add_data(&a1)?; // control root hi
        builder.add_data(&a0)?; // control root lo
        builder.add_i64(5)?; // Inform how many public inputs there are, 
        builder.add_data(&encoded_proof)?; // the proof itself, 
        builder.add_data(&serialized_vk)?; // the verifying key
        builder.add_data(&[ZkTag::Groth16 as u8])?; // the tag of which precompile.
        builder.add_op(OpZkPrecompile)?; // precomp opcode
        Ok(builder)
    }
}
