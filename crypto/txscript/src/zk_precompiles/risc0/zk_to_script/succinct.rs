use super::super::result::Result;
use crate::{
    opcodes::codes::OpZkPrecompile,
    script_builder::ScriptBuilder,
    zk_precompiles::{
        risc0::{rcpt::HashFnId, zk_to_script::{R0ScriptBuilder, UnboundedScript}},
        tags::ZkTag,
    },
};
use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, SuccinctReceipt, sha};
impl R0ScriptBuilder<UnboundedScript> {
    /// Converts a SuccinctReceipt into a Kaspa script.
    /// This script unlocks the UTXO if the verification of the receipt
    /// succeeds.
    pub fn from_succinct<Claim: Clone + Digestible>(
        receipt: &SuccinctReceipt<Claim>,
        journal: Digest,
        image_id: Digest,
    ) -> Result<ScriptBuilder> {
        // Initialize script builder
        let mut builder = ScriptBuilder::new();

        // The claim here might be already or not digested
        // but in either case we need to extract the digest
        // since that is what allows us to have a constant sized
        // stark proof.
        let serialized_claim: Digest = receipt.claim.digest::<sha::Impl>();
        
        builder.add_data(&serialized_claim.as_bytes())?;

        // Extract the control index and control digests
        // which are the merkle proof of inclusion.
        let (control_index, control_digests) = {
            (
                receipt.control_inclusion_proof.index.to_le_bytes(),
                receipt.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes().to_vec()).collect::<Vec<u8>>(),
            )
        };
        builder.add_data(&control_index)?;
        builder.add_data(&control_digests)?;

        // Add the seal but encode it as vec<u8>
        builder.add_data(&receipt.seal.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
        
        // Add the journal hash which contains the output of this
        // program
        builder.add_data(journal.as_bytes())?;

        // Add the image id which is the identifier of the program
        builder.add_data(image_id.as_bytes())?;

        // Add the identifier of which r0 circuit was executed.
        // Do note that the OpZkPrecompile R0 is expected to be used 
        // with the lift program, other programs might overrun the mass limit.
        builder.add_data(&receipt.control_id.as_bytes().to_vec())?;
        builder.add_data(&[HashFnId::try_from(receipt.hashfn.as_str())? as u8])?;

        // This is an r0 succinct proof.
        builder.add_data([ZkTag::R0Succinct as u8].as_ref())?;
        builder.add_op(OpZkPrecompile)?;
        Ok(builder)
    }
}
