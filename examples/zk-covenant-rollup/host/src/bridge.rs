use kaspa_consensus_core::constants::TX_VERSION;
use kaspa_txscript::opcodes::codes::{
    Op0, OpAdd, OpBlake2b, OpCat, OpCovOutCount, OpData32, OpDrop, OpDup, OpElse, OpEndIf, OpEqual, OpEqualVerify, OpFromAltStack,
    OpGreaterThan, OpGreaterThanOrEqual, OpIf, OpInputCovenantId, OpNip, OpNum2Bin, OpOver, OpRoll, OpRot, OpSHA256, OpSub, OpSwap,
    OpToAltStack, OpTrue, OpTxInputIndex, OpTxInputScriptSigLen, OpTxInputScriptSigSubstr, OpTxOutputSpk, OpVerify,
};
use kaspa_txscript::script_builder::ScriptBuilder;
use zk_covenant_rollup_core::permission_tree::PermProof;

#[cfg(test)]
mod tests;

/// Script domain suffixes for distinguishing different scripts within the same covenant.
///
/// Each domain has a 2-byte suffix (`[opcode, OP_DROP]`) appended to the redeem script
/// after the final `OP_ENDIF`. This is a no-op (pushes a value then drops it) so the
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

/// Permission redeem script prefix size in bytes.
///
/// Layout (42 bytes total):
/// - 1 byte:   `OpData32`
/// - 32 bytes:  root hash
/// - 1 byte:   `OP_DATA_8` (push-8-bytes opcode = 0x08)
/// - 8 bytes:   unclaimed_count (LE)
///
/// The domain tag (`OP_1, OP_DROP`) is a 2-byte **suffix** at the end of the
/// redeem, not part of this prefix.
pub const PERM_REDEEM_PREFIX_LEN: i64 = 42;

// ═══════════════════════════════════════════════════════════════════
//  Permission redeem script
// ═══════════════════════════════════════════════════════════════════

