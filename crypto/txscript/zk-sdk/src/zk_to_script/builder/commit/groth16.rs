use std::marker::PhantomData;

use crate::result::Result;
use crate::zk_to_script::{
    BoundedR0Groth16FixedJournalScript, BoundedR0Groth16Script, UnboundedR0Script, ZkScriptBuilder, append_r0_groth16_verifier,
    append_r0_groth16_verifier_with_fixed_journal,
};

impl ZkScriptBuilder<UnboundedR0Script> {
    /// Commit the script to unlocking only from a valid groth16 proof from a
    /// specified image id as public input.
    pub fn commit_to_groth16(mut self, image_id: [u8; 32]) -> Result<ZkScriptBuilder<BoundedR0Groth16Script>> {
        append_r0_groth16_verifier(&mut self.builder, image_id)?;
        Ok(ZkScriptBuilder { builder: self.builder, _state: PhantomData })
    }

    /// Commit the script to unlocking only from a valid groth16 proof for the
    /// given image id *and* a fixed `journal_hash` baked into the script
    pub fn commit_to_groth16_with_fixed_journal(
        mut self,
        image_id: [u8; 32],
        journal_hash: [u8; 32],
    ) -> Result<ZkScriptBuilder<BoundedR0Groth16FixedJournalScript>> {
        append_r0_groth16_verifier_with_fixed_journal(&mut self.builder, image_id, journal_hash)?;
        Ok(ZkScriptBuilder { builder: self.builder, _state: PhantomData })
    }
}
