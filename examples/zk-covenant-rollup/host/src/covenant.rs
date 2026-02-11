use kaspa_txscript::opcodes::codes::{
    Op0, OpAdd, OpCat, OpChainblockSeqCommit, OpData32, OpDrop, OpDup, OpFromAltStack, OpSHA256, OpSwap, OpToAltStack, OpTxInputIndex,
    OpTxInputScriptSigLen, OpTxInputScriptSigSubstr,
};
use kaspa_txscript::script_builder::ScriptBuilder;

/// Redeem script prefix size in bytes.
///
/// Layout (68 bytes total):
/// - 2 bytes: domain tag (`OP_0, OP_DROP` for state verification)
/// - 1 byte:  `OpData32`
/// - 32 bytes: `prev_seq_commitment`
/// - 1 byte:  `OpData32`
/// - 32 bytes: `prev_state_hash`
pub const REDEEM_PREFIX_LEN: i64 = 68;

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
    /// Leaves:  [..., REDEEM_PREFIX_LEN-byte prefix], alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]
    fn build_next_redeem_prefix_rollup(&mut self) -> Result<&mut Self, Self::Error>;

    /// Expects: [..., prefix]
    /// Leaves:  [..., new_redeem_script]
    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error>;

    /// Build journal preimage from alt stack and sig_script values, then hash.
    ///
    /// Expects: [..., exit_amount, exit_root, exit_unclaimed_count],
    ///          alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]
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
        // Build: OP_0 || OP_DROP || OpData32 || new_seq_commitment || OpData32 || new_app_state_hash
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
        self.add_op(OpCat)?;
        // Stack: [..., (OpData32||new_seq_commitment||OpData32||new_app_state_hash)] = 66-byte data

        // Prepend domain prefix: [OP_0(0x00), OP_DROP(0x75)]
        self.add_data(&[Op0, OpDrop])?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)
        // Stack: [..., (OP_0||OP_DROP||OpData32||new_seq_commitment||OpData32||new_app_state_hash)] = 68-byte prefix
    }

    fn extract_redeem_suffix_and_concat(&mut self, redeem_script_len: i64) -> Result<&mut Self, Self::Error> {
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        // Skip past the REDEEM_PREFIX_LEN-byte prefix to get the suffix
        self.add_i64(-redeem_script_len + REDEEM_PREFIX_LEN)?;
        self.add_op(OpAdd)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_op(OpTxInputScriptSigSubstr)?;
        self.add_op(OpCat)
    }

    fn build_and_hash_journal(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack:     [..., exit_amount, exit_root, exit_unclaimed_count]
        // Alt stack (top→bottom): [new_seq, new_state, prev_seq, prev_state]
        //
        // Target preimage (176 bytes):
        //   prev_state(32) || prev_seq(32) || new_state(32) || new_seq(32)
        //   || exit_amount(8) || exit_root(32) || exit_unclaimed_count(8)

        // --- Concat exit fields (already in correct order) ---
        self.add_op(OpCat)?; // [..., exit_amount, (exit_root||exit_unclaimed_count)]
        self.add_op(OpCat)?; // [..., exit_suffix] (48 bytes)

        // --- Build first 128 bytes from alt stack ---
        self.add_op(OpFromAltStack)?; // [..., exit_suffix, new_seq]
        self.add_op(OpFromAltStack)?; // [..., exit_suffix, new_seq, new_state]
        self.add_op(OpSwap)?;         // [..., exit_suffix, new_state, new_seq]
        self.add_op(OpCat)?;          // [..., exit_suffix, (new_state||new_seq)]
        self.add_op(OpFromAltStack)?; // [..., exit_suffix, (new_state||new_seq), prev_seq]
        self.add_op(OpFromAltStack)?; // [..., exit_suffix, (new_state||new_seq), prev_seq, prev_state]
        self.add_op(OpSwap)?;         // [..., exit_suffix, (new_state||new_seq), prev_state, prev_seq]
        self.add_op(OpCat)?;          // [..., exit_suffix, (new_state||new_seq), (prev_state||prev_seq)]
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;          // [..., exit_suffix, (prev||new)] (128 bytes)

        // --- Concat and hash ---
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;          // [..., 176-byte preimage]
        self.add_op(OpSHA256)         // [..., journal_hash]
    }
}
