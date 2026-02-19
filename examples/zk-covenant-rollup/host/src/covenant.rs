use kaspa_txscript::opcodes::codes::{
    OpAdd, OpBlake2b, OpCat, OpChainblockSeqCommit, OpCovOutCount, OpData32, OpDrop, OpDup, OpElse, OpEndIf, OpEqual, OpEqualVerify,
    OpFromAltStack, OpIf, OpInputCovenantId, OpNumEqual, OpNumEqualVerify, OpSHA256, OpSwap, OpToAltStack, OpTxInputIndex,
    OpTxInputScriptSigLen, OpTxInputScriptSigSubstr, OpTxOutputSpk, OpTxOutputSpkSubstr,
};
use kaspa_txscript::script_builder::ScriptBuilder;

/// Redeem script prefix size in bytes.
///
/// Layout (66 bytes total):
/// - 1 byte:  `OpData32`
/// - 32 bytes: `prev_seq_commitment`
/// - 1 byte:  `OpData32`
/// - 32 bytes: `prev_state_hash`
pub const REDEEM_PREFIX_LEN: i64 = 66;

/// Rollup covenant specific methods.
/// Note: hash_redeem_to_spk, verify_output_spk, and verify_input_index_zero
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

    /// Build journal preimage from alt stack values, then hash.
    ///
    /// Uses OpCovOutCount introspection to determine if a permission output exists:
    /// - If 2 covenant outputs: appends blake2b(output_1_spk) → 192-byte preimage.
    /// - If 1 covenant output: keeps 160-byte base preimage.
    ///
    /// Expects: [...],
    ///          alt:[prev_state_hash, prev_seq_commitment, new_app_state_hash, new_seq_commitment]
    /// Leaves:  [..., journal_hash]
    fn build_and_hash_journal(&mut self) -> Result<&mut Self, Self::Error>;

    /// Verify covenant output count and optionally append permission script hash.
    ///
    /// Uses OpCovOutCount introspection:
    /// - If count == 2: extracts the 32-byte script hash from output 1's P2SH SPK
    ///   (bytes 4..36 of `to_bytes()`), verifies P2SH format, appends hash to preimage.
    /// - If count == 1: preimage unchanged.
    /// - Any other count: script fails.
    ///
    /// Expects: [..., base_preimage]
    /// Leaves:  [..., base_preimage] or [..., extended_preimage]
    fn verify_outputs_and_append_perm_hash(&mut self) -> Result<&mut Self, Self::Error>;
}

impl RollupCovenant for ScriptBuilder {
    type Error = kaspa_txscript::script_builder::ScriptBuilderError;

    // ANCHOR: stash_prev_values
    fn stash_prev_values(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack: [..., block_prove_to, new_app_state_hash, prev_seq_commitment, prev_state_hash]
        self.add_op(OpToAltStack)?;
        // Stack: [..., block_prove_to, new_app_state_hash, prev_seq_commitment], alt:[prev_state_hash]
        self.add_op(OpToAltStack)
        // Stack: [..., block_prove_to, new_app_state_hash], alt:[prev_state_hash, prev_seq_commitment]
    }
    // ANCHOR_END: stash_prev_values

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
        // Skip past the REDEEM_PREFIX_LEN-byte prefix to get the suffix
        self.add_i64(-redeem_script_len + REDEEM_PREFIX_LEN)?;
        self.add_op(OpAdd)?;
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpTxInputScriptSigLen)?;
        self.add_op(OpTxInputScriptSigSubstr)?;
        self.add_op(OpCat)
    }

    // ANCHOR: build_and_hash_journal
    fn build_and_hash_journal(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack:     [...]
        // Alt stack (top→bottom): [new_seq, new_state, prev_seq, prev_state]
        //
        // Journal preimage (160 or 192 bytes):
        //   prev_state(32) || prev_seq(32) || new_state(32) || new_seq(32)
        //   || covenant_id(32)
        //   [optional: || permission_script_hash(32)]

        // --- Build 128 bytes from alt stack (prev_state||prev_seq||new_state||new_seq) ---
        self.add_op(OpFromAltStack)?; // [..., new_seq]
        self.add_op(OpFromAltStack)?; // [..., new_seq, new_state]
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?; // [..., (new_state||new_seq)]
        self.add_op(OpFromAltStack)?; // [..., (new_state||new_seq), prev_seq]
        self.add_op(OpFromAltStack)?; // [..., (new_state||new_seq), prev_seq, prev_state]
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?; // [..., (new_state||new_seq), (prev_state||prev_seq)]
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?; // [..., (prev||new)] (128B)

        // --- Append covenant_id → 160-byte base ---
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpInputCovenantId)?;
        self.add_op(OpCat)?; // [..., 160B base]

        // --- Verify outputs + optionally append perm hash → 160B or 192B, then SHA256 ---
        self.verify_outputs_and_append_perm_hash()?;
        self.add_op(OpSHA256) // [..., journal_hash]
    }
    // ANCHOR_END: build_and_hash_journal

    // ANCHOR: verify_outputs_append_perm
    fn verify_outputs_and_append_perm_hash(&mut self) -> Result<&mut Self, Self::Error> {
        // Stack: [..., base_preimage]
        // Read covenant output count via introspection.
        self.add_op(OpTxInputIndex)?;
        self.add_op(OpInputCovenantId)?;
        self.add_op(OpCovOutCount)?;
        // Stack: [..., base, count]
        self.add_op(OpDup)?;
        self.add_i64(2)?;
        self.add_op(OpNumEqual)?;
        // Stack: [..., base, count, is_two]
        self.add_op(OpIf)?;
        // count == 2: drop count, extract script hash from output 1 P2SH SPK
        self.add_op(OpDrop)?;
        // Stack: [..., base]
        // Extract script hash: bytes 4..36 of to_bytes() = version(2) + OpBlake2b(1) + OpData32(1) + hash(32) + OpEqual(1)
        self.add_i64(1)?;
        self.add_i64(4)?;
        self.add_i64(36)?;
        self.add_op(OpTxOutputSpkSubstr)?;
        // Stack: [..., base, script_hash(32B)]
        // Verify output 1 SPK is P2SH: reconstruct expected SPK from hash, compare with actual
        self.add_op(OpDup)?;
        // Stack: [..., base, hash, hash]
        self.add_data(&[0x00, 0x00, OpBlake2b, OpData32])?;
        self.add_op(OpSwap)?;
        self.add_op(OpCat)?;
        self.add_data(&[OpEqual])?;
        self.add_op(OpCat)?;
        // Stack: [..., base, hash, expected_spk(37B)]
        self.add_i64(1)?;
        self.add_op(OpTxOutputSpk)?;
        // Stack: [..., base, hash, expected_spk, actual_spk]
        self.add_op(OpEqualVerify)?;
        // Stack: [..., base, hash]
        self.add_op(OpCat)?;
        // Stack: [..., 192B]
        self.add_op(OpElse)?;
        // count != 2: verify count == 1
        self.add_i64(1)?;
        self.add_op(OpNumEqualVerify)?;
        // Stack: [..., base] (160B, unchanged)
        self.add_op(OpEndIf)
    }
    // ANCHOR_END: verify_outputs_append_perm
}
