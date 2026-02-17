use std::num::NonZeroUsize;

use kaspa_consensus_core::constants::TX_VERSION;
use kaspa_txscript::opcodes::codes::{
    Op0, OpAdd, OpBlake2b, OpCat, OpCovOutCount, OpData32, OpDrop, OpDup, OpElse, OpEndIf, OpEqual, OpEqualVerify, OpFalse,
    OpFromAltStack, OpGreaterThan, OpGreaterThanOrEqual, OpIf, OpInputCovenantId, OpNip, OpNum2Bin, OpOver, OpRoll, OpRot, OpSHA256,
    OpSub, OpSwap, OpToAltStack, OpTrue, OpTxInputAmount, OpTxInputCount, OpTxInputIndex, OpTxInputScriptSigLen,
    OpTxInputScriptSigSubstr, OpTxInputSpk, OpTxOutputAmount, OpTxOutputCount, OpTxOutputSpk, OpVerify,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use zk_covenant_rollup_core::permission_tree::PermProof;

#[cfg(test)]
mod tests;

// ─────────────────────────────────────────────────────────────────
//  Script domains
// ─────────────────────────────────────────────────────────────────

// ANCHOR: script_domain
/// Script domain suffixes for distinguishing different scripts within the same covenant.
///
/// Each domain has a 2-byte suffix (`[opcode, OP_DROP]`) appended to the redeem script
/// after the final `OP_TRUE`. This is a no-op (pushes a value then drops it) so the
/// stack result is unchanged, but it makes the last 2 bytes of the sig_script
/// (= last 2 bytes of the redeem) identifiable by cross-script introspection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptDomain {
    /// State verification (rollup): `[OP_0(0x00), OP_DROP(0x75)]`
    StateVerification,
    /// Permission: `[OP_1(0x51), OP_DROP(0x75)]`
    Permission,
}

impl ScriptDomain {
    /// Get the raw suffix bytes for this domain.
    pub fn suffix_bytes(&self) -> [u8; 2] {
        match self {
            ScriptDomain::StateVerification => [Op0, OpDrop],
            ScriptDomain::Permission => [OpTrue, OpDrop], // OP_1 == OpTrue == 0x51
        }
    }
}
// ANCHOR_END: script_domain

// ─────────────────────────────────────────────────────────────────
//  Constants
// ─────────────────────────────────────────────────────────────────

/// Permission redeem script prefix size in bytes.
///
/// Layout (42 bytes total):
/// - 1 byte:   `OpData32`
/// - 32 bytes:  root hash
/// - 1 byte:   `OP_DATA_8` (push-8-bytes opcode = 0x08)
/// - 8 bytes:   unclaimed_count (LE)
pub const PERM_REDEEM_PREFIX_LEN: i64 = 42;

/// Delegate entry script bytes **before** the 32-byte covenant_id (bytes 0..7).
///
/// ```text
/// OpTxInputIndex(0xb9) Op0(0x00) OpGreaterThan(0xa0) OpVerify(0x69)
/// Op0(0x00) OpInputCovenantId(0xcf) OpData32(0x20)
/// ```
const DELEGATE_SCRIPT_PREFIX: [u8; 7] = [0xb9, 0x00, 0xa0, 0x69, 0x00, 0xcf, 0x20];

/// Delegate entry script bytes **after** the 32-byte covenant_id (bytes 39..53).
///
/// ```text
/// OpEqualVerify(0x88) Op0(0x00) Op0(0x00) OpTxInputScriptSigLen(0xc9)
/// OpDup(0x76) Op2(0x52) OpSub(0x94) OpSwap(0x7c) OpTxInputScriptSigSubstr(0xbc)
/// push-2(0x02) 0x51 0x75 OpEqualVerify(0x88) OpTrue(0x51)
/// ```
const DELEGATE_SCRIPT_SUFFIX: [u8; 14] = [0x88, 0x00, 0x00, 0xc9, 0x76, 0x52, 0x94, 0x7c, 0xbc, 0x02, 0x51, 0x75, 0x88, 0x51];

/// Maximum number of transaction outputs allowed by the permission script.
///
/// Layout: withdrawal(1) + continuation(0-1) + delegate change(0-1) + collateral change(0-1).
const MAX_OUTPUTS: i64 = 4;

// ═══════════════════════════════════════════════════════════════════
//  Permission redeem — trait + implementation
// ═══════════════════════════════════════════════════════════════════

