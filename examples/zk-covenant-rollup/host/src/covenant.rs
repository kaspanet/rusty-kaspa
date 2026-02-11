use kaspa_txscript::opcodes::codes::{
    OpAdd, OpCat, OpChainblockSeqCommit, OpData32, OpDup, OpFromAltStack, OpSHA256, OpSwap, OpToAltStack, OpTxInputIndex,
    OpTxInputScriptSigLen, OpTxInputScriptSigSubstr,
};
use kaspa_txscript::script_builder::ScriptBuilder;

/// Rollup covenant specific methods.
/// Note: hash_redeem_to_spk, verify_output_spk, verify_input_index_zero, and verify_covenant_single_output
/// are provided by the CovenantBase trait from zk_covenant_common.
pub trait RollupCovenant {
    type Error;

    /// Stash prefix-pushed prev values to alt stack.
    ///
    /// Expects: [..., block_prove_to, new_app_state_hash, prev_seq_commitment, prev_state_hash]
    /// Leaves:  [..., block_prove_to, new_app_state_hash], alt:[prev_state_hash, prev_seq_commitment]
    fn stash_prev_values(&mut self) -> Result<&mut Self, Self::Error>;

    /// Expects: [..., block_prove_to, new_app_state_hash]
    /// Leaves:  [..., new_app_state_hash, new_seq_commitment]
    fn obtain_new_seq_commitment(&mut self) -> Result<&mut Self, Self::Error>;

    /// Expects: [..., new_app_state_hash, new_seq_commitment], alt:[prev_state_hash, prev_seq_commitment]
    /// Leaves:  [..., 66-byte-prefix], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]
    fn build_next_redeem_prefix_rollup(&mut self) -> Result<&mut Self, Self::Error>;

    /// Expects: [..., prefix]
    /// Leaves:  [..., new_redeem_script]
    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error>;

    /// Build journal preimage from alt stack and hash.
    ///
    /// Expects: [...], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]
    /// Leaves:  [..., journal_hash]
    fn build_and_hash_journal(&mut self) -> Result<&mut Self, Self::Error>;
}

impl RollupCovenant for ScriptBuilder {
    type Error = kaspa_txscript::script_builder::ScriptBuilderError;

    fn stash_prev_values(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack: [..., block_prove_to, new_app_state_hash, prev_seq_commitment, prev_state_hash]
        self.add_op(OpToAltStack)?;
        // Stack: [..., block_prove_to, new_app_state_hash, prev_seq_commitment], alt:[prev_state_hash]
        self.add_op(OpToAltStack)
        // Stack: [..., block_prove_to, new_app_state_hash], alt:[prev_state_hash, prev_seq_commitment]
    }

    fn obtain_new_seq_commitment(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack: [..., block_prove_to, new_app_state_hash]
        self.add_op(OpSwap)?;
        // Stack: [..., new_app_state_hash, block_prove_to]
        self.add_op(OpChainblockSeqCommit)
        // Stack: [..., new_app_state_hash, new_seq_commitment]
    }

    fn build_next_redeem_prefix_rollup(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack: [..., new_app_state_hash, new_seq_commitment]
        // Build: OpData32 || new_seq_commitment || OpData32 || new_app_state_hash
        // Stash new values on alt stack (on top of prev values already there)

        self.add_op(OpSwap)?;
        // Stack: [..., new_seq_commitment, new_app_state_hash]
        self.add_op(OpDup)?;
        self.add_op(OpToAltStack)?;
        // Stack: [..., new_seq_commitment, new_app_state_hash], alt:[..., new_app_state_hash]
        self.add_data(&[OpData32])?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;
        // Stack: [..., new_seq_commitment, (OpData32||new_app_state_hash)]
        self.add_op(OpSwap)?;
        // Stack: [..., (OpData32||new_app_state_hash), new_seq_commitment]
        self.add_op(OpDup)?;
        self.add_op(OpToAltStack)?;
        // Stack: [..., (OpData32||new_app_state_hash), new_seq_commitment], alt:[..., new_app_state_hash, new_seq_commitment]
        self.add_data(&[OpData32])?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;
        // Stack: [..., (OpData32||new_app_state_hash), (OpData32||new_seq_commitment)]
        self.add_op(OpSwap)?;
        self.add_op(OpCat)
        // Stack: [..., (OpData32||new_seq_commitment||OpData32||new_app_state_hash)] = 66-byte prefix
    }

    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_i64(-redeem_script_len + 66)?;
        self.add_op(OpAdd)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_op(OpTxInputScriptSigSubstr)?;
        self.add_op(OpCat)
    }

    fn build_and_hash_journal(&mut self) -> Result<&mut Self, Self::Error> {
        // Alt stack (top→bottom): [new_seq_commitment, new_app_state_hash, prev_seq_commitment, prev_state_hash]
        // Need preimage: prev_state_hash || prev_seq_commitment || new_app_state_hash || new_seq_commitment

        self.add_op(OpFromAltStack)?;
        // Stack: [..., new_seq_commitment]
        self.add_op(OpFromAltStack)?;
        // Stack: [..., new_seq_commitment, new_app_state_hash]
        self.add_op(OpSwap)?;
        // Stack: [..., new_app_state_hash, new_seq_commitment]
        self.add_op(OpCat)?;
        // Stack: [..., (new_app_state_hash||new_seq_commitment)]

        self.add_op(OpFromAltStack)?;
        // Stack: [..., (new_app||new_seq), prev_seq_commitment]
        self.add_op(OpFromAltStack)?;
        // Stack: [..., (new_app||new_seq), prev_seq_commitment, prev_state_hash]
        self.add_op(OpSwap)?;
        // Stack: [..., (new_app||new_seq), prev_state_hash, prev_seq_commitment]
        self.add_op(OpCat)?;
        // Stack: [..., (new_app||new_seq), (prev_state_hash||prev_seq_commitment)]

        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;
        // Stack: [..., (prev_state_hash||prev_seq_commitment||new_app_state_hash||new_seq_commitment)]

        self.add_op(OpSHA256)
        // Stack: [..., journal_hash]
    }
}
