mod vk;

use crate::result::Result;
use crate::zk_to_script::builder::proof::FinalizedR0Script;
pub use crate::zk_to_script::builder::proof::groth16::vk::R0_SERIALIZED_UNCOMPRESSED_VK;
use crate::zk_to_script::{BoundedR0Groth16FixedJournalScript, BoundedR0Groth16Script, ZkScriptBuilder, push_r0_groth16_witness};
use risc0_binfmt::Digestible;
use risc0_zkvm::Groth16Receipt;

impl ZkScriptBuilder<BoundedR0Groth16Script> {
    /// Add the proof to an existing groth16 commit script and return both the
    /// spending script and the inner redeem script.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(
        mut self,
        receipt: Groth16Receipt<Claim>,
        journal_hash: [u8; 32],
    ) -> Result<FinalizedR0Script> {
        let redeem_script = self.builder.drain();

        // Caller-owned journal hash goes on the stack first (what we claim to
        // be), then the proof witness lands on top of it.
        self.builder.add_data(&journal_hash)?;
        push_r0_groth16_witness(&mut self.builder, receipt)?;

        self.builder.add_data(&redeem_script)?; // push the redeem script

        Ok(FinalizedR0Script { sig_script: self.builder.drain(), redeem_script })
    }
}

impl ZkScriptBuilder<BoundedR0Groth16FixedJournalScript> {
    /// Add the proof to a fixed-journal groth16 commit script. The journal hash
    /// is already baked into the redeem script, so the spending script only
    /// carries the proof witness.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(mut self, receipt: Groth16Receipt<Claim>) -> Result<FinalizedR0Script> {
        let redeem_script = self.builder.drain();

        push_r0_groth16_witness(&mut self.builder, receipt)?;

        self.builder.add_data(&redeem_script)?; // push the redeem script

        Ok(FinalizedR0Script { sig_script: self.builder.drain(), redeem_script })
    }
}