/// Build the permission redeem script with a known redeem length.
///
/// This script governs L2→L1 withdrawals. It embeds a Merkle tree root
/// and an unclaimed leaf count. When spent, the sig_script provides a
/// withdrawal claim (spk, amount, deduct_amount, merkle proof). The
/// script verifies the claim against the embedded root, computes the
/// updated tree root, and enforces that the first output carries the
/// updated script (unless unclaimed drops to 0).
///
/// # Redeem layout
/// ```text
/// prefix(42B) || body || suffix(2B)
/// ```
///
/// Prefix (42 bytes): `[OpData32, root(32B), OP_DATA_8, unclaimed(8B)]`
/// Suffix (2 bytes):  `[OP_1(0x51), OP_DROP(0x75)]` — domain tag (no-op)
///
/// # Sig_script push order (bottom of stack first)
/// ```text
/// G2_sib_{d-1}(32B), G2_dir_{d-1}(0|1), ..., G2_sib_0, G2_dir_0,
/// G1_sib_{d-1}(32B), G1_dir_{d-1}(0|1), ..., G1_sib_0, G1_dir_0,
/// spk(var), amount(8B LE), deduct(8B LE),
/// redeem_script
/// ```
///
/// G1 = siblings for old root verification, G2 = siblings for new root computation.
/// Both are identical copies of the same merkle proof.
///
/// # Stack after P2SH pops redeem + redeem pushes embedded (top→bottom)
/// ```text
/// uncl_emb, root_emb, deduct, amount, spk,
/// G1_dir_0, G1_sib_0, ..., G1_dir_{d-1}, G1_sib_{d-1},
/// G2_dir_0, G2_sib_0, ..., G2_dir_{d-1}, G2_sib_{d-1}
/// ```
pub fn build_permission_redeem(root: &[u32; 8], unclaimed_count: u64, depth: usize, redeem_script_len: i64) -> Vec<u8> {
    let mut b = ScriptBuilder::new();

    // ═══════════ PREFIX (42 bytes) ═══════════
    b.add_data(bytemuck::bytes_of(root)).unwrap(); // OpData32 + root(32)
    b.add_data(&unclaimed_count.to_le_bytes()).unwrap(); // OP_DATA_8 + unclaimed(8)
                                                         // Stack: [..., uncl_emb(8B), root_emb(32B)]

    // ═══════════ P0: stash embedded data ═══════════
    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., uncl_emb], alt:[root_emb]   (note: alt shown top-first from here on)
    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., deduct, amount, spk, G1..., G2...], alt:[root_emb, uncl_emb]

    // ═══════════ P1: validate amounts ═══════════
    // Check deduct > 0
    b.add_op(OpDup).unwrap();
    b.add_i64(0).unwrap();
    b.add_op(OpGreaterThan).unwrap();
    b.add_op(OpVerify).unwrap();

    // Compute new_amount = amount - deduct; check >= 0
    // OpSub pops b(top), a(second) and pushes a − b.
    b.add_op(OpOver).unwrap();
    // Stack: [..., amount, deduct, amount]
    b.add_op(OpSwap).unwrap();
    // Stack: [..., amount, amount, deduct]
    b.add_op(OpSub).unwrap();
    // Stack: [..., amount, new_amount]          (new_amount in minimal i64 encoding)
    b.add_op(OpDup).unwrap();
    b.add_i64(0).unwrap();
    b.add_op(OpGreaterThanOrEqual).unwrap();
    b.add_op(OpVerify).unwrap();
    // Stack: [..., new_amount, amount, spk, G1..., G2...]

    // ═══════════ P2: compute leaf hashes ═══════════

    // 2a. is_zero = (new_amount == 0), stash for P5
    b.add_op(OpDup).unwrap();
    b.add_i64(0).unwrap();
    b.add_op(OpEqual).unwrap();
    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., new_amount, amount, spk, G1...], alt:[root, uncl, is_zero]

    // 2b. new_amount → 8-byte LE for hashing (OpNum2Bin: sign-magnitude, same as u64 LE for ≥0)
    b.add_i64(8).unwrap();
    b.add_op(OpNum2Bin).unwrap();
    // Stack: [..., new_amt_8b(8B), amount(8B), spk, G1...]

    // 2c. Dup spk for reuse in old leaf hash
    // OpRot: [a,b,c] → [b,c,a] (3rd-from-top moves to top)
    b.add_op(OpRot).unwrap();
    // Stack: [..., spk, new_amt_8b, amount, G1...]
    b.add_op(OpDup).unwrap();
    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., spk, new_amt_8b, amount, G1...], alt:[root, uncl, is_zero, spk_dup]

    // 2d. New leaf (nonzero): SHA256("PermLeaf" || spk || new_amt_8b)
    // OpCat pops b(top), a(second), pushes a||b.
    b.add_op(OpSwap).unwrap();
    // Stack: [..., new_amt_8b, spk, amount, G1...]
    b.add_op(OpCat).unwrap();
    // Stack: [..., spk||new_amt_8b, amount, G1...]
    b.add_data(b"PermLeaf").unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    // Stack: [..., "PermLeaf"||spk||new_amt_8b, amount, G1...]
    b.add_op(OpSHA256).unwrap();
    // Stack: [..., new_leaf_nz(32B), amount, G1...]

    // 2e. Empty leaf hash (constant)
    b.add_data(b"PermEmpty").unwrap();
    b.add_op(OpSHA256).unwrap();
    // Stack: [..., empty_h(32B), new_leaf_nz, amount, G1...]

    // 2f. Select new_leaf based on is_zero; re-stash is_zero and spk_dup
    b.add_op(OpFromAltStack).unwrap(); // spk_dup
    b.add_op(OpFromAltStack).unwrap(); // is_zero
                                       // Stack: [..., is_zero, spk_dup, empty_h, new_leaf_nz, amount, G1...], alt:[root, uncl]

    b.add_op(OpDup).unwrap();
    b.add_op(OpToAltStack).unwrap(); // re-stash is_zero
    b.add_op(OpSwap).unwrap();
    b.add_op(OpToAltStack).unwrap(); // re-stash spk_dup
                                     // Stack: [..., is_zero, empty_h, new_leaf_nz, amount, G1...], alt:[root, uncl, is_zero, spk_dup]

    b.add_op(OpIf).unwrap();
    b.add_op(OpNip).unwrap(); // is_zero true: keep empty_h, drop new_leaf_nz
    b.add_op(OpElse).unwrap();
    b.add_op(OpDrop).unwrap(); // is_zero false: keep new_leaf_nz, drop empty_h
    b.add_op(OpEndIf).unwrap();
    // Stack: [..., new_leaf(32B), amount(8B), G1..., G2...]

    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., amount, G1...], alt:[root, uncl, is_zero, spk_dup, new_leaf]

    // 2g. Old leaf hash: SHA256("PermLeaf" || spk || amount)
    //     `amount` here is the original 8-byte LE from sig_script (no arithmetic was applied to it).
    b.add_op(OpFromAltStack).unwrap(); // new_leaf
    b.add_op(OpFromAltStack).unwrap(); // spk_dup
                                       // Stack: [..., spk_dup, new_leaf, amount, G1...], alt:[root, uncl, is_zero]
    b.add_op(OpRot).unwrap();
    // Stack: [..., amount, spk_dup, new_leaf, G1...]
    b.add_op(OpCat).unwrap();
    // Stack: [..., spk_dup||amount, new_leaf, G1...]     (Cat: spk_dup(second)||amount(top))
    b.add_data(b"PermLeaf").unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    // Stack: [..., "PermLeaf"||spk||amount, new_leaf, G1...]
    b.add_op(OpSHA256).unwrap();
    // Stack: [..., old_leaf(32B), new_leaf, G1...]

    // Stash new_leaf for P4
    b.add_op(OpSwap).unwrap();
    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., old_leaf, G1..., G2...], alt:[root, uncl, is_zero, new_leaf]

    // ═══════════ P3: old root merkle walk ═══════════
    // Each step: Expects [..., sib, dir, hash], Leaves [..., parent_hash]
    for _ in 0..depth {
        emit_merkle_step(&mut b);
    }
    // Stack: [..., computed_old_root(32B), G2...], alt:[root, uncl, is_zero, new_leaf]

    // Pop from alt to verify root and set up P4
    b.add_op(OpFromAltStack).unwrap(); // new_leaf
    b.add_op(OpFromAltStack).unwrap(); // is_zero
    b.add_op(OpFromAltStack).unwrap(); // root_emb
                                       // Stack: [..., root_emb, is_zero, new_leaf, computed_old_root, G2...], alt:[uncl]

    // Bring computed_old_root to top (it's at position 3 from top)
    b.add_i64(3).unwrap();
    b.add_op(OpRoll).unwrap();
    // Stack: [..., computed_old_root, root_emb, is_zero, new_leaf, G2...]
    b.add_op(OpEqualVerify).unwrap();
    // Stack: [..., is_zero, new_leaf, G2...]

    b.add_op(OpToAltStack).unwrap();
    // Stack: [..., new_leaf, G2...], alt:[uncl, is_zero]

    // ═══════════ P4: new root merkle walk ═══════════
    for _ in 0..depth {
        emit_merkle_step(&mut b);
    }
    // Stack: [new_root(32B)], alt:[uncl, is_zero]

    // ═══════════ P5: compute new_uncl ═══════════
    b.add_op(OpFromAltStack).unwrap(); // is_zero
    b.add_op(OpFromAltStack).unwrap(); // uncl_emb
                                       // Stack: [uncl_emb, is_zero, new_root]

    // new_uncl = if is_zero then uncl-1 else uncl
    b.add_op(OpSwap).unwrap();
    // Stack: [is_zero, uncl_emb, new_root]
    b.add_op(OpIf).unwrap();
    b.add_i64(1).unwrap();
    b.add_op(OpSub).unwrap(); // uncl - 1
    b.add_op(OpElse).unwrap();
    // uncl unchanged
    b.add_op(OpEndIf).unwrap();
    // Stack: [new_uncl, new_root]

    // Convert to 8-byte LE for prefix reconstruction
    b.add_i64(8).unwrap();
    b.add_op(OpNum2Bin).unwrap();
    // Stack: [new_uncl_8b(8B), new_root(32B)]

    // ═══════════ P6: conditional output verification ═══════════

    // Check if all exits claimed (expected_new_uncl == 0)
    b.add_op(OpDup).unwrap();
    b.add_data(&0u64.to_le_bytes()).unwrap();
    b.add_op(OpEqual).unwrap();
    // Stack: [is_done, new_uncl_8b, new_root]

    b.add_op(OpIf).unwrap();
    {
        // All exits claimed — no continuation output needed.
        b.add_op(OpDrop).unwrap(); // drop new_uncl_8b
        b.add_op(OpDrop).unwrap(); // drop new_root
                                   // Stack: []

        // V3: verify no covenant continuation outputs remain
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpInputCovenantId).unwrap();
        b.add_op(OpCovOutCount).unwrap();
        b.add_i64(0).unwrap();
        b.add_op(OpEqualVerify).unwrap();
        b.add_op(OpTrue).unwrap();
        // Stack: [TRUE]
    }
    b.add_op(OpElse).unwrap();
    {
        // Unclaimed exits remain — build continuation output with updated permission redeem.
        // Stack: [new_uncl_8b(8B), new_root(32B)]
        // Target prefix: [OpData32, new_root(32), 0x08, new_uncl(8)]  (42B)

        // Step 1: build unclaimed part [0x08 || new_uncl_8b]
        b.add_data(&[0x08u8]).unwrap();
        // Stack: [[0x08], new_uncl_8b, new_root]
        b.add_op(OpSwap).unwrap();
        // Stack: [new_uncl_8b, [0x08], new_root]
        b.add_op(OpCat).unwrap();
        // Stack: [[0x08||uncl_8b](9B), new_root]          (Cat: [0x08](second)||new_uncl_8b(top))

        // Step 2: build root part [OpData32 || new_root]
        b.add_op(OpSwap).unwrap();
        // Stack: [new_root, [0x08||uncl_8b]]
        b.add_data(&[OpData32]).unwrap();
        b.add_op(OpSwap).unwrap();
        // Stack: [new_root, [0x20], [0x08||uncl_8b]]
        b.add_op(OpCat).unwrap();
        // Stack: [[0x20||root](33B), [0x08||uncl_8b]]     (Cat: [0x20](second)||root(top))

        // Step 3: concat both halves → new prefix (42B)
        b.add_op(OpSwap).unwrap();
        b.add_op(OpCat).unwrap();
        // Stack: [new_prefix(42B)]                         (Cat: [0x20||root](second)||[0x08||uncl](top))

        // Step 4: extract redeem body+suffix from own sig_script, concat with new prefix → new_redeem
        //   sig_script layout: [...pushes... | push_header | redeem_bytes(redeem_script_len)]
        //   redeem_bytes = prefix(42B) || body || domain_suffix(2B)
        //   body+suffix starts at: sig_len − redeem_script_len + PERM_REDEEM_PREFIX_LEN
        b.add_op(OpTxInputIndex).unwrap(); // idx for Substr
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpTxInputScriptSigLen).unwrap(); // sig_len
        b.add_i64(-redeem_script_len + PERM_REDEEM_PREFIX_LEN).unwrap();
        b.add_op(OpAdd).unwrap(); // start = sig_len − redeem_len + 42
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpTxInputScriptSigLen).unwrap(); // end = sig_len
        b.add_op(OpTxInputScriptSigSubstr).unwrap(); // Substr(idx, start, end) → body+suffix
        b.add_op(OpCat).unwrap();
        // Stack: [new_redeem]                               (Cat: new_prefix(second)||body+suffix(top))

        // Step 5: hash new redeem → expected SPK bytes
        emit_hash_redeem_to_spk(&mut b);
        // Stack: [expected_spk(37B)]

        // Step 6: verify output SPK matches
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpTxOutputSpk).unwrap();
        // Stack: [actual_spk, expected_spk]
        b.add_op(OpEqualVerify).unwrap();
        // Stack: []

        // Step 7: verify covenant continuity (exactly one covenant output)
        b.add_op(OpTxInputIndex).unwrap();
        b.add_op(OpInputCovenantId).unwrap();
        b.add_op(OpCovOutCount).unwrap();
        b.add_i64(1).unwrap();
        b.add_op(OpEqualVerify).unwrap();

        b.add_op(OpTrue).unwrap();
        // Stack: [TRUE]
    }
    b.add_op(OpEndIf).unwrap();

    // ═══════════ DOMAIN SUFFIX (2 bytes) ═══════════
    // No-op tag: pushes 1 then drops it, leaving the stack unchanged.
    // The delegate verifies these are the last 2 bytes of input 0's sig_script
    // to confirm it is a permission script (not state verification).
    b.add_op(OpTrue).unwrap(); // 0x51
    b.add_op(OpDrop).unwrap(); // 0x75

    b.drain()
}

