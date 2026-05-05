mod vk;

use super::Result;
pub use crate::zk_precompiles::risc0::zk_to_script::builder::proof::groth16::vk::R0_SERIALIZED_UNCOMPRESSED_VK;
use crate::zk_precompiles::risc0::zk_to_script::{BoundedR0Groth16Script, R0ScriptBuilder};
use crate::zk_precompiles::{
    points::{G1, G2, PointFromBytes},
    risc0::R0Error,
};
use ark_bn254::Bn254;
use ark_groth16::Proof;
use ark_serialize::CanonicalSerialize;
use risc0_binfmt::Digestible;
use risc0_groth16::Seal;
use risc0_zkvm::Groth16Receipt;

impl R0ScriptBuilder<BoundedR0Groth16Script> {
    /// Add the proof to an existing groth16 commit script
    /// returning the full script (including the signature).
    /// The mental model here is that we commit first, then redeem
    /// but since the script execution expects the signature script first,
    /// we need to place the commitment in the end.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(
        mut self,
        receipt: Groth16Receipt<Claim>,
        journal_hash: [u8; 32],
    ) -> Result<Vec<u8>> {
        let commit_script = self.builder.drain();

        // Decode the seal
        let seal = Seal::decode(&receipt.seal).map_err(|e| R0Error::SealDecoding(e.to_string()))?;

        // Decode the bytes into group elements.
        let g1 = G1::from_r0_bytes(&seal.a)?;
        let g1_c = G1::from_r0_bytes(&seal.c)?;
        let g2 = G2::from_r0_bytes(&seal.b)?;

        // Create the three group elements.
        let proof: Proof<ark_ec::bn::Bn<ark_bn254::Config>> = Proof::<Bn254> { a: g1.0, b: g2.0, c: g1_c.0 };
        let mut encoded_proof = Vec::new();

        // Operations on proof is not deemed to be of necessity
        // therefore we use compressed form.
        proof.serialize_compressed(&mut encoded_proof)?;

        self.builder.add_data(&journal_hash)?; // push the journal hash, i.e. what we claim to be
        self.builder.add_data(&encoded_proof)?; // push the proof that asserts the claim

        self.builder.add_data(&commit_script)?;

        // Thats it, now the commit script will consume these inputs and execute the
        // program.
        Ok(self.builder.drain())
    }
}
