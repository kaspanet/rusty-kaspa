use super::Result;
use crate::zk_precompiles::risc0::zk_to_script::{BoundedR0SuccinctScript, R0ScriptBuilder, builder::proof::FinalizedR0Script};
use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, SuccinctReceipt, sha};

impl R0ScriptBuilder<BoundedR0SuccinctScript> {
    /// Add the proof to an existing succinct commit script and return both
    /// the spending script and the inner redeem script.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(
        mut self,
        receipt: SuccinctReceipt<Claim>,
        journal: Digest,
    ) -> Result<FinalizedR0Script> {
        let redeem_script = self.builder.drain();

        // The claim here might be already or not digested
        // but in either case we need to extract the digest
        // since that is what allows us to have a constant sized
        // stark proof.
        let serialized_claim: Digest = receipt.claim.digest::<sha::Impl>();

        self.builder.add_data(serialized_claim.as_bytes())?;

        // Extract the control index and control digests
        // which are the merkle proof of inclusion.
        let (control_index, control_digests) = {
            (
                receipt.control_inclusion_proof.index.to_le_bytes(),
                receipt.control_inclusion_proof.digests.iter().flat_map(|d| d.as_bytes().to_vec()).collect::<Vec<u8>>(),
            )
        };
        self.builder.add_data(&control_index)?;
        self.builder.add_data(&control_digests)?;

        // Add the seal but encode it as vec<u8>
        self.builder.add_data(&receipt.seal.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())?;

        // Add the journal hash which contains the output of this
        // program
        self.builder.add_data(journal.as_bytes())?;

        self.builder.add_data(&redeem_script)?; // push the redeem script 

        Ok(FinalizedR0Script { sig_script: self.builder.drain(), redeem_script })
    }
}