/// Build permission redeem with converging length loop.
pub fn build_permission_redeem_converged(root: &[u32; 8], unclaimed_count: u64, depth: usize) -> Vec<u8> {
    let mut len = 200i64;
    loop {
        let script = build_permission_redeem(root, unclaimed_count, depth, len);
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
/// - dir == 0 → current is left child:  left = current_hash, right = sib
/// - dir == 1 → current is right child: left = sib, right = current_hash
///
/// OpCat pops `b`(top), `a`(second) and pushes `a||b`.
fn emit_merkle_step(b: &mut ScriptBuilder) {
    // Stack: [..., sib, dir, current_hash]
    b.add_op(OpSwap).unwrap();
    // Stack: [..., sib, current_hash, dir]

    b.add_op(OpIf).unwrap();
    // dir == 1: Stack: [..., sib, current_hash]. Cat → sib||current ✓
    b.add_op(OpElse).unwrap();
    // dir == 0:
    b.add_op(OpSwap).unwrap();
    // Stack: [..., current_hash, sib]. Cat → current||sib ✓
    b.add_op(OpEndIf).unwrap();

    // Stack: [..., left, right]
    b.add_op(OpCat).unwrap();
    // Stack: [..., left||right]
    b.add_data(b"PermBranch").unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    // Stack: [..., "PermBranch"||left||right]
    b.add_op(OpSHA256).unwrap();
    // Stack: [..., branch_hash(32B)]
}

// ─────────────────────────────────────────────────────────────────
//  Hash helpers (mirror CovenantBase from zk-covenant-common)
// ─────────────────────────────────────────────────────────────────

/// Hash redeem script → P2SH SPK bytes (version-prefixed).
/// Identical to `CovenantBase::hash_redeem_to_spk`.
///
/// Expects: `[..., redeem_script]`
/// Leaves:  `[..., spk_bytes]`
///
/// Produces `version(2B LE) || OpBlake2b || OpData32 || blake2b(redeem) || OpEqual`
/// matching `ScriptPublicKey::to_bytes()` format for P2SH.
fn emit_hash_redeem_to_spk(b: &mut ScriptBuilder) {
    // Stack: [..., redeem_script]
    b.add_op(OpBlake2b).unwrap();
    // Stack: [..., blake2b_hash(32B)]
    let mut data = [0u8; 4];
    data[0..2].copy_from_slice(&TX_VERSION.to_le_bytes());
    data[2] = OpBlake2b;
    data[3] = OpData32;
    b.add_data(&data).unwrap();
    b.add_op(OpSwap).unwrap();
    b.add_op(OpCat).unwrap();
    // Stack: [..., (version||OpBlake2b||OpData32||hash)]  (36B)
    b.add_data(&[OpEqual]).unwrap();
    b.add_op(OpCat).unwrap();
    // Stack: [..., spk_bytes(37B)]
}

// ─────────────────────────────────────────────────────────────────
//  Permission sig_script builder
// ─────────────────────────────────────────────────────────────────

/// Build the permission sig_script for a withdrawal claim.
///
/// Pushes all data needed by the permission redeem script in the correct order.
pub fn build_permission_sig_script(spk: &[u8], amount: u64, deduct: u64, proof: &PermProof, permission_redeem: &[u8]) -> Vec<u8> {
    let mut b = ScriptBuilder::new();

    // G2: merkle path for new root walk (pushed in reverse depth order, sits at stack bottom)
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
    b.add_data(&amount.to_le_bytes()).unwrap();
    b.add_data(&deduct.to_le_bytes()).unwrap();

    // Redeem script (P2SH pops this last)
    b.add_data(permission_redeem).unwrap();

    b.drain()
}

// ─────────────────────────────────────────────────────────────────
//  Delegate / entry script
// ─────────────────────────────────────────────────────────────────

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
///
/// The domain check extracts the last 2 bytes of input 0's sig_script.
/// Since the redeem script is the last push in a P2SH sig_script, the
/// last bytes of the sig_script are the last bytes of the redeem. The
/// permission redeem always ends with `[OP_1(0x51), OP_DROP(0x75)]`.
pub fn build_delegate_entry_script(permission_covenant_id: &[u8; 32]) -> Vec<u8> {
    let mut builder = ScriptBuilder::new();

    // ── Step 1: verify self not at input 0 ──
    builder.add_op(OpTxInputIndex).unwrap();
    builder.add_i64(0).unwrap();
    builder.add_op(OpGreaterThan).unwrap();
    builder.add_op(OpVerify).unwrap();
    // Stack: []

    // ── Step 2: check covenant ID of input 0 ──
    builder.add_op(Op0).unwrap();
    builder.add_op(OpInputCovenantId).unwrap();
    // Stack: [cov_id_of_input_0(32B)]
    builder.add_data(permission_covenant_id).unwrap();
    builder.add_op(OpEqualVerify).unwrap();
    // Stack: []

    // ── Step 3: verify input 0's sig_script ends with permission domain suffix ──
    //   Extracts sig_script_0[sig_len-2 .. sig_len] — the last 2 bytes.
    //   These are the last 2 bytes of the redeem (= domain suffix).
    builder.add_op(Op0).unwrap(); // idx for Substr
    builder.add_op(Op0).unwrap(); // idx for SigLen
    builder.add_op(OpTxInputScriptSigLen).unwrap();
    // Stack: [0, sig_len_0]
    builder.add_op(OpDup).unwrap();
    // Stack: [0, sig_len_0, sig_len_0]
    builder.add_i64(2).unwrap();
    builder.add_op(OpSub).unwrap();
    // Stack: [0, sig_len_0, sig_len_0 - 2]
    builder.add_op(OpSwap).unwrap();
    // Stack: [0, sig_len_0 - 2, sig_len_0]
    builder.add_op(OpTxInputScriptSigSubstr).unwrap();
    // Stack: [last 2 bytes of sig_script_0]
    builder.add_data(&ScriptDomain::Permission.suffix_bytes()).unwrap();
    builder.add_op(OpEqualVerify).unwrap();
    // Stack: []

    builder.add_op(OpTrue).unwrap();
    // Stack: [TRUE]

    builder.drain()
}