/// Emitter for the phases of a permission redeem script.
///
/// The permission script governs L2->L1 withdrawals by verifying merkle-tree
/// claims and enforcing delegate input balance.  Each phase emits opcodes
/// into a [`ScriptBuilder`] and uses `OpVerify`/`OpEqualVerify` for its own
/// checks — no phase passes a `TRUE` sentinel to the next.
///
/// ## Transaction layout
///
/// **Inputs:**
///
/// | Index   | Role                                   |
/// |---------|----------------------------------------|
/// | 0       | Permission script (this input)         |
/// | 1..N    | Delegate inputs (bridge reserves)      |
/// | N+1     | Optional collateral for fees           |
///
/// **Outputs (max 4, fixed order):**
///
/// | Index   | Presence             | Role                           |
/// |---------|----------------------|--------------------------------|
/// | 0       | Always               | Withdrawal to leaf's SPK       |
/// | 1       | If unclaimed > 0     | Permission continuation        |
/// | 2       | If delegate change   | Delegate change output         |
/// | 3       | Optional             | Collateral change (unchecked)  |
///
/// ## Sig_script push order (bottom of stack first)
///
/// ```text
/// G2_sib_{d-1}, G2_dir_{d-1}, ..., G2_sib_0, G2_dir_0,
/// G1_sib_{d-1}, G1_dir_{d-1}, ..., G1_sib_0, G1_dir_0,
/// spk(var), amount(8B LE), deduct(i64),
/// redeem_script
/// ```
pub trait PermissionRedeemEmitter {
    /// Emit the 42-byte prefix: `[OpData32, root(32B), 0x08, unclaimed_count(8B)]`.
    fn emit_prefix(&self, b: &mut ScriptBuilder, root: &[u32; 8], unclaimed_count: u64);

    /// Stash the two embedded constants (root, unclaimed_count) to the alt stack.
    ///
    /// ```text
    /// Main:  [..G2, ..G1, spk, amount, deduct]
    /// Alt:   [] -> [uncl_emb, root_emb]
    /// ```
    fn emit_stash_embedded(&self, b: &mut ScriptBuilder);

    /// Validate `deduct > 0` and `amount >= deduct`.
    ///
    /// Stashes a copy of `deduct` to the alt stack for later use by P7.
    /// Computes `new_amount = amount - deduct` and leaves it on the stack.
    ///
    /// ```text
    /// Main: [..G2, ..G1, spk, amount, deduct]
    ///    -> [..G2, ..G1, spk, amount, new_amount]
    /// Alt:  [uncl, root] -> [uncl, root, deduct]
    /// ```
    fn emit_validate_amounts(&self, b: &mut ScriptBuilder);

    /// Verify output 0's SPK matches the leaf's `spk` (withdrawal goes to rightful owner).
    ///
    /// Reads `spk` from the stack (depth 2), prepends the 2-byte SPK version prefix,
    /// and compares against `OpTxOutputSpk(0)`.  Restores the stack afterward.
    fn emit_verify_withdrawal(&self, b: &mut ScriptBuilder);

    /// Compute old and new leaf hashes from `(spk, amount, new_amount)`.
    ///
    /// Produces `old_leaf = SHA256("PermLeaf" || spk || amount)` and
    /// `new_leaf` (either the same form with `new_amount`, or the empty-leaf
    /// hash when `new_amount == 0`).
    ///
    /// ```text
    /// Stack: [..G2, ..G1, spk, amount, new_amount]
    ///     -> [..G2, ..G1, old_leaf(32B)]
    /// Alt:   [..., root, deduct] -> [..., root, deduct, is_zero, new_leaf]
    /// ```
    fn emit_compute_leaf_hashes(&self, b: &mut ScriptBuilder);

    /// Merkle walk over G1 siblings to verify old root matches embedded root.
    ///
    /// Consumes `depth` `(sib, dir)` pairs from the stack.
    /// Fails (via `OpEqualVerify`) if the computed root != embedded root.
    fn emit_verify_old_root(&self, b: &mut ScriptBuilder);

    /// Merkle walk over G2 siblings to compute the new root.
    ///
    /// Consumes `depth` `(sib, dir)` pairs from the stack.
    fn emit_compute_new_root(&self, b: &mut ScriptBuilder);

    /// Compute `new_unclaimed`: decrements by 1 when the leaf is fully consumed.
    ///
    /// Converts the result to 8-byte LE via `OpNum2Bin` for prefix reconstruction.
    fn emit_compute_new_unclaimed(&self, b: &mut ScriptBuilder);

    /// Verify transaction outputs: output count cap, and continuation SPK when needed.
    ///
    /// * Enforces `output_count <= 4`.
    /// * If `new_unclaimed > 0`: verifies output 1 SPK matches P2SH(new_redeem)
    ///   and exactly one covenant continuation output exists.
    /// * If `new_unclaimed == 0`: verifies zero covenant outputs remain.
    ///
    /// Leaves `[deduct]` on the stack (no TRUE sentinel).
    fn emit_verify_outputs(&self, b: &mut ScriptBuilder);

