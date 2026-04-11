use crate::{opcodes::codes::OpZkPrecompile, script_builder::ScriptBuilder, zk_precompiles::zk_to_script::ZkScriptBuilder};

use super::super::result::Result;
use ark_relations::r1cs::ToConstraintField;
use kaspa_utils::vec;
use risc0_zkvm::{Digest, SuccinctReceipt};
impl ZkScriptBuilder {
    /// Converts a SuccinctReceipt into a Kaspa script.
    /// This script unlocks the UTXO if the verification of the receipt
    /// succeeds. 
    pub fn from_succinct<Claim: Into<Vec<u8>> + Clone>(
        receipt: &SuccinctReceipt<Claim>,
        journal: Digest,
        image_id: Digest,
    ) -> Result<ScriptBuilder> {
        let mut builder = ScriptBuilder::new();
        builder.add_data(&receipt.seal.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;
        let serialized_claim: Vec<u8> = receipt.claim.clone().value()?.into();
        builder.add_data(&serialized_claim)?;
        builder.add_data(receipt.hashfn.as_bytes())?;
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
        builder.add_op(OpZkPrecompile)?;
        Ok(builder)
    }
}
//    build_zk_script(&[seal, claim, hashfn, control_index, control_digests, journal, image_id, vec![stark_tag]]).unwrap()
