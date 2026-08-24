use std::marker::PhantomData;

use crate::result::Result;
use crate::zk_to_script::{
    BoundedR0SuccinctFixedJournalScript, BoundedR0SuccinctScript, UnboundedR0Script, ZkScriptBuilder, append_r0_succinct_verifier,
    append_r0_succinct_verifier_with_fixed_journal,
};
use kaspa_txscript::zk_precompiles::risc0::rcpt::HashFnId;

impl ZkScriptBuilder<UnboundedR0Script> {
    /// Commits to the succinct proof system; the locking script will expect a
    /// successful verification of a succinct proof from the specified image id,
    /// control id and hash function.
    pub fn commit_to_succinct(
        mut self,
        image_id: [u8; 32],
        control_id: [u8; 32],
        hash_fn_id: Option<HashFnId>,
    ) -> Result<ZkScriptBuilder<BoundedR0SuccinctScript>> {
        append_r0_succinct_verifier(&mut self.builder, image_id, control_id, hash_fn_id)?;
        Ok(ZkScriptBuilder { builder: self.builder, _state: PhantomData })
    }

    /// Commits to the succinct proof system *and* a fixed `journal` baked into
    /// the script
    pub fn commit_to_succinct_with_fixed_journal(
        mut self,
        image_id: [u8; 32],
        control_id: [u8; 32],
        hash_fn_id: Option<HashFnId>,
        journal: [u8; 32],
    ) -> Result<ZkScriptBuilder<BoundedR0SuccinctFixedJournalScript>> {
        append_r0_succinct_verifier_with_fixed_journal(&mut self.builder, image_id, control_id, hash_fn_id, journal)?;
        Ok(ZkScriptBuilder { builder: self.builder, _state: PhantomData })
    }
}
