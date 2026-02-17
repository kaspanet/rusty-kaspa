//! Permission redeem script construction (`no_std` compatible).
//!
//! Byte-level port of `host::bridge::PermissionRedeemEmitter`. Produces
//! identical bytes without requiring `ScriptBuilder`.

extern crate alloc;
use alloc::vec::Vec;

use crate::p2sh::build_perm_redeem_prefix;

// ─────────────────────────────────────────────────────────────────
//  Opcode byte constants
// ─────────────────────────────────────────────────────────────────

const OP_FALSE: u8 = 0x00;
const OP_TRUE: u8 = 0x51;
const OP_IF: u8 = 0x63;
const OP_ELSE: u8 = 0x67;
const OP_ENDIF: u8 = 0x68;
const OP_VERIFY: u8 = 0x69;
const OP_TOALTSTACK: u8 = 0x6b;
const OP_FROMALTSTACK: u8 = 0x6c;
const OP_DROP: u8 = 0x75;
const OP_DUP: u8 = 0x76;
const OP_NIP: u8 = 0x77;
const OP_OVER: u8 = 0x78;
const OP_ROLL: u8 = 0x7a;
const OP_ROT: u8 = 0x7b;
const OP_SWAP: u8 = 0x7c;
const OP_CAT: u8 = 0x7e;
const OP_EQUAL: u8 = 0x87;
const OP_EQUALVERIFY: u8 = 0x88;
const OP_ADD: u8 = 0x93;
const OP_SUB: u8 = 0x94;
const OP_GREATERTHAN: u8 = 0xa0;
const OP_GREATERTHANOREQUAL: u8 = 0xa2;
const OP_SHA256: u8 = 0xa8;
const OP_BLAKE2B: u8 = 0xaa;
const OP_NUM2BIN: u8 = 0xcd;
const OP_TXINPUTCOUNT: u8 = 0xb3;
const OP_TXOUTPUTCOUNT: u8 = 0xb4;
const OP_TXINPUTINDEX: u8 = 0xb9;
const OP_TXINPUTSCRIPTSIGSUBSTR: u8 = 0xbc;
const OP_TXINPUTAMOUNT: u8 = 0xbe;
const OP_TXINPUTSPK: u8 = 0xbf;
const OP_TXOUTPUTAMOUNT: u8 = 0xc2;
const OP_TXOUTPUTSPK: u8 = 0xc3;
const OP_TXINPUTSCRIPTSIGLEN: u8 = 0xc9;
const OP_INPUTCOVENANTID: u8 = 0xcf;
const OP_COVOUTCOUNT: u8 = 0xd2;

// ─────────────────────────────────────────────────────────────────
//  Script constants
// ─────────────────────────────────────────────────────────────────

/// Permission redeem prefix length in bytes.
const PERM_REDEEM_PREFIX_LEN: i64 = 42;

/// Maximum transaction outputs allowed by the permission script.
const MAX_OUTPUTS: i64 = 4;

/// Delegate entry script bytes **before** the 32-byte covenant_id (bytes 0..7).
const DELEGATE_SCRIPT_PREFIX: [u8; 7] = [0xb9, 0x00, 0xa0, 0x69, 0x00, 0xcf, 0x20];

/// Delegate entry script bytes **after** the 32-byte covenant_id (bytes 39..53).
const DELEGATE_SCRIPT_SUFFIX: [u8; 14] = [0x88, 0x00, 0x00, 0xc9, 0x76, 0x52, 0x94, 0x7c, 0xbc, 0x02, 0x51, 0x75, 0x88, 0x51];

// ─────────────────────────────────────────────────────────────────
//  ScriptVec — minimal Vec<u8> wrapper with encoding methods
// ─────────────────────────────────────────────────────────────────

/// Minimal script byte buffer with encoding methods matching `ScriptBuilder`.
struct ScriptVec(Vec<u8>);

impl ScriptVec {
    fn with_capacity(cap: usize) -> Self {
        Self(Vec::with_capacity(cap))
    }

    /// Push a single opcode byte.
    #[inline]
    fn push_op(&mut self, op: u8) {
        self.0.push(op);
    }

    /// Push data with a length prefix (len ≤ 75).
    #[inline]
    fn push_data(&mut self, data: &[u8]) {
        debug_assert!(data.len() <= 75, "push_data only supports len ≤ 75");
        self.0.push(data.len() as u8);
        self.0.extend_from_slice(data);
    }