    /// Verify the delegate input/output balance equation (index-based).
    ///
    /// Enforces:
    /// ```text
    /// input_count <= N + 2
    /// expected_change = sum(delegate_input_amounts) - deduct >= 0
    /// if expected_change > 0:
    ///     output[1 + CovOutCount] has delegate SPK and amount == expected_change
    /// ```
    ///
    /// The delegate change output position is deterministic:
    /// `delegate_idx = 1 + CovOutCount` (after withdrawal and optional continuation).
    ///
    /// where N = `max_delegate_inputs`.
    ///
    /// Leaves an empty stack.
    fn emit_verify_delegate_balance(&self, b: &mut ScriptBuilder);

    /// Emit the 2-byte domain suffix `[OP_1(0x51), OP_DROP(0x75)]` — a no-op
    /// that tags this script for cross-script introspection by the delegate.
    fn emit_domain_suffix(&self, b: &mut ScriptBuilder);

    /// Build the complete permission redeem script by calling all phases in order.
    fn build(&self, root: &[u32; 8], unclaimed_count: u64) -> Vec<u8> {
        let mut b = ScriptBuilder::new();
        self.emit_prefix(&mut b, root, unclaimed_count);
        self.emit_stash_embedded(&mut b);
        self.emit_validate_amounts(&mut b);
        self.emit_verify_withdrawal(&mut b);
        self.emit_compute_leaf_hashes(&mut b);
        self.emit_verify_old_root(&mut b);
        self.emit_compute_new_root(&mut b);
        self.emit_compute_new_unclaimed(&mut b);
        self.emit_verify_outputs(&mut b);
        self.emit_verify_delegate_balance(&mut b);
        b.add_op(OpTrue).unwrap(); // single final TRUE for P2SH
        self.emit_domain_suffix(&mut b);
        b.drain()
    }
}

/// Standard implementation of the permission redeem script.
pub struct PermissionRedeem {
    pub depth: usize,
    pub redeem_script_len: i64,
    pub max_delegate_inputs: NonZeroUsize,
}

impl PermissionRedeemEmitter for PermissionRedeem {
    fn emit_prefix(&self, b: &mut ScriptBuilder, root: &[u32; 8], unclaimed_count: u64) {
        b.add_data(bytemuck::bytes_of(root)).unwrap(); // OpData32 + root(32)
        b.add_data(&unclaimed_count.to_le_bytes()).unwrap(); // OP_DATA_8 + unclaimed(8)
    }

    fn emit_stash_embedded(&self, b: &mut ScriptBuilder) {
        // After prefix the top two items are uncl_emb (top) and root_emb.
        b.add_op(OpToAltStack).unwrap(); // stash uncl_emb
        b.add_op(OpToAltStack).unwrap(); // stash root_emb
                                         // Main: [..G2, ..G1, spk, amount, deduct]
                                         // Alt:  [uncl_emb, root_emb]  (root on top)
    }

    // ANCHOR: emit_validate_amounts
    fn emit_validate_amounts(&self, b: &mut ScriptBuilder) {
        // Main: [..G2, ..G1, spk, amount, deduct]
        // Alt: [uncl, root]

        // ── verify deduct > 0 ──
        b.add_op(OpDup).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpGreaterThan).unwrap();
        b.add_op(OpVerify).unwrap();

        // ── stash deduct to alt for later use by P7 ──
        b.add_op(OpDup).unwrap();
        b.add_op(OpToAltStack).unwrap();
        // Alt: [uncl, root, deduct]

