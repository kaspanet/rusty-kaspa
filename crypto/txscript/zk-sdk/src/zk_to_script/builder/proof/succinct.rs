use crate::result::Result;
use crate::zk_to_script::{
    BoundedR0SuccinctFixedJournalScript, BoundedR0SuccinctScript, R0ScriptBuilder, builder::proof::FinalizedR0Script,
    push_r0_succinct_witness,
};
use risc0_binfmt::Digestible;
use risc0_zkvm::{Digest, SuccinctReceipt};

impl R0ScriptBuilder<BoundedR0SuccinctScript> {
    /// Add the proof to an existing succinct commit script and return both the
    /// spending script and the inner redeem script.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(
        mut self,
        receipt: SuccinctReceipt<Claim>,
        journal: Digest,
    ) -> Result<FinalizedR0Script> {
        let redeem_script = self.builder.drain();

        // Push the receipt-derived witness items (push-only), then the
        // caller-owned journal on top — matching the stack shape the verifier
        // consumes.
        push_r0_succinct_witness(&mut self.builder, receipt)?;
        self.builder.add_data(journal.as_bytes())?;

        self.builder.add_data(&redeem_script)?; // push the redeem script

        Ok(FinalizedR0Script { sig_script: self.builder.drain(), redeem_script })
    }
}

impl R0ScriptBuilder<BoundedR0SuccinctFixedJournalScript> {
    /// Add the proof to a fixed-journal succinct commit script. The journal is
    /// already baked into the redeem script, so the spending script only carries
    /// the four receipt-derived witness items.
    pub fn finalize_with_proof<Claim: Digestible + Clone>(mut self, receipt: SuccinctReceipt<Claim>) -> Result<FinalizedR0Script> {
        let redeem_script = self.builder.drain();

        push_r0_succinct_witness(&mut self.builder, receipt)?;

        self.builder.add_data(&redeem_script)?; // push the redeem script

        Ok(FinalizedR0Script { sig_script: self.builder.drain(), redeem_script })
    }
}
