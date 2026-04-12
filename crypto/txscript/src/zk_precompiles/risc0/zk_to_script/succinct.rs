use crate::{opcodes::codes::OpZkPrecompile, script_builder::ScriptBuilder, zk_precompiles::{risc0::{rcpt::HashFnId, zk_to_script::R0ScriptBuilder}, tags::ZkTag}, };
use super::super::result::Result;
use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, SuccinctReceipt, sha::{ self, rust_crypto::Sha256}};
impl R0ScriptBuilder {
    /// Converts a SuccinctReceipt into a Kaspa script.
    /// This script unlocks the UTXO if the verification of the receipt
    /// succeeds. 
    pub fn from_succinct<Claim: Clone+Digestible>(
        receipt: &SuccinctReceipt<Claim>,
        journal: Digest,
        image_id: Digest,
    ) -> Result<ScriptBuilder> {
        let mut builder = ScriptBuilder::new();
        builder.add_data(&receipt.control_id.as_bytes().to_vec())?;
        builder.add_data(&receipt.seal.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
        let serialized_claim: Digest= receipt.claim.clone().digest::<sha::Impl>();
        builder.add_data(&serialized_claim.as_bytes().to_vec())?;
        builder.add_data(&[HashFnId::try_from(&receipt.hashfn)? as u8])?;
        let (control_index, control_digests) = {
            (
                receipt.control_inclusion_proof.index.to_le_bytes(),
                receipt.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes().to_vec()).collect::<Vec<u8>>(),
            )
        };
        builder.add_data(&control_index)?;
        builder.add_data(&control_digests)?;
        builder.add_data(journal.as_bytes())?;
        builder.add_data(image_id.as_bytes())?;
        builder.add_data(&[ZkTag::R0Succinct as u8].to_vec())?;
        builder.add_op(OpZkPrecompile)?;
        Ok(builder)
    }
}
//    build_zk_script(&[seal, claim, hashfn, control_index, control_digests, journal, image_id, vec![stark_tag]]).unwrap()