        // ── compute new_amount = amount - deduct; verify >= 0 ──
        b.add_op(OpOver).unwrap(); // [..., amount, deduct, amount]
        b.add_op(OpSwap).unwrap(); // [..., amount, amount, deduct]
        b.add_op(OpSub).unwrap(); // [..., amount, new_amount]
        b.add_op(OpDup).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpGreaterThanOrEqual).unwrap();
        b.add_op(OpVerify).unwrap();
        // Main: [..G2, ..G1, spk, amount, new_amount]
    }
    // ANCHOR_END: emit_validate_amounts

    // ANCHOR: emit_verify_withdrawal
    fn emit_verify_withdrawal(&self, b: &mut ScriptBuilder) {
        // Main: [..., spk, amount, new_amount]

        // ── bring spk to top ──
        b.add_i64(2).unwrap();
        b.add_op(OpRoll).unwrap();
        // Main: [..., amount, new_amount, spk]

        b.add_op(OpDup).unwrap();
        // Main: [..., amount, new_amount, spk, spk_copy]

        // ── prepend SPK version prefix (0x0000 for version 0, big-endian u16) ──
        b.add_data(&0u16.to_be_bytes()).unwrap();
        b.add_op(OpSwap).unwrap();
        b.add_op(OpCat).unwrap();
        // Main: [..., amount, new_amount, spk, version||spk_copy]

        // ── compare with output 0's SPK ──
        b.add_i64(0).unwrap();
        b.add_op(OpTxOutputSpk).unwrap();
        b.add_op(OpEqualVerify).unwrap();
        // Main: [..., amount, new_amount, spk]

        // ── restore stack: spk back to depth 2 ──
        b.add_op(OpRot).unwrap(); // [new_amount, spk, amount]
        b.add_op(OpRot).unwrap(); // [spk, amount, new_amount]
                                  // Main: [..G2, ..G1, spk, amount, new_amount]
    }
    // ANCHOR_END: emit_verify_withdrawal

    // ANCHOR: emit_compute_leaf_hashes
    fn emit_compute_leaf_hashes(&self, b: &mut ScriptBuilder) {
        // ── is_zero = (new_amount == 0), stash for later ──
        b.add_op(OpDup).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpEqual).unwrap();
        b.add_op(OpToAltStack).unwrap();
        // Alt: [uncl, root, deduct, is_zero]

        // ── new_amount -> 8-byte LE for hashing ──
        b.add_i64(8).unwrap();
        b.add_op(OpNum2Bin).unwrap();

        // ── dup spk for reuse in old leaf hash ──
        b.add_op(OpRot).unwrap(); // [..G2, ..G1, amount, new_amt_8b, spk]
        b.add_op(OpDup).unwrap();
        b.add_op(OpToAltStack).unwrap();
        // Alt: [uncl, root, deduct, is_zero, spk_dup]

        // ── new leaf (nonzero): SHA256("PermLeaf" || spk || new_amt_8b) ──
        b.add_op(OpSwap).unwrap(); // [..G2, ..G1, amount, spk, new_amt_8b]
        b.add_op(OpCat).unwrap(); // [..G2, ..G1, amount, spk||new_amt_8b]
        b.add_data(b"PermLeaf").unwrap();
        b.add_op(OpSwap).unwrap();
        b.add_op(OpCat).unwrap();
        b.add_op(OpSHA256).unwrap();

        // ── empty leaf hash (constant) ──
        b.add_data(b"PermEmpty").unwrap();
        b.add_op(OpSHA256).unwrap();

        // ── select new_leaf based on is_zero; re-stash is_zero and spk_dup ──
        b.add_op(OpFromAltStack).unwrap(); // spk_dup
        b.add_op(OpFromAltStack).unwrap(); // is_zero
        b.add_op(OpDup).unwrap();
        b.add_op(OpToAltStack).unwrap(); // re-stash is_zero
        b.add_op(OpSwap).unwrap();
        b.add_op(OpToAltStack).unwrap(); // re-stash spk_dup
                                         // Alt: [uncl, root, deduct, is_zero, spk_dup]

        b.add_op(OpIf).unwrap();
        b.add_op(OpNip).unwrap(); // is_zero true: keep empty_h, drop new_leaf_nz
        b.add_op(OpElse).unwrap();
        b.add_op(OpDrop).unwrap(); // is_zero false: keep new_leaf_nz, drop empty_h
        b.add_op(OpEndIf).unwrap();

        b.add_op(OpToAltStack).unwrap();
        // Alt: [uncl, root, deduct, is_zero, spk_dup, new_leaf]

        // ── old leaf hash: SHA256("PermLeaf" || spk || amount) ──
        b.add_op(OpFromAltStack).unwrap(); // new_leaf
        b.add_op(OpFromAltStack).unwrap(); // spk_dup
                                           // Alt: [uncl, root, deduct, is_zero]
        b.add_op(OpRot).unwrap(); // [..G2, ..G1, new_leaf, spk_dup, amount]
        b.add_op(OpCat).unwrap(); // [..G2, ..G1, new_leaf, spk_dup||amount]
        b.add_data(b"PermLeaf").unwrap();
        b.add_op(OpSwap).unwrap();
        b.add_op(OpCat).unwrap();
        b.add_op(OpSHA256).unwrap();

        // Stash new_leaf for compute_new_root
        b.add_op(OpSwap).unwrap();
        b.add_op(OpToAltStack).unwrap();
        // Main: [..G2, ..G1, old_leaf]
        // Alt: [uncl, root, deduct, is_zero, new_leaf]
    }
    // ANCHOR_END: emit_compute_leaf_hashes

    // ANCHOR: emit_verify_old_root
    fn emit_verify_old_root(&self, b: &mut ScriptBuilder) {
        for _ in 0..self.depth {
            emit_merkle_step(b);
        }
        // Main: [..G2, computed_root], Alt: [uncl, root, deduct, is_zero, new_leaf]

        b.add_op(OpFromAltStack).unwrap(); // new_leaf
        b.add_op(OpFromAltStack).unwrap(); // is_zero
        b.add_op(OpFromAltStack).unwrap(); // deduct
        b.add_op(OpFromAltStack).unwrap(); // root_emb
                                           // Alt: [uncl]
                                           // Main: [..G2, computed_root, new_leaf, is_zero, deduct, root_emb]

        // Bring computed_old_root to top (position 4 from top)
        b.add_i64(4).unwrap();
        b.add_op(OpRoll).unwrap();
        b.add_op(OpEqualVerify).unwrap();
        // Main: [..G2, new_leaf, is_zero, deduct]

        // Re-stash deduct (deep) and is_zero, leave new_leaf on main
        b.add_op(OpRot).unwrap(); // [..G2, is_zero, deduct, new_leaf]
        b.add_op(OpSwap).unwrap(); // [..G2, is_zero, new_leaf, deduct]
        b.add_op(OpToAltStack).unwrap(); // stash deduct. Alt: [uncl, deduct]
        b.add_op(OpSwap).unwrap(); // [..G2, new_leaf, is_zero]
        b.add_op(OpToAltStack).unwrap(); // stash is_zero. Alt: [uncl, deduct, is_zero]
                                         // Main: [..G2, new_leaf]
    }
    // ANCHOR_END: emit_verify_old_root

    fn emit_compute_new_root(&self, b: &mut ScriptBuilder) {
        for _ in 0..self.depth {
            emit_merkle_step(b);
        }
        // Main: [new_root(32B)], Alt: [uncl, deduct, is_zero]
    }

    fn emit_compute_new_unclaimed(&self, b: &mut ScriptBuilder) {
        b.add_op(OpFromAltStack).unwrap(); // is_zero
        b.add_op(OpFromAltStack).unwrap(); // deduct
        b.add_op(OpFromAltStack).unwrap(); // uncl_emb
                                           // Alt: [], Main: [new_root, is_zero, deduct, uncl_emb]

        // Bring is_zero to top for conditional
        b.add_op(OpRot).unwrap(); // [new_root, deduct, uncl_emb, is_zero]

        // new_uncl = if is_zero { uncl - 1 } else { uncl }
        b.add_op(OpIf).unwrap();
        b.add_i64(1).unwrap();
        b.add_op(OpSub).unwrap();
        b.add_op(OpElse).unwrap();
        // uncl unchanged
        b.add_op(OpEndIf).unwrap();
        // Main: [new_root, deduct, new_uncl]

        // Convert to 8-byte LE for prefix reconstruction
        b.add_i64(8).unwrap();
        b.add_op(OpNum2Bin).unwrap();
        // Main: [new_root, deduct, new_uncl_8b]

        // Rearrange to [deduct, new_root, new_uncl_8b]
        b.add_op(OpRot).unwrap(); // [deduct, new_uncl_8b, new_root]
        b.add_op(OpSwap).unwrap(); // [deduct, new_root, new_uncl_8b]
    }

    // ANCHOR: emit_verify_outputs
    fn emit_verify_outputs(&self, b: &mut ScriptBuilder) {
        // ── enforce output_count <= MAX_OUTPUTS ──
        b.add_op(OpTxOutputCount).unwrap();
        b.add_i64(MAX_OUTPUTS).unwrap();
        b.add_op(OpGreaterThan).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpEqualVerify).unwrap();

        // ── check if all exits claimed (new_uncl == 0) ──
        b.add_op(OpDup).unwrap();
        b.add_data(&0u64.to_le_bytes()).unwrap();
        b.add_op(OpEqual).unwrap();
        // Main: [deduct, new_root, new_uncl_8b, is_done]

        b.add_op(OpIf).unwrap();
        {
            // All exits claimed — no continuation output needed.
            b.add_op(OpDrop).unwrap(); // drop new_uncl_8b
            b.add_op(OpDrop).unwrap(); // drop new_root

            // Verify no covenant continuation outputs remain.
            b.add_op(OpTxInputIndex).unwrap();
            b.add_op(OpInputCovenantId).unwrap();
            b.add_op(OpCovOutCount).unwrap();
            b.add_i64(0).unwrap();
            b.add_op(OpEqualVerify).unwrap();
            // Main: [deduct]
        }
        b.add_op(OpElse).unwrap();
        {
            // Unclaimed exits remain — verify continuation at output 1.
            // Main: [deduct, new_root, new_uncl_8b]

            // Build unclaimed part: [0x08 || new_uncl_8b]
            b.add_data(&[0x08u8]).unwrap();
            b.add_op(OpSwap).unwrap();
            b.add_op(OpCat).unwrap();

            // Build root part: [OpData32 || new_root]
            b.add_op(OpSwap).unwrap();
            b.add_data(&[OpData32]).unwrap();
            b.add_op(OpSwap).unwrap();
            b.add_op(OpCat).unwrap();

            // Concat -> new prefix (42B)
            b.add_op(OpSwap).unwrap();
            b.add_op(OpCat).unwrap();

            // Extract body+suffix from own sig_script
            b.add_op(OpTxInputIndex).unwrap();
            b.add_op(OpTxInputIndex).unwrap();
            b.add_op(OpTxInputScriptSigLen).unwrap();
            b.add_i64(-self.redeem_script_len + PERM_REDEEM_PREFIX_LEN).unwrap();
            b.add_op(OpAdd).unwrap();
            b.add_op(OpTxInputIndex).unwrap();
            b.add_op(OpTxInputScriptSigLen).unwrap();
            b.add_op(OpTxInputScriptSigSubstr).unwrap();
            b.add_op(OpCat).unwrap();

            // Hash -> expected SPK
            emit_hash_redeem_to_spk(b);

            // Verify output 1 SPK matches (output 0 is the withdrawal)
            b.add_i64(1).unwrap();
            b.add_op(OpTxOutputSpk).unwrap();
            b.add_op(OpEqualVerify).unwrap();

            // Verify exactly one covenant continuation output
            b.add_op(OpTxInputIndex).unwrap();
            b.add_op(OpInputCovenantId).unwrap();
            b.add_op(OpCovOutCount).unwrap();
            b.add_i64(1).unwrap();
            b.add_op(OpEqualVerify).unwrap();

            // Main: [deduct]
        }
        b.add_op(OpEndIf).unwrap();
    }
    // ANCHOR_END: emit_verify_outputs

    // ANCHOR: emit_verify_delegate_balance
    fn emit_verify_delegate_balance(&self, b: &mut ScriptBuilder) {
        let n = self.max_delegate_inputs.get();

        // Main: [deduct]
        // deduct is already minimal i64 (pushed via add_i64 in sig_script).

        // ── enforce input_count <= N+2 ──
        b.add_op(OpTxInputCount).unwrap();
        b.add_i64((n + 2) as i64).unwrap();
        b.add_op(OpGreaterThan).unwrap(); // input_count > N+2?
        b.add_i64(0).unwrap();
        b.add_op(OpEqualVerify).unwrap(); // must be false

        // ── build expected delegate P2SH SPK (37B) from covenant_id ──
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpInputCovenantId).unwrap();
        b.add_data(&DELEGATE_SCRIPT_PREFIX).unwrap();
        b.add_op(OpSwap).unwrap();
        b.add_op(OpCat).unwrap();
        b.add_data(&DELEGATE_SCRIPT_SUFFIX).unwrap();
        b.add_op(OpCat).unwrap();
        emit_hash_redeem_to_spk(b);
        b.add_op(OpToAltStack).unwrap();
        // Main: [deduct], Alt: [expected_spk]

        // ── sum delegate input amounts (unrolled i = 1..N) ──
        b.add_i64(0).unwrap(); // accum = 0
        for i in 1..=n {
            b.add_op(OpTxInputCount).unwrap();
            b.add_i64((i + 1) as i64).unwrap();
            b.add_op(OpGreaterThanOrEqual).unwrap();
            b.add_op(OpIf).unwrap();
            {
                b.add_i64(i as i64).unwrap();
                b.add_op(OpTxInputSpk).unwrap();
                b.add_op(OpFromAltStack).unwrap();
                b.add_op(OpDup).unwrap();
                b.add_op(OpToAltStack).unwrap();
                b.add_op(OpEqual).unwrap();
                b.add_op(OpIf).unwrap();
                {
                    b.add_i64(i as i64).unwrap();
                    b.add_op(OpTxInputAmount).unwrap();
                    b.add_op(OpAdd).unwrap();
                }
                b.add_op(OpEndIf).unwrap();
            }
            b.add_op(OpEndIf).unwrap();
        }
        // Main: [deduct, total_input], Alt: [expected_spk]

        // ── guard: input N+1 must NOT have delegate SPK ──
        b.add_op(OpTxInputCount).unwrap();
        b.add_i64((n + 2) as i64).unwrap();
        b.add_op(OpGreaterThanOrEqual).unwrap();
        b.add_op(OpIf).unwrap();
        {
            b.add_i64((n + 1) as i64).unwrap();
            b.add_op(OpTxInputSpk).unwrap();
            b.add_op(OpFromAltStack).unwrap();
            b.add_op(OpDup).unwrap();
            b.add_op(OpToAltStack).unwrap();
            b.add_op(OpEqual).unwrap();
            b.add_op(OpFalse).unwrap();
            b.add_op(OpEqualVerify).unwrap();
        }
        b.add_op(OpEndIf).unwrap();

        // ── compute expected_change = total_input - deduct; verify >= 0 ──
        b.add_op(OpSwap).unwrap(); // [total_input, deduct]
        b.add_op(OpSub).unwrap(); // total_input - deduct
                                  // Main: [expected_change], Alt: [expected_spk]
        b.add_op(OpDup).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpGreaterThanOrEqual).unwrap();
        b.add_op(OpVerify).unwrap();

        // ── delegate change index = 1 + CovOutCount (after withdrawal + optional continuation) ──
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpInputCovenantId).unwrap();
        b.add_op(OpCovOutCount).unwrap();
        b.add_i64(1).unwrap();
        b.add_op(OpAdd).unwrap();
        // Main: [expected_change, delegate_idx], Alt: [expected_spk]

        // ── verify delegate change output at expected index if needed ──
        b.add_op(OpOver).unwrap(); // [expected_change, delegate_idx, expected_change]
        b.add_i64(0).unwrap();
        b.add_op(OpGreaterThan).unwrap(); // [expected_change, delegate_idx, change_needed]
        b.add_op(OpIf).unwrap();
        {
            // expected_change > 0: verify output[delegate_idx] has delegate SPK and correct amount
            b.add_op(OpDup).unwrap(); // [expected_change, delegate_idx, delegate_idx]
            b.add_op(OpTxOutputSpk).unwrap(); // [expected_change, delegate_idx, output_spk]
            b.add_op(OpFromAltStack).unwrap(); // [..., output_spk, expected_spk]
            b.add_op(OpEqualVerify).unwrap(); // [expected_change, delegate_idx]
            b.add_op(OpTxOutputAmount).unwrap(); // [expected_change, output_amount]
            b.add_op(OpEqualVerify).unwrap(); // []
        }
        b.add_op(OpElse).unwrap();
        {
            // expected_change == 0: no delegate change output needed, clean up
            b.add_op(OpDrop).unwrap(); // drop delegate_idx
            b.add_op(OpDrop).unwrap(); // drop expected_change
            b.add_op(OpFromAltStack).unwrap();
            b.add_op(OpDrop).unwrap(); // drop expected_spk
        }
        b.add_op(OpEndIf).unwrap();
        // Main: [] (empty — final TRUE is added by build())
    }
    // ANCHOR_END: emit_verify_delegate_balance

    fn emit_domain_suffix(&self, b: &mut ScriptBuilder) {
        b.add_op(OpTrue).unwrap(); // 0x51
        b.add_op(OpDrop).unwrap(); // 0x75
    }
}

