use kaspa_txscript::opcodes::codes::{
    OpAdd, OpCat, OpData32, OpDup, OpSHA256, OpSwap, OpTxInputIndex, OpTxInputScriptSigLen, OpTxInputScriptSigSubstr,
    OpTxPayloadSubstr,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use zk_covenant_inline_core::VersionedActionRaw;

/// Inline covenant specific methods.
/// Note: hash_redeem_to_spk, verify_output_spk, verify_input_index_zero, and verify_covenant_single_output
/// are provided by the CovenantBase trait from zk_covenant_common.
pub trait InlineCovenant {
    type Error;

    /// Pushes old_state_hash, swaps with new_state_hash, duplicates new_state_hash.
    ///
    /// Expects on stack: [..., new_state_hash]
    /// Leaves on stack:  [..., old_state_hash, new_state_hash, new_state_hash]
    fn push_old_state_and_dup_new(&mut self, old_state_hash: [u32; 8]) -> Result<&mut Self, Self::Error>;

    /// Builds prefix (OpData32 || new_state_hash) for the new redeem script.
    ///
    /// Expects on stack: [..., new_state_hash, new_state_hash]
    /// Leaves on stack:  [..., new_state_hash, (OpData32 || new_state_hash)]
    fn build_next_redeem_prefix(&mut self) -> Result<&mut Self, Self::Error>;

    /// Extracts redeem script suffix from sig_script and concatenates with prefix.
    ///
    /// Expects on stack: [..., (OpData32 || new_state_hash)]
    /// Leaves on stack:  [..., new_redeem_script]
    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error>;

    /// Constructs preimage = versioned_action_raw || old_state_hash || new_state_hash.
    ///
    /// Expects on stack: [..., old_state_hash, new_state_hash]
    /// Leaves on stack:  [..., preimage]
    fn build_journal_preimage(&mut self) -> Result<&mut Self, Self::Error>;

    /// Hashes preimage with SHA256 to get journal_hash.
    ///
    /// Expects on stack: [..., preimage]
    /// Leaves on stack:  [..., journal_hash]
    fn hash_journal(&mut self) -> Result<&mut Self, Self::Error>;
}

impl InlineCovenant for ScriptBuilder {
    type Error = kaspa_txscript::script_builder::ScriptBuilderError;

    fn push_old_state_and_dup_new(&mut self, old_state_hash: [u32; 8]) -> Result<&mut Self, Self::Error> {
        self.add_data(bytemuck::bytes_of(&old_state_hash))?;
        self.add_op(OpSwap)?;
        self.add_op(OpDup)
    }

    fn build_next_redeem_prefix(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_data(&[OpData32])?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)
    }

    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_i64(-redeem_script_len + 33)?;
        self.add_op(OpAdd)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_op(OpTxInputScriptSigSubstr)?;
        self.add_op(OpCat)
    }

    fn build_journal_preimage(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpCat)?;
        self.add_i64(0)?;
        self.add_i64(size_of::<VersionedActionRaw>() as i64)?;
        self.add_op(OpTxPayloadSubstr)?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)
    }

    fn hash_journal(&mut self) -> Result<&mut Self, Self::Error> {
        self.add_op(OpSHA256)
    }
}
