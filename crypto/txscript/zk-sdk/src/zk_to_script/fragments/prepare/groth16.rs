use crate::error::Error;
use crate::points::{G1, G2, PointFromBytes};
use crate::result::Result;
use ark_bn254::Bn254;
use ark_groth16::Proof;
use ark_serialize::CanonicalSerialize;
use risc0_binfmt::Digestible;
use risc0_groth16::Seal;
use risc0_zkvm::Groth16Receipt;

/// Maps a RISC0 Groth16 receipt to the compressed ark-groth16 proof bytes that
/// the verifier fragment expects on the stack.
///
/// This is a pure transformation with no [`ScriptBuilder`] involvement: a
/// transaction builder can call it once, cache the bytes, and push them as a
/// plain data push without re-running the curve math. [`push_r0_groth16_witness`]
/// is implemented on top of this function.
///
/// [`ScriptBuilder`]: kaspa_txscript::script_builder::ScriptBuilder
/// [`push_r0_groth16_witness`]: super::super::push_r0_groth16_witness
pub fn prepare_r0_groth16_proof<Claim: Digestible + Clone>(receipt: &Groth16Receipt<Claim>) -> Result<Vec<u8>> {
    // Decode the seal.
    let seal = Seal::decode(&receipt.seal).map_err(|e| Error::SealDecoding(e.to_string()))?;

    // Decode the bytes into group elements.
    let g1 = G1::from_r0_bytes(&seal.a)?;
    let g1_c = G1::from_r0_bytes(&seal.c)?;
    let g2 = G2::from_r0_bytes(&seal.b)?;

    // Assemble the three group elements into a proof.
    let proof = Proof::<Bn254> { a: g1.0, b: g2.0, c: g1_c.0 };

    // Operations on the proof are not required, therefore we use compressed form.
    let mut encoded_proof = Vec::new();
    proof.serialize_compressed(&mut encoded_proof)?;
    Ok(encoded_proof)
}