// ─────────────────────────────────────────────────────────────────
//  Public builder functions
// ─────────────────────────────────────────────────────────────────

/// Build the permission redeem script with a known redeem length.
pub fn build_permission_redeem(
    root: &[u32; 8],
    unclaimed_count: u64,
    depth: usize,
    redeem_script_len: i64,
    max_delegate_inputs: NonZeroUsize,
) -> Vec<u8> {
    PermissionRedeem { depth, redeem_script_len, max_delegate_inputs }.build(root, unclaimed_count)
}

/// Build permission redeem with converging length loop.
pub fn build_permission_redeem_converged(
    root: &[u32; 8],
    unclaimed_count: u64,
    depth: usize,
    max_delegate_inputs: NonZeroUsize,
) -> Vec<u8> {
    let mut len = 200i64;
    loop {
        let script = build_permission_redeem(root, unclaimed_count, depth, len, max_delegate_inputs);
        let new_len = script.len() as i64;
        if new_len == len {
            return script;
        }
        len = new_len;
    }
}

// ─────────────────────────────────────────────────────────────────
//  Merkle step helper
// ─────────────────────────────────────────────────────────────────

/// One step of a Merkle walk: combine current_hash with a sibling.
///
/// Expects: `[..., sib(32B), dir(0|1), current_hash(32B)]`
/// Leaves:  `[..., SHA256("PermBranch" || left || right)]`
///
/// `dir` selects child position:
/// - dir == 0 -> current is left child:  left = current_hash, right = sib
/// - dir == 1 -> current is right child: left = sib, right = current_hash
fn emit_merkle_step(b: &mut ScriptBuilder) {
    b.add_op(OpSwap).unwrap();
    b.add_op(OpIf).unwrap();
    // dir == 1: Cat -> sib||current
    b.add_op(OpElse).unwrap();
    b.add_op(OpSwap).unwrap();
    // dir == 0: Cat -> current||sib
    b.add_op(OpEndIf).unwrap();
    b.add_op(OpCat).unwrap();
    b.add_data(b"PermBranch").unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    b.add_op(OpSHA256).unwrap();
}