    /// Push an i64 value using minimal script number encoding.
    ///
    /// Matches `ScriptBuilder::add_i64`:
    /// - 0 → Op0 (0x00)
    /// - -1 → Op1Negate (0x4f)
    /// - 1..16 → Op1..Op16 (0x51..0x60)
    /// - Otherwise: minimal LE encoding with sign bit via push_data
    fn push_i64(&mut self, val: i64) {
        match val {
            0 => self.0.push(0x00),
            -1 => self.0.push(0x4f),
            v @ 1..=16 => self.0.push(0x50 + v as u8),
            _ => {
                let negative = val < 0;
                let mut abs_val = if negative { (-(val as i128)) as u64 } else { val as u64 };

                let mut buf = [0u8; 9];
                let mut len = 0usize;
                while abs_val > 0 {
                    buf[len] = (abs_val & 0xFF) as u8;
                    abs_val >>= 8;
                    len += 1;
                }

                if buf[len - 1] & 0x80 != 0 {
                    buf[len] = if negative { 0x80 } else { 0x00 };
                    len += 1;
                } else if negative {
                    buf[len - 1] |= 0x80;
                }

                self.push_data(&buf[..len]);
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────
//  Public builder functions
// ─────────────────────────────────────────────────────────────────

/// Build the permission redeem script as raw bytes (`no_std` compatible).
///
/// Byte-level port of `host::bridge::PermissionRedeem::build()`. Produces
/// identical output without requiring `ScriptBuilder`.
pub fn build_permission_redeem_bytes(
    root: &[u32; 8],
    unclaimed_count: u64,
    depth: usize,
    redeem_script_len: i64,
    max_delegate_inputs: usize,
) -> Vec<u8> {
    let mut s = ScriptVec::with_capacity(redeem_script_len.max(64) as usize);

    // Phase 1: Prefix (42 bytes)
    let prefix = build_perm_redeem_prefix(root, unclaimed_count);
    s.0.extend_from_slice(&prefix);

    // Phase 2: Stash embedded
    s.push_op(OP_TOALTSTACK); // stash uncl_emb
    s.push_op(OP_TOALTSTACK); // stash root_emb

    // Phase 3: Validate amounts
    emit_validate_amounts(&mut s);

    // Phase 4: Verify withdrawal
    emit_verify_withdrawal(&mut s);

    // Phase 5: Compute leaf hashes
    emit_compute_leaf_hashes(&mut s);

    // Phase 6: Verify old root
    emit_verify_old_root(&mut s, depth);

    // Phase 7: Compute new root
    emit_compute_new_root(&mut s, depth);

    // Phase 8: Compute new unclaimed
    emit_compute_new_unclaimed(&mut s);

    // Phase 9: Verify outputs
    emit_verify_outputs(&mut s, redeem_script_len);

    // Phase 10: Verify delegate balance
    emit_verify_delegate_balance(&mut s, max_delegate_inputs);

    // Final TRUE for P2SH
    s.push_op(OP_TRUE);

    // Phase 11: Domain suffix [OP_1, OP_DROP]
    s.push_op(OP_TRUE);
    s.push_op(OP_DROP);

    s.0
}

/// Build permission redeem with converging length loop.
///
/// Iterates `build_permission_redeem_bytes` until the script length
/// stabilises (because the script embeds its own length).
pub fn build_permission_redeem_bytes_converged(
    root: &[u32; 8],
    unclaimed_count: u64,
    depth: usize,
    max_delegate_inputs: usize,
) -> Vec<u8> {
    let mut len = 200i64;
    loop {
        let script = build_permission_redeem_bytes(root, unclaimed_count, depth, len, max_delegate_inputs);
        let new_len = script.len() as i64;
        if new_len == len {
            return script;
        }
        len = new_len;
    }
}

// ─────────────────────────────────────────────────────────────────
//  Phase implementations
// ─────────────────────────────────────────────────────────────────

/// Phase 3: Validate `deduct > 0` and `amount >= deduct`.
fn emit_validate_amounts(s: &mut ScriptVec) {
    // verify deduct > 0
    s.push_op(OP_DUP);
    s.push_i64(0);
    s.push_op(OP_GREATERTHAN);
    s.push_op(OP_VERIFY);

    // stash deduct to alt for later use
    s.push_op(OP_DUP);
    s.push_op(OP_TOALTSTACK);

    // new_amount = amount - deduct; verify >= 0
    s.push_op(OP_OVER);
    s.push_op(OP_SWAP);
    s.push_op(OP_SUB);
    s.push_op(OP_DUP);
    s.push_i64(0);
    s.push_op(OP_GREATERTHANOREQUAL);
    s.push_op(OP_VERIFY);
}

/// Phase 4: Verify output 0's SPK matches the leaf's SPK.
fn emit_verify_withdrawal(s: &mut ScriptVec) {
    // bring spk to top
    s.push_i64(2);
    s.push_op(OP_ROLL);

    s.push_op(OP_DUP);

    // prepend SPK version prefix (0x0000 for version 0)
    s.push_data(&0u16.to_be_bytes());
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);

    // compare with output 0's SPK
    s.push_i64(0);
    s.push_op(OP_TXOUTPUTSPK);
    s.push_op(OP_EQUALVERIFY);

    // restore stack: spk back to depth 2
    s.push_op(OP_ROT);
    s.push_op(OP_ROT);
}

/// Phase 5: Compute old and new leaf hashes.
fn emit_compute_leaf_hashes(s: &mut ScriptVec) {
    // is_zero = (new_amount == 0), stash
    s.push_op(OP_DUP);
    s.push_i64(0);
    s.push_op(OP_EQUAL);
    s.push_op(OP_TOALTSTACK);

    // new_amount -> 8-byte LE
    s.push_i64(8);
    s.push_op(OP_NUM2BIN);

    // dup spk for reuse in old leaf hash
    s.push_op(OP_ROT);
    s.push_op(OP_DUP);
    s.push_op(OP_TOALTSTACK);

    // new leaf (nonzero): SHA256("PermLeaf" || spk || new_amt_8b)
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_data(b"PermLeaf");
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_op(OP_SHA256);

    // empty leaf hash
    s.push_data(b"PermEmpty");
    s.push_op(OP_SHA256);

    // select new_leaf based on is_zero; re-stash is_zero and spk_dup
    s.push_op(OP_FROMALTSTACK); // spk_dup
    s.push_op(OP_FROMALTSTACK); // is_zero
    s.push_op(OP_DUP);
    s.push_op(OP_TOALTSTACK); // re-stash is_zero
    s.push_op(OP_SWAP);
    s.push_op(OP_TOALTSTACK); // re-stash spk_dup

    s.push_op(OP_IF);
    s.push_op(OP_NIP); // is_zero true: keep empty_h, drop new_leaf_nz
    s.push_op(OP_ELSE);
    s.push_op(OP_DROP); // is_zero false: keep new_leaf_nz, drop empty_h
    s.push_op(OP_ENDIF);

    s.push_op(OP_TOALTSTACK); // stash new_leaf

    // old leaf hash: SHA256("PermLeaf" || spk || amount)
    s.push_op(OP_FROMALTSTACK); // new_leaf
    s.push_op(OP_FROMALTSTACK); // spk_dup

    s.push_op(OP_ROT); // [new_leaf, spk_dup, amount]
    s.push_op(OP_CAT); // [new_leaf, spk_dup||amount]
    s.push_data(b"PermLeaf");
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_op(OP_SHA256);

    // swap and stash new_leaf
    s.push_op(OP_SWAP);
    s.push_op(OP_TOALTSTACK);
}

/// Phase 6: Verify old root matches embedded root.
fn emit_verify_old_root(s: &mut ScriptVec, depth: usize) {
    for _ in 0..depth {
        emit_merkle_step(s);
    }

    // Restore from alt stack
    s.push_op(OP_FROMALTSTACK); // new_leaf
    s.push_op(OP_FROMALTSTACK); // is_zero
    s.push_op(OP_FROMALTSTACK); // deduct
    s.push_op(OP_FROMALTSTACK); // root_emb

    // Bring computed_old_root to top (position 4 from top)
    s.push_i64(4);
    s.push_op(OP_ROLL);
    s.push_op(OP_EQUALVERIFY);

    // Re-stash deduct and is_zero, leave new_leaf on main
    s.push_op(OP_ROT);
    s.push_op(OP_SWAP);
    s.push_op(OP_TOALTSTACK); // stash deduct
    s.push_op(OP_SWAP);
    s.push_op(OP_TOALTSTACK); // stash is_zero
}

/// Phase 7: Compute new root from new_leaf.
fn emit_compute_new_root(s: &mut ScriptVec, depth: usize) {
    for _ in 0..depth {
        emit_merkle_step(s);
    }
}

/// Phase 8: Compute new_unclaimed.
fn emit_compute_new_unclaimed(s: &mut ScriptVec) {
    s.push_op(OP_FROMALTSTACK); // is_zero
    s.push_op(OP_FROMALTSTACK); // deduct
    s.push_op(OP_FROMALTSTACK); // uncl_emb

    // Bring is_zero to top
    s.push_op(OP_ROT);

    // new_uncl = if is_zero { uncl - 1 } else { uncl }
    s.push_op(OP_IF);
    s.push_i64(1);
    s.push_op(OP_SUB);
    s.push_op(OP_ELSE);
    // uncl unchanged
    s.push_op(OP_ENDIF);

    // Convert to 8-byte LE
    s.push_i64(8);
    s.push_op(OP_NUM2BIN);

    // Rearrange to [deduct, new_root, new_uncl_8b]
    s.push_op(OP_ROT);
    s.push_op(OP_SWAP);
}

/// Phase 9: Verify transaction outputs.
fn emit_verify_outputs(s: &mut ScriptVec, redeem_script_len: i64) {
    // enforce output_count <= MAX_OUTPUTS
    s.push_op(OP_TXOUTPUTCOUNT);
    s.push_i64(MAX_OUTPUTS);
    s.push_op(OP_GREATERTHAN);
    s.push_i64(0);
    s.push_op(OP_EQUALVERIFY);

    // check if all exits claimed (new_uncl == 0)
    s.push_op(OP_DUP);
    s.push_data(&0u64.to_le_bytes());
    s.push_op(OP_EQUAL);

    s.push_op(OP_IF);
    {
        // All exits claimed — no continuation output needed.
        s.push_op(OP_DROP); // drop new_uncl_8b
        s.push_op(OP_DROP); // drop new_root

        // Verify no covenant continuation outputs remain.
        s.push_op(OP_TXINPUTINDEX);
        s.push_op(OP_INPUTCOVENANTID);
        s.push_op(OP_COVOUTCOUNT);
        s.push_i64(0);
        s.push_op(OP_EQUALVERIFY);
    }
    s.push_op(OP_ELSE);
    {
        // Unclaimed exits remain — verify continuation at output 1.

        // Build unclaimed part: [0x08 || new_uncl_8b]
        // Use push_i64(8) to match ScriptBuilder's add_data(&[0x08]) canonicalization
        // (single-byte values 1-16 are encoded as OP_N opcodes).
        s.push_i64(8);
        s.push_op(OP_SWAP);
        s.push_op(OP_CAT);

        // Build root part: [OpData32 || new_root]
        s.push_op(OP_SWAP);
        s.push_data(&[0x20u8]);
        s.push_op(OP_SWAP);
        s.push_op(OP_CAT);

        // Concat -> new prefix (42B)
        s.push_op(OP_SWAP);
        s.push_op(OP_CAT);

        // Extract body+suffix from own sig_script
        s.push_op(OP_TXINPUTINDEX);
        s.push_op(OP_TXINPUTINDEX);
        s.push_op(OP_TXINPUTSCRIPTSIGLEN);
        s.push_i64(-redeem_script_len + PERM_REDEEM_PREFIX_LEN);
        s.push_op(OP_ADD);
        s.push_op(OP_TXINPUTINDEX);
        s.push_op(OP_TXINPUTSCRIPTSIGLEN);
        s.push_op(OP_TXINPUTSCRIPTSIGSUBSTR);
        s.push_op(OP_CAT);

        // Hash -> expected SPK
        emit_hash_redeem_to_spk(s);

        // Verify output 1 SPK matches
        s.push_i64(1);
        s.push_op(OP_TXOUTPUTSPK);
        s.push_op(OP_EQUALVERIFY);

        // Verify exactly one covenant continuation output
        s.push_op(OP_TXINPUTINDEX);
        s.push_op(OP_INPUTCOVENANTID);
        s.push_op(OP_COVOUTCOUNT);
        s.push_i64(1);
        s.push_op(OP_EQUALVERIFY);
    }
    s.push_op(OP_ENDIF);
}

/// Phase 10: Verify delegate input/output balance.
fn emit_verify_delegate_balance(s: &mut ScriptVec, max_delegate_inputs: usize) {
    let n = max_delegate_inputs;

    // enforce input_count <= N+2
    s.push_op(OP_TXINPUTCOUNT);
    s.push_i64((n + 2) as i64);
    s.push_op(OP_GREATERTHAN);
    s.push_i64(0);
    s.push_op(OP_EQUALVERIFY);

    // Build expected delegate P2SH SPK (37B) from covenant_id
    s.push_op(OP_TXINPUTINDEX);
    s.push_op(OP_INPUTCOVENANTID);
    s.push_data(&DELEGATE_SCRIPT_PREFIX);
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_data(&DELEGATE_SCRIPT_SUFFIX);
    s.push_op(OP_CAT);
    emit_hash_redeem_to_spk(s);
    s.push_op(OP_TOALTSTACK);

    // Sum delegate input amounts (unrolled i = 1..N)
    s.push_i64(0); // accum = 0
    for i in 1..=n {
        s.push_op(OP_TXINPUTCOUNT);
        s.push_i64((i + 1) as i64);
        s.push_op(OP_GREATERTHANOREQUAL);
        s.push_op(OP_IF);
        {
            s.push_i64(i as i64);
            s.push_op(OP_TXINPUTSPK);
            s.push_op(OP_FROMALTSTACK);
            s.push_op(OP_DUP);
            s.push_op(OP_TOALTSTACK);
            s.push_op(OP_EQUAL);
            s.push_op(OP_IF);
            {
                s.push_i64(i as i64);
                s.push_op(OP_TXINPUTAMOUNT);
                s.push_op(OP_ADD);
            }
            s.push_op(OP_ENDIF);
        }
        s.push_op(OP_ENDIF);
    }

    // Guard: input N+1 must NOT have delegate SPK
    s.push_op(OP_TXINPUTCOUNT);
    s.push_i64((n + 2) as i64);
    s.push_op(OP_GREATERTHANOREQUAL);
    s.push_op(OP_IF);
    {
        s.push_i64((n + 1) as i64);
        s.push_op(OP_TXINPUTSPK);
        s.push_op(OP_FROMALTSTACK);
        s.push_op(OP_DUP);
        s.push_op(OP_TOALTSTACK);
        s.push_op(OP_EQUAL);
        s.push_op(OP_FALSE);
        s.push_op(OP_EQUALVERIFY);
    }
    s.push_op(OP_ENDIF);

    // Compute expected_change = total_input - deduct; verify >= 0
    s.push_op(OP_SWAP);
    s.push_op(OP_SUB);
    s.push_op(OP_DUP);
    s.push_i64(0);
    s.push_op(OP_GREATERTHANOREQUAL);
    s.push_op(OP_VERIFY);

    // Delegate change index = 1 + CovOutCount
    s.push_op(OP_TXINPUTINDEX);
    s.push_op(OP_INPUTCOVENANTID);
    s.push_op(OP_COVOUTCOUNT);
    s.push_i64(1);
    s.push_op(OP_ADD);

    // Verify delegate change output if needed
    s.push_op(OP_OVER);
    s.push_i64(0);
    s.push_op(OP_GREATERTHAN);
    s.push_op(OP_IF);
    {
        // expected_change > 0: verify output[delegate_idx]
        s.push_op(OP_DUP);
        s.push_op(OP_TXOUTPUTSPK);
        s.push_op(OP_FROMALTSTACK);
        s.push_op(OP_EQUALVERIFY);
        s.push_op(OP_TXOUTPUTAMOUNT);
        s.push_op(OP_EQUALVERIFY);
    }
    s.push_op(OP_ELSE);
    {
        // expected_change == 0: clean up
        s.push_op(OP_DROP); // delegate_idx
        s.push_op(OP_DROP); // expected_change
        s.push_op(OP_FROMALTSTACK);
        s.push_op(OP_DROP); // expected_spk
    }
    s.push_op(OP_ENDIF);
}

// ─────────────────────────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────────────────────────

/// One Merkle walk step: combine current_hash with a sibling.
fn emit_merkle_step(s: &mut ScriptVec) {
    s.push_op(OP_SWAP);
    s.push_op(OP_IF);
    // dir == 1: Cat -> sib||current (already in order)
    s.push_op(OP_ELSE);
    s.push_op(OP_SWAP);
    // dir == 0: Cat -> current||sib
    s.push_op(OP_ENDIF);
    s.push_op(OP_CAT);
    s.push_data(b"PermBranch");
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_op(OP_SHA256);
}

/// Hash redeem script → P2SH SPK bytes (version-prefixed).
///
/// Produces `version(2B LE) || OpBlake2b || OpData32 || blake2b(redeem) || OpEqual`
fn emit_hash_redeem_to_spk(s: &mut ScriptVec) {
    s.push_op(OP_BLAKE2B);
    // TX_VERSION (0) as 2B LE, then OpBlake2b, OpData32
    s.push_data(&[0x00, 0x00, OP_BLAKE2B, 0x20]);
    s.push_op(OP_SWAP);
    s.push_op(OP_CAT);
    s.push_data(&[OP_EQUAL]);
    s.push_op(OP_CAT);
}