// ─────────────────────────────────────────────────────────────────
//  Hash helpers
// ─────────────────────────────────────────────────────────────────

/// Hash redeem script -> P2SH SPK bytes (version-prefixed).
///
/// Expects: `[..., redeem_script]`
/// Leaves:  `[..., spk_bytes(37B)]`
///
/// Produces `version(2B LE) || OpBlake2b || OpData32 || blake2b(redeem) || OpEqual`
/// matching `ScriptPublicKey::to_bytes()` format for P2SH.
fn emit_hash_redeem_to_spk(b: &mut ScriptBuilder) {
    b.add_op(OpBlake2b).unwrap();
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&TX_VERSION.to_le_bytes());
    data[2] = OpBlake2b;
    data[3] = OpData32;
    b.add_data(&data).unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    b.add_data(&[OpEqual]).unwrap();
    b.add_op(OpCat).unwrap();
}

// ─────────────────────────────────────────────────────────────────
//  Permission sig_script builder
// ─────────────────────────────────────────────────────────────────

// ANCHOR: build_permission_sig_script
/// Build the permission sig_script for a withdrawal claim.
///
/// `deduct` uses minimal i64 encoding (via `add_i64`); a copy is stashed to alt
/// by the redeem script for later use in delegate balance verification.
/// `amount` uses 8-byte LE (via `add_data`) because leaf hashes need exact bytes.
pub fn build_permission_sig_script(spk: &[u8], amount: u64, deduct: u64, proof: &PermProof, permission_redeem: &[u8]) -> Vec<u8> {
    let mut b = ScriptBuilder::new();

    // G2: merkle path for new root walk (pushed in reverse depth order)
    for level in (0..proof.depth).rev() {
        b.add_data(bytemuck::bytes_of(&proof.siblings[level])).unwrap();
        b.add_i64(((proof.index >> level) & 1) as i64).unwrap();
    }

    // G1: same merkle path for old root walk (identical data)
    for level in (0..proof.depth).rev() {
        b.add_data(bytemuck::bytes_of(&proof.siblings[level])).unwrap();
        b.add_i64(((proof.index >> level) & 1) as i64).unwrap();
    }

    // Leaf data
    b.add_data(spk).unwrap();
    b.add_data(&amount.to_le_bytes()).unwrap(); // 8-byte LE for leaf hash
    b.add_i64(deduct as i64).unwrap(); // minimal i64 for arithmetic

    // Redeem script (P2SH pops this last)
    b.add_data(permission_redeem).unwrap();

    b.drain()
}
// ANCHOR_END: build_permission_sig_script

// ─────────────────────────────────────────────────────────────────
//  Delegate / entry script
// ─────────────────────────────────────────────────────────────────

// ANCHOR: build_delegate_entry_script
/// Build the delegate/entry script (used as a P2SH redeem).
///
/// This script allows additional transaction inputs to ride alongside a
/// permission input. It verifies that input 0 is a legitimate permission
/// script by checking covenant ID and domain suffix.
///
/// Expects: `[]` (empty — no sig_script data needed, only introspection)
/// Leaves:  `[TRUE]`
///
/// Verifies:
/// 1. Self is not at input index 0 (reserved for permission script)
/// 2. Input 0 carries the expected `covenant_id`
/// 3. Input 0's sig_script ends with `[0x51, 0x75]` (permission domain suffix)
pub fn build_delegate_entry_script(permission_covenant_id: &[u8; 32]) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // ── Step 1: verify self not at input 0 ──
    builder.add_op(OpTxInputIndex).unwrap();
    builder.add_i64(0).unwrap();
    builder.add_op(OpGreaterThan).unwrap();
    builder.add_op(OpVerify).unwrap();

    // ── Step 2: check covenant ID of input 0 ──
    builder.add_op(Op0).unwrap();
    builder.add_op(OpInputCovenantId).unwrap();
    builder.add_data(permission_covenant_id).unwrap();
    builder.add_op(OpEqualVerify).unwrap();

    // ── Step 3: verify input 0's sig_script ends with permission domain suffix ──
    builder.add_op(Op0).unwrap(); // idx for Substr
    builder.add_op(Op0).unwrap(); // idx for SigLen
    builder.add_op(OpTxInputScriptSigLen).unwrap();
    builder.add_op(OpDup).unwrap();
    builder.add_i64(2).unwrap();
    builder.add_op(OpSub).unwrap();
    builder.add_op(OpSwap).unwrap();
    builder.add_op(OpTxInputScriptSigSubstr).unwrap();
    builder.add_data(&ScriptDomain::Permission.suffix_bytes()).unwrap();
    builder.add_op(OpEqualVerify).unwrap();

    builder.add_op(OpTrue).unwrap();

    builder.drain()
}
// ANCHOR_END: build_delegate_entry_script
