//! Variable-time modular inversion via Lehmer's extended-GCD algorithm,
//! specialized for the 3072-bit MuHash element and fully stack-allocated.
//!
//! This is a *Euclidean* extended GCD (true quotients), not the binary
//! Bernstein-Yang "safegcd" divstep. The distinction matters: safegcd divides by
//! 2 each step, so its cofactors carry a `2^-62k` factor that must be paid down
//! with a full-width modular reduction every round, whereas a Euclidean GCD keeps
//! a single cofactor pair of clean integers that grow gradually (~`n/2` limbs on
//! average) and need only one sign fixup at the end.
//!
//! Outer loop ([`invert`]): each iteration extracts a 2x2 reduction matrix from
//! the aligned top *two* words of the two remainders (the half-GCD guess
//! [`hgcd2`]) and applies it to the full remainders ([`apply_matrix_xy`]) and to
//! the cofactor pair ([`apply_matrix_t`]), peeling ~62 bits per matrix (~50
//! matrices for a 3072-bit inverse). When the guess cannot confirm even one
//! quotient it falls back to a single exact Euclidean division step.
//!
//! The guess ([`hgcd2`]) works on the top two `u64` words: a quotient of 1 is
//! resolved by one two-word subtraction and a top-word compare, so a hardware
//! divide is needed only for `q >= 2`. A trailing half-limb refinement keeps
//! every matrix entry below `2^63`, so the matrix-apply accumulators never
//! overflow and carry no per-step guard.
//!
//! Sign convention: track only `(t0, t1)`, the cofactor of `value`, kept
//! non-negative and growing; flip a `swapped` flag on each Euclidean swap and
//! recover the answer's sign from it at exit.
//!
//! Algorithm: Knuth, TAOCP Vol. 2, sec. 4.5.2, Algorithm L. Adapted from the
//! MIT-licensed 256-bit implementation by Dean Little (Copyright (c) 2026 Dean
//! Little, <https://github.com/blueshift-gg/solana-secp256k1>, `src/lehmer.rs`),
//! generalized to 48 limbs with a two-word half-GCD guess. Uses native
//! `u128`/`i128` accumulators plus this crate's `Uint3072` for the rare
//! multi-limb-divisor fallback; no external dependency.

use crate::Uint3072;
use core::cmp::Ordering;

/// Diagnostics-only per-inverse operation counters (enabled with the
/// `lehmer-instrument` feature). Used by `examples/lehmer_stats.rs` to measure
/// the work distribution. No effect on the production path.
#[cfg(feature = "lehmer-instrument")]
pub mod instrument {
    use core::sync::atomic::AtomicU64;
    pub static MATRICES: AtomicU64 = AtomicU64::new(0);
    pub static FALLBACK_STEPS: AtomicU64 = AtomicU64::new(0);
    pub static DIVIDES: AtomicU64 = AtomicU64::new(0);
    pub static APPLY_XY_LIMBS: AtomicU64 = AtomicU64::new(0);
    pub static APPLY_T_LIMBS: AtomicU64 = AtomicU64::new(0);
    pub static SWAPS: AtomicU64 = AtomicU64::new(0);
    pub static INVERSES: AtomicU64 = AtomicU64::new(0);
    pub static GCD_DIV_NORM: AtomicU64 = AtomicU64::new(0);
}

macro_rules! count {
    ($name:ident += $v:expr) => {{
        #[cfg(feature = "lehmer-instrument")]
        instrument::$name.fetch_add($v, core::sync::atomic::Ordering::Relaxed);
    }};
}

/// Limb count of the operands (3072 bits in base 2^64).
const N: usize = 48;
/// Limb count for the cofactor buffers. The cofactor of `value` is bounded in
/// magnitude by the modulus (< 2^3072, i.e. <= `N` limbs); the extra headroom
/// absorbs carry-out during a matrix application and the multi-limb fallback.
const T: usize = N + 8;
// Matrix entries from `hgcd2` are below `2^63` by construction (the half-limb
// refinement discards the least significant half limb), so the `i128`/`u128`
// accumulators in the matrix applications cannot overflow
// (`a*x + b*y + carry < 2^128` with `a, b < 2^63` and `x, y < 2^64`).

/// Compute the multiplicative inverse of `value` modulo `modulus` (an odd
/// modulus, e.g. the MuHash prime), via Lehmer's extended GCD. `value`,
/// `modulus` and `out` are little-endian base-2^64 limbs (`N = 48` words), and
/// `value` must already be reduced (`value < modulus`).
///
/// Returns `true` and writes the inverse into `out` when
/// `gcd(value, modulus) == 1`; returns `false` (leaving `out` unspecified)
/// otherwise. A zero `value` yields `false`.
pub fn invert(value: [u64; N], modulus: [u64; N], out: &mut [u64; N]) -> bool {
    count!(INVERSES += 1);
    if is_zero(&value) {
        core::hint::cold_path();
        return false;
    }

    // Euclidean state on (x, y) with x >= y, plus the cofactor of `value`.
    // Invariant: x ≡ -t0·value (mod m) and y ≡ t1·value (mod m) when
    // `swapped == false`, and the roles flip with each swap. `x`, `y` are owned
    // working buffers (they are reduced in place).
    let mut x: [u64; N] = modulus;
    let mut y: [u64; N] = value;
    let mut x_len = top_len(&x);
    let mut y_len = top_len(&y);
    let mut t0 = [0u64; T];
    let mut t1 = [0u64; T];
    t1[0] = 1;
    let mut t0_len = 1usize; // 0 has conventional length 1
    let mut t1_len = 1usize;
    let mut swapped = false;

    // Multi-limb phase: run until y collapses to a single limb, then finish
    // with a u64 extended-Euclidean tail.
    while y_len > 1 {
        let (x_hi, y_hi) = highest_two_words_normalized(&x, &y, x_len);
        let guess = hgcd2((x_hi >> 64) as u64, x_hi as u64, (y_hi >> 64) as u64, y_hi as u64);

        if let Some((a, b, c, d)) = guess {
            count!(MATRICES += 1);
            (x_len, y_len) = apply_matrix_xy(&mut x, &mut y, x_len, a, b, c, d);
            apply_matrix_t(&mut t0, &mut t1, &mut t0_len, &mut t1_len, a, b, c, d);

            // A partial guess can leave x < y; restore x >= y, toggling the sign.
            if x_len < y_len || (x_len == y_len && cmp_prefix(&x, &y, x_len).is_lt()) {
                core::mem::swap(&mut x, &mut y);
                core::mem::swap(&mut x_len, &mut y_len);
                core::mem::swap(&mut t0, &mut t1);
                core::mem::swap(&mut t0_len, &mut t1_len);
                swapped = !swapped;
            }
        } else {
            // The guess almost always confirms a quotient, so this exact-division
            // path is cold; keep it out of the hot loop body.
            core::hint::cold_path();
            count!(FALLBACK_STEPS += 1);
            // Guess could not confirm a quotient: one exact Euclidean step.
            let (q, r) = div_rem_step(&x, &y);
            t_add_qmul(&mut t0, &mut t0_len, &q, &t1, t1_len);
            x = y;
            x_len = y_len;
            y = r;
            y_len = top_len(&y);
            core::mem::swap(&mut t0, &mut t1);
            core::mem::swap(&mut t0_len, &mut t1_len);
            swapped = !swapped;
        }
    }

    // Single-limb tail. `y` is now <= 1 limb; one u64 extended Euclidean step
    // replaces the remaining ~tens of `t_add_qmul` iterations.
    if y_len == 1 && y[0] != 0 {
        if x[1..N].iter().any(|&w| w != 0) {
            let (q, r) = div_rem_step(&x, &y);
            t_add_qmul(&mut t0, &mut t0_len, &q, &t1, t1_len);
            let old_y = y[0];
            x = [0u64; N];
            x[0] = old_y;
            y = r;
            core::mem::swap(&mut t0, &mut t1);
            core::mem::swap(&mut t0_len, &mut t1_len);
            swapped = !swapped;
        }

        let (g, cx, cy) = gcd_ext_u64(x[0], y[0]);

        // Fold the sign of the chosen cofactor into `swapped`, then combine
        // magnitudes: new_t0 = |cx|·t0 + |cy|·t1.
        if cx < 0 || (cx == 0 && cy > 0) {
            swapped = !swapped;
        }
        let mut new_t0 = [0u64; T];
        let mut new_t0_len = 1usize;
        add_mul_word(&mut new_t0, &mut new_t0_len, cx.unsigned_abs(), &t0, t0_len);
        add_mul_word(&mut new_t0, &mut new_t0_len, cy.unsigned_abs(), &t1, t1_len);
        t0 = new_t0;

        x = [0u64; N];
        x[0] = g;
        x_len = if g == 0 { 0 } else { 1 };
    }

    // For an odd prime modulus and `value` in [1, m), the gcd is 1.
    if !(x_len == 1 && x[0] == 1) {
        return false;
    }

    // x = 1 = ±t0·value (mod m). With `swapped`, the inverse is +t0; otherwise
    // it is -t0 ≡ m - t0. Reduce t0 into [0, m) first.
    let mut inv = reduce_mod(&t0, &modulus);
    if !swapped && !is_zero(&inv) {
        inv = sub_n(&modulus, &inv);
    }
    *out = inv;
    true
}

/// Two-word subtract `(h1·2^64 + l1) - (h2·2^64 + l2)`, returning `(hi, lo)`.
/// Callers guarantee the minuend is the larger, so no underflow off the top.
#[inline(always)]
fn sub2(h1: u64, l1: u64, h2: u64, l2: u64) -> (u64, u64) {
    let (lo, borrow) = l1.overflowing_sub(l2);
    (h1.wrapping_sub(h2).wrapping_sub(borrow as u64), lo)
}

/// Fold the next half limb up into a fresh full word, `(hi << 32) | (lo >> 32)`,
/// for the handoff from the main hgcd2 loop (working on top *words*) to the
/// half-limb refinement loop. `hi < 2^32` here, so the result fits a `u64`.
#[inline(always)]
fn fold_half(hi: u64, lo: u64) -> u64 {
    (hi << 32).wrapping_add(lo >> 32)
}

/// The half-GCD guess: from the aligned top *two* words of the remainders
/// (`(ah, al) >=~ (bh, bl)`), run Euclidean steps on the 128-bit approximation
/// and return the 2x2 reduction matrix in this module's `(a, b, c, d)`
/// convention, `(x', y') = (a·x - b·y, d·y - c·x)`. Returns `None` when no
/// quotient can be confirmed (the caller then takes one exact division step).
///
/// A quotient of 1 is resolved by one two-word subtraction and a top-word
/// compare, so [`gcd_div`] (the hardware divide) is reached only for `q >= 2`.
/// The trailing half-limb refinement (the `HALF_LIMIT_2` loop) keeps every
/// matrix entry below `2^63`, which is why the `apply_matrix_*` accumulators
/// need no per-step overflow guard. All matrix arithmetic is `wrapping_*`
/// (entries are `< 2^63`, so it never actually wraps) to avoid the workspace
/// `overflow-checks = true` panic pads.
#[inline]
fn hgcd2(mut ah: u64, mut al: u64, mut bh: u64, mut bl: u64) -> Option<(u64, u64, u64, u64)> {
    // Need at least two leading bits of headroom in both top words. The guess
    // almost always confirms a quotient, so every `return None` here is cold.
    if ah < 2 || bh < 2 {
        core::hint::cold_path();
        return None;
    }

    // Reduction matrix M = (m00 m01; m10 m11), accumulated by the same column
    // updates as the full-width apply; returned remapped to (a,b,c,d).
    let (mut m00, mut m01, mut m10, mut m11): (u64, u64, u64, u64);
    if ah > bh || (ah == bh && al > bl) {
        (ah, al) = sub2(ah, al, bh, bl); // a -= b
        if ah < 2 {
            return None;
        }
        (m01, m10) = (1, 0);
    } else {
        (bh, bl) = sub2(bh, bl, ah, al); // b -= a
        if bh < 2 {
            return None;
        }
        (m01, m10) = (0, 1);
    }
    (m00, m11) = (1, 1);

    const HALF: u32 = 32;
    const HALF_LIMIT_1: u64 = 1 << HALF;

    let mut subtract_a = ah < bh;
    let mut subtract_a1 = false;
    let mut done = false;
    loop {
        if subtract_a {
            subtract_a = false;
        } else {
            if ah == bh {
                done = true;
                break;
            }
            if ah < HALF_LIMIT_1 {
                // Top words collapsed to a half limb: fold in the next half from
                // the low words and finish in the refinement loop below.
                ah = fold_half(ah, al);
                bh = fold_half(bh, bl);
                break;
            }
            // Subtract a -= q·b (affects the second column of M).
            (ah, al) = sub2(ah, al, bh, bl);
            if ah < 2 {
                done = true;
                break;
            }
            if ah <= bh {
                m01 = m01.wrapping_add(m00); // q = 1: no divide
                m11 = m11.wrapping_add(m10);
            } else {
                let n = ((ah as u128) << 64) | al as u128;
                let d = ((bh as u128) << 64) | bl as u128;
                let (mut q, r) = gcd_div(n, d);
                ah = (r >> 64) as u64;
                al = r as u64;
                if ah < 2 {
                    m01 = m01.wrapping_add(q.wrapping_mul(m00)); // q correct, a too small
                    m11 = m11.wrapping_add(q.wrapping_mul(m10));
                    done = true;
                    break;
                }
                q = q.wrapping_add(1); // one subtraction already taken above
                m01 = m01.wrapping_add(q.wrapping_mul(m00));
                m11 = m11.wrapping_add(q.wrapping_mul(m10));
            }
        }
        if ah == bh {
            done = true;
            break;
        }
        if bh < HALF_LIMIT_1 {
            ah = fold_half(ah, al);
            bh = fold_half(bh, bl);
            subtract_a1 = true;
            break;
        }
        // Subtract b -= q·a (affects the first column of M).
        (bh, bl) = sub2(bh, bl, ah, al);
        if bh < 2 {
            done = true;
            break;
        }
        if bh <= ah {
            m00 = m00.wrapping_add(m01); // q = 1: no divide
            m10 = m10.wrapping_add(m11);
        } else {
            let n = ((bh as u128) << 64) | bl as u128;
            let d = ((ah as u128) << 64) | al as u128;
            let (mut q, r) = gcd_div(n, d);
            bh = (r >> 64) as u64;
            bl = r as u64;
            if bh < 2 {
                m00 = m00.wrapping_add(q.wrapping_mul(m01));
                m10 = m10.wrapping_add(q.wrapping_mul(m11));
                done = true;
                break;
            }
            q = q.wrapping_add(1);
            m00 = m00.wrapping_add(q.wrapping_mul(m01));
            m10 = m10.wrapping_add(q.wrapping_mul(m11));
        }
    }

    // Half-limb refinement: peel a bit more (single-word divides on the folded
    // top words) until |a - b| fits in one limb + 1 bit. This is what keeps the
    // matrix entries below 2^63, discarding the least significant half limb.
    if !done {
        const HALF_LIMIT_2: u64 = 1 << (HALF + 1);
        loop {
            if subtract_a1 {
                subtract_a1 = false;
            } else {
                ah = ah.wrapping_sub(bh);
                if ah < HALF_LIMIT_2 {
                    break;
                }
                if ah <= bh {
                    m01 = m01.wrapping_add(m00);
                    m11 = m11.wrapping_add(m10);
                } else {
                    let q = ah / bh;
                    ah %= bh;
                    if ah < HALF_LIMIT_2 {
                        m01 = m01.wrapping_add(q.wrapping_mul(m00));
                        m11 = m11.wrapping_add(q.wrapping_mul(m10));
                        break;
                    }
                    let q = q.wrapping_add(1);
                    m01 = m01.wrapping_add(q.wrapping_mul(m00));
                    m11 = m11.wrapping_add(q.wrapping_mul(m10));
                }
            }
            bh = bh.wrapping_sub(ah);
            if bh < HALF_LIMIT_2 {
                break;
            }
            if ah >= bh {
                m00 = m00.wrapping_add(m01);
                m10 = m10.wrapping_add(m11);
            } else {
                let q = bh / ah;
                bh %= ah;
                if bh < HALF_LIMIT_2 {
                    m00 = m00.wrapping_add(q.wrapping_mul(m01));
                    m10 = m10.wrapping_add(q.wrapping_mul(m11));
                    break;
                }
                let q = q.wrapping_add(1);
                m00 = m00.wrapping_add(q.wrapping_mul(m01));
                m10 = m10.wrapping_add(q.wrapping_mul(m11));
            }
        }
    }

    // Remap M to this module's apply convention:
    // x' = m11·x - m01·y, y' = m00·y - m10·x.
    Some((m11, m01, m10, m00))
}

/// `(q, r) = (n / d, n % d)` for 128-bit `n, d` with `d >= 2^64`, computed from
/// hardware `u64` divisions (via [`div_2by1`]) instead of the `u128 / u128`
/// software routine. The quotient is `< 2^64` since `n < 2^128 <= d * 2^64`.
#[inline(always)]
fn gcd_div(n: u128, d: u128) -> (u64, u128) {
    count!(DIVIDES += 1);
    let (n1, n0) = ((n >> 64) as u64, n as u64);
    let (mut d1, mut d0) = ((d >> 64) as u64, d as u64);
    debug_assert!(d1 != 0);
    let (q, r) = (n1 / d1, n1 % d1);
    if q > d1 {
        // Non-normalized divisor: when `d1`'s top word is small, the single-word
        // estimate `q = n1/d1` can exceed `d1`. Rare but genuinely reachable (a
        // b-step can drop the divisor below 2^32 before an a-step divides by it),
        // so this is required for correctness, not merely defensive; marked cold
        // because it is rare. Normalize `d1` to have its top bit set, then do one
        // 128/64 division.
        core::hint::cold_path();
        count!(GCD_DIV_NORM += 1);
        let c = d1.leading_zeros();
        let wc = 64 - c;
        let n2 = n1 >> wc;
        let n1n = (n1 << c) | (n0 >> wc);
        let n0n = n0 << c;
        d1 = (d1 << c) | (d0 >> wc);
        d0 <<= c;
        let (mut q, rem1) = div_2by1(n2, n1n, d1);
        let prod = (q as u128) * (d0 as u128);
        let (mut t1, mut t0) = ((prod >> 64) as u64, prod as u64);
        if t1 > rem1 || (t1 == rem1 && t0 > n0n) {
            q -= 1;
            let (nt0, br) = t0.overflowing_sub(d0);
            t0 = nt0;
            t1 = t1.wrapping_sub(d1).wrapping_sub(br as u64);
        }
        let (rr0, br) = n0n.overflowing_sub(t0);
        let rr1 = rem1.wrapping_sub(t1).wrapping_sub(br as u64);
        let rr = ((rr1 as u128) << 64) | rr0 as u128;
        (q, rr >> c) // undo normalization
    } else {
        let mut q = q;
        let prod = (q as u128).wrapping_mul(d0 as u128);
        let (mut t1, mut t0) = ((prod >> 64) as u64, prod as u64);
        if t1 > r || (t1 == r && t0 > n0) {
            q -= 1;
            let (nt0, br) = t0.overflowing_sub(d0);
            t0 = nt0;
            t1 = t1.wrapping_sub(d1).wrapping_sub(br as u64);
        }
        let (rr0, br) = n0.overflowing_sub(t0);
        let rr1 = r.wrapping_sub(t1).wrapping_sub(br as u64);
        (q, ((rr1 as u128) << 64) | rr0 as u128)
    }
}

/// Multiply-accumulate: `acc + m·w + carry`, returning `(low 64 bits, high 64
/// bits)`. The high word is the carry into the next limb. A single widening
/// `64x64->128` multiply plus an add-with-carry; the carry is one word.
#[inline(always)]
fn mac(acc: u64, m: u64, w: u64, carry: u64) -> (u64, u64) {
    let t = (m as u128).wrapping_mul(w as u128).wrapping_add(acc as u128).wrapping_add(carry as u128);
    (t as u64, (t >> 64) as u64)
}

/// Multiply-subtract-borrow: `acc - m·w - borrow`, returning `(low 64 bits,
/// borrow)`. The borrow is the magnitude subtracted off the next limb. It fits
/// in one word because callers pass `m < 2^63` (the [`hgcd2`] entry bound), so
/// `m·w + borrow < 2^127 + 2^64` and its high word stays below `2^63`.
#[inline(always)]
fn msb(acc: u64, m: u64, w: u64, borrow: u64) -> (u64, u64) {
    let sub = (m as u128).wrapping_mul(w as u128).wrapping_add(borrow as u128);
    let (lo, b) = acc.overflowing_sub(sub as u64);
    (lo, ((sub >> 64) as u64).wrapping_add(b as u64))
}

/// `(x, y) <- (a·x - b·y, d·y - c·x)`, over the `len` live limbs only (the
/// current larger length), returning the new `(x_len, y_len)`. Both results are
/// non-negative and fit in `len` limbs for matrices from [`hgcd2`], so
/// the limbs at `[len..N]` (already zero) stay zero. Tracking lengths here lets
/// passes shrink with the remainders instead of always touching all `N` limbs.
///
/// Each output uses a multiply-accumulate / multiply-subtract structure: a `mac`
/// carry chain for the additive term and an independent `msb` borrow chain for
/// the subtractive one, both single-word. Keeping the two chains independent
/// shortens the loop-carried dependency versus a fused signed-`i128` accumulator,
/// which would serialize a two-register carry plus a per-limb sign-extension
/// every step.
#[inline]
fn apply_matrix_xy(x: &mut [u64; N], y: &mut [u64; N], len: usize, a: u64, b: u64, c: u64, d: u64) -> (usize, usize) {
    // A single up-front bound: once the compiler knows `len <= N`, every `[i]`
    // with `i < len` below is provably in-bounds, so the per-limb bounds checks
    // collapse into this one branch and no `unsafe` is needed. `len <= N` always
    // holds here, so this never actually panics.
    assert!(len <= N, "apply_matrix_xy: len exceeds N");
    count!(APPLY_XY_LIMBS += len as u64);
    // x' = a·x - b·y: `carry_ax` accumulates a·x, `borrow_x` peels off b·y.
    let mut carry_ax: u64 = 0;
    let mut borrow_x: u64 = 0;
    // y' = d·y - c·x.
    let mut carry_dy: u64 = 0;
    let mut borrow_y: u64 = 0;

    for i in 0..len {
        let xi = x[i];
        let yi = y[i];

        let (px, ncx) = mac(0, a, xi, carry_ax);
        let (nx, nbx) = msb(px, b, yi, borrow_x);
        carry_ax = ncx;
        borrow_x = nbx;

        let (py, ncy) = mac(0, d, yi, carry_dy);
        let (ny, nby) = msb(py, c, xi, borrow_y);
        carry_dy = ncy;
        borrow_y = nby;

        x[i] = nx;
        y[i] = ny;
    }
    // Non-negative result fitting in `len` limbs: the positive carry-out exactly
    // cancels the negative borrow-out (both name the same high limb, which is 0).
    debug_assert_eq!(carry_ax, borrow_x, "apply_matrix_xy: x went negative or overflowed len limbs");
    debug_assert_eq!(carry_dy, borrow_y, "apply_matrix_xy: y went negative or overflowed len limbs");

    let nlx = x[..len].iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
    let nly = y[..len].iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
    (nlx, nly)
}

/// `(t0, t1) <- (a·t0 + b·t1, c·t0 + d·t1)`, growing by at most one limb.
/// All-positive (the cofactor matrix uses `+` signs), so `u128` accumulators
/// suffice. Lengths are tracked inline; the cell at `max_len` is always written
/// (carry or zero) to keep `t[len..] == 0` without a separate clearing pass.
#[allow(clippy::too_many_arguments)]
#[inline]
fn apply_matrix_t(t0: &mut [u64; T], t1: &mut [u64; T], t0_len: &mut usize, t1_len: &mut usize, a: u64, b: u64, c: u64, d: u64) {
    let max_len = (*t0_len).max(*t1_len);
    count!(APPLY_T_LIMBS += max_len as u64);
    // Single up-front bound; see `apply_matrix_xy`. With `max_len < T` known, the
    // body `[i]` (i < max_len), the carry write `[max_len]`, and the length scan
    // (`max_len + 1 <= T`) are all provably in-bounds, collapsing the per-limb
    // checks into this one branch. The cofactor magnitude is bounded by the
    // modulus with `T = N + 8` limbs of headroom, so `max_len < T` always holds.
    assert!(max_len < T, "cofactor exceeded T limbs");
    let (a, b, c, d) = (a as u128, b as u128, c as u128, d as u128);

    let mut c0: u128 = 0;
    let mut c1: u128 = 0;

    for i in 0..max_len {
        let ti0 = t0[i] as u128;
        let ti1 = t1[i] as u128;

        let p_at0 = a.wrapping_mul(ti0);
        let p_bt1 = b.wrapping_mul(ti1);
        let p_ct0 = c.wrapping_mul(ti0);
        let p_dt1 = d.wrapping_mul(ti1);

        let n0 = c0.wrapping_add(p_at0).wrapping_add(p_bt1);
        let n1 = c1.wrapping_add(p_ct0).wrapping_add(p_dt1);

        t0[i] = n0 as u64;
        t1[i] = n1 as u64;
        c0 = n0 >> 64;
        c1 = n1 >> 64;
    }
    // Carry-out (provably < 2^64); also clears any stale cell at `max_len`.
    t0[max_len] = c0 as u64;
    t1[max_len] = c1 as u64;

    let nl0 = t0[..max_len + 1].iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
    let nl1 = t1[..max_len + 1].iter().rposition(|&w| w != 0).map_or(0, |i| i + 1);
    *t0_len = nl0.max(1);
    *t1_len = nl1.max(1);
}

/// `t0 += q · t1`, where `q` is up to `N` limbs (the exact Euclidean quotient).
fn t_add_qmul(t0: &mut [u64; T], t0_len: &mut usize, q: &[u64; N], t1: &[u64; T], t1_len: usize) {
    for (i, &qi) in q.iter().enumerate() {
        if qi == 0 {
            continue;
        }
        let qi = qi as u128;
        let mut carry: u64 = 0;
        for (j, &t1j) in t1.iter().enumerate().take(t1_len) {
            let k = i + j;
            if k >= T {
                debug_assert_eq!(carry, 0, "t_add_qmul: truncated nonzero limb");
                break;
            }
            let prod = qi.wrapping_mul(t1j as u128).wrapping_add(t0[k] as u128).wrapping_add(carry as u128);
            t0[k] = prod as u64;
            carry = (prod >> 64) as u64;
        }
        let mut k = i + t1_len;
        while carry > 0 && k < T {
            let (s, c) = t0[k].overflowing_add(carry);
            t0[k] = s;
            carry = c as u64;
            k += 1;
        }
        debug_assert!(carry == 0, "t_add_qmul: carry lost off the top");
    }
    *t0_len = top_len_t(t0);
}

/// `out += w · t` (multi-limb unsigned), used by the single-limb tail.
fn add_mul_word(out: &mut [u64; T], out_len: &mut usize, w: u64, t: &[u64; T], t_len: usize) {
    if w != 0 {
        let w = w as u128;
        let mut carry: u64 = 0;
        for j in 0..t_len {
            // `j < t_len <= T` always, but the bound also lets the compiler clamp
            // the trip count and skip the per-limb bounds check.
            if j >= T {
                break;
            }
            let prod = w.wrapping_mul(t[j] as u128).wrapping_add(out[j] as u128).wrapping_add(carry as u128);
            out[j] = prod as u64;
            carry = (prod >> 64) as u64;
        }
        let mut k = t_len;
        while carry > 0 && k < T {
            let (s, c) = out[k].overflowing_add(carry);
            out[k] = s;
            carry = c as u64;
            k += 1;
        }
        debug_assert!(carry == 0, "add_mul_word: carry lost off the top");
    }
    *out_len = top_len_t(out);
}

/// `(q, r) = (x / y, x % y)`. Fast path for a single-limb divisor (the common
/// case once Lehmer has shrunk `y`); otherwise [`Uint3072::div_rem`].
fn div_rem_step(x: &[u64; N], y: &[u64; N]) -> ([u64; N], [u64; N]) {
    if y[1..N].iter().all(|&w| w == 0) {
        let (q, r0) = div_n_by_1(x, y[0]);
        let mut r_arr = [0u64; N];
        r_arr[0] = r0;
        return (q, r_arr);
    }
    let (q, r) = Uint3072(*x).div_rem(Uint3072(*y));
    (q.0, r.0)
}

/// `(q, r) = (x / d, x % d)` for a nonzero single-limb divisor `d` (the common
/// case once Lehmer has collapsed `y`). The divisor is constant across all `N`
/// limbs, so a reciprocal is computed once and each limb costs two multiplies
/// instead of a hardware divide. Precomputed-reciprocal division
/// (Moller-Granlund, 2011).
fn div_n_by_1(x: &[u64; N], d: u64) -> ([u64; N], u64) {
    debug_assert!(d != 0);
    let mut q = [0u64; N];
    let s = d.leading_zeros();
    if s == 0 {
        // `d` already has its top bit set: no normalization shift needed.
        let v = invert_limb(d);
        let mut r: u64 = 0;
        for i in (0..N).rev() {
            (q[i], r) = div_2by1_preinv(r, x[i], d, v);
        }
        (q, r)
    } else {
        // Normalize: divide `x << s` by `d << s` (same quotient); the bits shifted
        // off the top of `x` seed the running remainder, and `x % d = r >> s`.
        let d_norm = d << s;
        let v = invert_limb(d_norm);
        let mut r = x[N - 1] >> (64 - s);
        for i in (0..N).rev() {
            let lo = if i == 0 { 0 } else { x[i - 1] };
            let cur = (x[i] << s) | (lo >> (64 - s));
            (q[i], r) = div_2by1_preinv(r, cur, d_norm, v);
        }
        (q, r >> s)
    }
}

/// Reciprocal of a normalized 64-bit divisor `d` (top bit set):
/// `v = floor((2^128 - 1) / d) - 2^64`, the "3/2" reciprocal consumed by
/// [`div_2by1_preinv`] (Moller-Granlund, "Improved division by invariant
/// integers", 2011). The lone wide divide here runs once per [`div_n_by_1`]
/// call (amortized over all `N` limbs), so it is off the per-limb hot path.
#[inline]
const fn invert_limb(d: u64) -> u64 {
    debug_assert!(d >> 63 == 1, "invert_limb requires a normalized divisor");
    // v = floor((2^128 - 1) / d) - 2^64 = floor(((!d)·2^64 + !0) / d): subtracting
    // 2^64·d from the numerator drops the quotient by exactly 2^64, and that
    // numerator's high word `!d` is `< d` for a normalized `d`, so the quotient
    // fits a u64 and the software 128/64 [`div_2by1`] computes it -- avoiding the
    // slow `u128 / u128` (`__udivti3`).
    div_2by1(!d, u64::MAX, d).0
}

/// `(nh·2^64 + nl) / d -> (q, r)` via the precomputed reciprocal `di` from
/// [`invert_limb`]. Requires `d` normalized (top bit set) and `nh < d` (so the
/// quotient fits a `u64`); the returned `r` satisfies `r < d`, sustaining that
/// precondition limb to limb. One widening multiply, a branchless first
/// correction, and a rarely-taken second. All `wrapping_*` (the workspace builds
/// with `overflow-checks = true`).
#[inline(always)]
fn div_2by1_preinv(nh: u64, nl: u64, d: u64, di: u64) -> (u64, u64) {
    debug_assert!(d >> 63 == 1 && nh < d);
    // (qh:ql) = nh·di + (nh + 1)·2^64 + nl
    let prod = (nh as u128).wrapping_mul(di as u128);
    let (mut qh, ql) = ((prod >> 64) as u64, prod as u64);
    let (ql, carry) = ql.overflowing_add(nl);
    qh = qh.wrapping_add(nh.wrapping_add(1)).wrapping_add(carry as u64);

    let mut r = nl.wrapping_sub(qh.wrapping_mul(d));
    // First correction: if the estimate `qh` is one too high, `r` wraps above
    // `ql`; `mask` is all-ones then, folding the `-1`/`+d` fixup in branch-free.
    let mask = 0u64.wrapping_sub((r > ql) as u64);
    qh = qh.wrapping_add(mask);
    r = r.wrapping_add(mask & d);
    // Second correction (rare): a still-too-small quotient leaves `r >= d`.
    if r >= d {
        r = r.wrapping_sub(d);
        qh = qh.wrapping_add(1);
    }
    (qh, r)
}

/// `(hi·2^64 + lo) / d -> (q, r)`. Knuth Algorithm D specialized to 128/64.
/// Precondition: `d != 0` and `hi < d` (so the quotient fits in a `u64`).
///
/// Not on the per-limb hot path (that goes through [`div_n_by_1`]'s reciprocal
/// step, [`div_2by1_preinv`]): this generic divide is reached only once per
/// [`div_n_by_1`] (to seed the reciprocal, via [`invert_limb`]) and from
/// [`gcd_div`]'s rare normalization branch, so a plain software long division is
/// fine.
#[inline]
const fn div_2by1(hi: u64, lo: u64, d: u64) -> (u64, u64) {
    debug_assert!(d != 0);
    debug_assert!(hi < d, "div_2by1 quotient would overflow u64");

    let s = d.leading_zeros();
    let d = if s == 0 { d } else { d << s };
    let un32 = if s == 0 { hi } else { (hi << s) | (lo >> (64 - s)) };
    let un10 = if s == 0 { lo } else { lo << s };

    let vn1 = d >> 32;
    let vn0 = d & 0xFFFF_FFFF;
    let un1 = un10 >> 32;
    let un0 = un10 & 0xFFFF_FFFF;

    let mut q1 = un32 / vn1;
    let mut rhat = un32 - q1 * vn1;
    while q1 >= (1u64 << 32) || q1 * vn0 > (rhat << 32) | un1 {
        q1 -= 1;
        rhat += vn1;
        if rhat >= (1u64 << 32) {
            break;
        }
    }

    let un21 = (un32 << 32).wrapping_add(un1).wrapping_sub(q1.wrapping_mul(d));
    let mut q0 = un21 / vn1;
    let mut rhat = un21 - q0 * vn1;
    while q0 >= (1u64 << 32) || q0 * vn0 > (rhat << 32) | un0 {
        q0 -= 1;
        rhat += vn1;
        if rhat >= (1u64 << 32) {
            break;
        }
    }

    let r = (un21 << 32).wrapping_add(un0).wrapping_sub(q0.wrapping_mul(d)) >> s;
    ((q1 << 32) | q0, r)
}

/// Extended Euclidean GCD of two `u64`s: `(gcd, cx, cy)` with `cx·x + cy·y = g`.
/// Called once per inversion, so `i128` intermediates are fine.
fn gcd_ext_u64(mut x: u64, mut y: u64) -> (u64, i64, i64) {
    let (mut a, mut b, mut c, mut d) = (1i128, 0i128, 0i128, 1i128);
    while y != 0 {
        let q = (x / y) as i128;
        let r = x.wrapping_sub((q as u64).wrapping_mul(y));
        let (nc, nd) = (a.wrapping_sub(q.wrapping_mul(c)), b.wrapping_sub(q.wrapping_mul(d)));
        a = c;
        b = d;
        c = nc;
        d = nd;
        x = y;
        y = r;
    }
    (x, a as i64, b as i64)
}

/// Reduce a cofactor (`<= N` significant limbs) into `[0, m)`. The cofactor is
/// bounded by `m` at loop exit, so a few subtractions suffice; the `div_rem`
/// branch is a defensive fallback.
fn reduce_mod(value: &[u64; T], m: &[u64; N]) -> [u64; N] {
    debug_assert!(value[N..T].iter().all(|&w| w == 0), "cofactor exceeded N limbs");
    let mut out = [0u64; N];
    out.copy_from_slice(&value[..N]);
    for _ in 0..8 {
        if cmp_n(&out, m).is_lt() {
            return out;
        }
        out = sub_n(&out, m);
    }
    (Uint3072(out) % Uint3072(*m)).0
}

/// Top 128-bit prefix of `x` and of `y`, both shifted left by the same amount so
/// the prefix of the larger operand `x` has its top bit set (the normalization
/// [`hgcd2`] expects). `y` is read at the same limb positions as `x`, so its
/// prefix is naturally the smaller. Requires `x` the larger operand and
/// `x_len >= 2`.
#[inline]
fn highest_two_words_normalized(x: &[u64; N], y: &[u64; N], x_len: usize) -> (u128, u128) {
    debug_assert!(x_len >= 2);
    let i = x_len - 1;
    let lz = x[i].leading_zeros();
    let lo_idx_ok = i >= 2;
    // Combine the limbs at positions (i, i-1, i-2) into the top 128 bits after a
    // left shift by `lz`. `arr[i]`'s `lz` leading zeros guarantee no overflow.
    let combine = |hi: u64, mid: u64, lo: u64| -> u128 {
        let base = ((hi as u128) << 64) | (mid as u128);
        if lz == 0 { base } else { (base << lz) | ((lo >> (64 - lz)) as u128) }
    };
    let x_lo = if lo_idx_ok { x[i - 2] } else { 0 };
    let y_lo = if lo_idx_ok { y[i - 2] } else { 0 };
    (combine(x[i], x[i - 1], x_lo), combine(y[i], y[i - 1], y_lo))
}

#[inline]
fn is_zero(x: &[u64; N]) -> bool {
    x.iter().all(|&w| w == 0)
}

#[inline]
fn top_len(x: &[u64; N]) -> usize {
    for i in (0..N).rev() {
        if x[i] != 0 {
            return i + 1;
        }
    }
    0
}

/// Significant length of a cofactor buffer; 0 maps to 1 by convention.
#[inline]
fn top_len_t(x: &[u64; T]) -> usize {
    for i in (0..T).rev() {
        if x[i] != 0 {
            return i + 1;
        }
    }
    1
}

#[inline]
fn sub_n(a: &[u64; N], b: &[u64; N]) -> [u64; N] {
    let mut out = [0u64; N];
    let mut borrow = 0u64;
    for i in 0..N {
        let (d1, b1) = a[i].overflowing_sub(b[i]);
        let (d2, b2) = d1.overflowing_sub(borrow);
        out[i] = d2;
        borrow = (b1 | b2) as u64;
    }
    out
}

#[inline]
fn cmp_n(a: &[u64; N], b: &[u64; N]) -> Ordering {
    for i in (0..N).rev() {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[inline]
fn cmp_prefix(a: &[u64; N], b: &[u64; N], n: usize) -> Ordering {
    for i in (0..n).rev() {
        match a[i].cmp(&b[i]) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Uint3072;
    use rand_chacha::{
        ChaCha8Rng,
        rand_core::{RngCore, SeedableRng},
    };

    // MuHash prime: 2^3072 - 1103717.
    const MUHASH_PRIME: Uint3072 = {
        let mut max = Uint3072::MAX;
        max.0[0] -= 1103717 - 1;
        max
    };

    fn lehmer_inv(value: [u64; N], modulus: [u64; N]) -> Option<[u64; N]> {
        let mut out = [0u64; N];
        invert(value, modulus, &mut out).then_some(out)
    }

    // Malachite reference oracle. The generic `Uint::mod_inverse` was removed, so this calls
    // malachite's `Natural::mod_inverse` directly (malachite is a dev-dependency, available here).
    fn malachite_inv(value: [u64; N], modulus: [u64; N]) -> Option<[u64; N]> {
        use malachite_base::num::arithmetic::traits::ModInverse;
        use malachite_nz::natural::Natural;
        let x = Natural::from_limbs_asc(&value);
        let m = Natural::from_limbs_asc(&modulus);
        x.mod_inverse(m).map(|inv| {
            let mut out = [0u64; N];
            let limbs = inv.into_limbs_asc();
            out[..limbs.len()].copy_from_slice(&limbs);
            out
        })
    }

    fn addmod(a: Uint3072, b: Uint3072, m: Uint3072) -> Uint3072 {
        let (res, overflow) = a.overflowing_add(b);
        if overflow || res >= m { res.overflowing_sub(m).0 } else { res }
    }

    /// `(a * b) mod m` via binary double-and-add (overflow-safe oracle).
    fn mulmod(a: Uint3072, b: Uint3072, m: Uint3072) -> Uint3072 {
        let mut result = Uint3072::ZERO;
        let mut base = a % m;
        let mut exp = b;
        while !exp.is_zero() {
            if exp.0[0] & 1 == 1 {
                result = addmod(result, base, m);
            }
            base = addmod(base, base, m);
            exp = exp >> 1;
        }
        result
    }

    #[test]
    fn matches_malachite_muhash_prime() {
        // Lehmer must agree bit-for-bit with malachite (the reference) on the MuHash prime.
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let mut buf = [0u8; Uint3072::BYTES];
        for _ in 0..2000 {
            rng.fill_bytes(&mut buf);
            let v = Uint3072::from_le_bytes(buf) % MUHASH_PRIME;
            if v.is_zero() {
                continue;
            }
            let expected = malachite_inv(v.0, MUHASH_PRIME.0).unwrap();
            let got = lehmer_inv(v.0, MUHASH_PRIME.0).unwrap();
            assert_eq!(got.as_slice(), expected.as_slice(), "v={v}");
        }
    }

    #[test]
    fn product_is_one_muhash_prime() {
        // v * inv(v) == 1 (mod prime), independent of any oracle.
        let mut rng = ChaCha8Rng::seed_from_u64(99);
        let mut buf = [0u8; Uint3072::BYTES];
        for _ in 0..500 {
            rng.fill_bytes(&mut buf);
            let v = Uint3072::from_le_bytes(buf) % MUHASH_PRIME;
            if v.is_zero() {
                continue;
            }
            let inv = lehmer_inv(v.0, MUHASH_PRIME.0).unwrap();
            assert_eq!(mulmod(v, Uint3072(inv), MUHASH_PRIME), Uint3072::from_u64(1), "v={v}");
        }
    }

    #[test]
    fn small_values_force_multilimb_quotient() {
        // Tiny `value` makes the first quotient ~48 limbs, exercising the
        // multi-limb `div_rem` fallback and the wide `t_add_qmul` path.
        for v_small in [1u64, 2, 3, 5, 7, 1103717, u64::MAX] {
            let v = Uint3072::from_u64(v_small);
            let inv = lehmer_inv(v.0, MUHASH_PRIME.0).unwrap();
            assert_eq!(mulmod(v, Uint3072(inv), MUHASH_PRIME), Uint3072::from_u64(1), "v={v_small}");
        }
    }

    #[test]
    fn edge_cases_muhash_prime() {
        let one = Uint3072::from_u64(1);
        let p_minus_1 = MUHASH_PRIME.overflowing_sub(one).0;
        assert_eq!(lehmer_inv(one.0, MUHASH_PRIME.0).unwrap().as_slice(), one.0.as_slice());
        assert_eq!(lehmer_inv(p_minus_1.0, MUHASH_PRIME.0).unwrap().as_slice(), p_minus_1.0.as_slice());
        assert!(lehmer_inv(Uint3072::ZERO.0, MUHASH_PRIME.0).is_none());
    }

    #[test]
    fn div_n_by_1_matches_uint_div() {
        // The reciprocal n-by-1 division must agree with Uint3072::div_rem across
        // tiny, normalized (top bit set), and arbitrary unnormalized divisors.
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let mut buf = [0u8; Uint3072::BYTES];
        for _ in 0..20000 {
            rng.fill_bytes(&mut buf);
            let x = Uint3072::from_le_bytes(buf);
            let d = match rng.next_u64() % 4 {
                0 => 1 + (rng.next_u64() % 1000),  // tiny (large shift)
                1 => rng.next_u64() | (1 << 63),   // already normalized
                2 => (rng.next_u64() >> 7).max(1), // unnormalized
                _ => rng.next_u64().max(1),        // arbitrary
            };
            let (q, r) = div_n_by_1(&x.0, d);
            let mut dl = [0u64; N];
            dl[0] = d;
            let (eq, er) = Uint3072(x.0).div_rem(Uint3072(dl));
            assert_eq!(q.as_slice(), eq.0.as_slice(), "quotient mismatch x={x} d={d}");
            assert_eq!(r, er.0[0], "remainder mismatch x={x} d={d}");
        }
        // Boundary divisors: 1, 2, MAX, 2^63 (min normalized), 2^63-1 (max
        // unnormalized), and the MuHash prime difference constant.
        for &d in &[1u64, 2, 3, u64::MAX, 1 << 63, (1 << 63) - 1, 1103717] {
            rng.fill_bytes(&mut buf);
            let x = Uint3072::from_le_bytes(buf);
            let (q, r) = div_n_by_1(&x.0, d);
            let mut dl = [0u64; N];
            dl[0] = d;
            let (eq, er) = Uint3072(x.0).div_rem(Uint3072(dl));
            assert_eq!(q.as_slice(), eq.0.as_slice(), "quotient mismatch x={x} d={d}");
            assert_eq!(r, er.0[0], "remainder mismatch x={x} d={d}");
        }
    }

    #[test]
    fn gcd_div_matches_u128_both_branches() {
        // `gcd_div` computes `(n/d, n%d)` for 128-bit `n, d` with `d >= 2^64`.
        // Cover both arms against the native `u128` oracle: the normal arm
        // (`q = n1/d1 <= d1`) and the rare normalization arm (`q > d1`), which
        // fires when the divisor's top word is small. The normalization arm is
        // hit only occasionally through `invert`, so pin it directly here.
        let oracle = |n: u128, d: u128| ((n / d) as u64, n % d);

        // Explicit cases that force `q > d1` (small divisor top word).
        let forcing: &[(u128, u128)] = &[
            (u128::MAX, (1u128 << 64) | 1), // d1 = 1
            (u128::MAX, 1u128 << 64),       // d = 2^64
            ((0xDEAD_BEEFu128 << 96) | 0x1234, (1u128 << 64) | u64::MAX as u128),
            (u128::MAX, (2u128 << 64) | 7),                     // d1 = 2
            (u128::MAX - 12345, (0x1_0000u128 << 64) | 0xABCD), // d1 = 2^16
        ];
        for &(n, d) in forcing {
            assert!((n >> 64) as u64 / (d >> 64) as u64 > (d >> 64) as u64, "case does not force q>d1");
            assert_eq!(gcd_div(n, d), oracle(n, d), "q>d1 arm mismatch n={n} d={d}");
        }
        // Normal arm (`q <= d1`): divisor with a large top word.
        for &(n, d) in &[(u128::MAX, (u64::MAX as u128) << 64 | 3), (1u128 << 127, (1u128 << 127) | 1)] {
            assert!((n >> 64) as u64 / (d >> 64) as u64 <= (d >> 64) as u64, "case is not the normal arm");
            assert_eq!(gcd_div(n, d), oracle(n, d), "normal arm mismatch n={n} d={d}");
        }

        // Randomized, biased to small divisor top words so most iterations take
        // the `q > d1` arm; every result checked bit-for-bit against `u128`.
        let mut rng = ChaCha8Rng::seed_from_u64(2024);
        let mut branch_hits = 0u64;
        for _ in 0..200_000 {
            let n = ((rng.next_u64() as u128) << 64) | rng.next_u64() as u128;
            let d1 = (rng.next_u64() % (1 << 20)) + 1; // 1 ..= 2^20
            let d = ((d1 as u128) << 64) | rng.next_u64() as u128;
            assert_eq!(gcd_div(n, d), oracle(n, d), "random gcd_div mismatch n={n} d={d}");
            if (n >> 64) as u64 / d1 > d1 {
                branch_hits += 1;
            }
        }
        assert!(branch_hits > 1000, "q>d1 arm under-exercised: only {branch_hits} hits");
    }

    #[test]
    fn q_gt_d1_vector_matches_malachite() {
        // A full-inverse input that drives `gcd_div` through its `q > d1`
        // normalization arm, pinned as an explicit regression case (that arm is
        // otherwise only hit occasionally by the random-vector tests).
        let v: [u64; N] = [
            0xbaac78a3d8d04a44,
            0x454126b8efd12383,
            0xa93bb055d701be60,
            0xe940f5627944ba89,
            0x68842c54edf3df88,
            0xf7c6332a4e2be869,
            0x396e8533e978070a,
            0x2703f08794977aad,
            0x373dc910525ad335,
            0x52520bee468a9073,
            0xf4580a27f3ca91ee,
            0x5de1060b3d65f732,
            0x4b1072d2e2cc0da1,
            0x4e2a032ba51f609a,
            0xeae5995402410005,
            0x885f9eb04a59ab3c,
            0x165446e913daa18e,
            0xf2b1311c3eb843c0,
            0x630b8232162a2fca,
            0xe4224b2c61bafd4e,
            0x25397d500e52b519,
            0x7cd55e35a4ac6022,
            0x8629892065e194c0,
            0x713ea3ded7bcd68c,
            0xf1f138b0052d1fdc,
            0x7974414d82958c31,
            0xc29243783567cf96,
            0xf9214af62a72ec1e,
            0xa1e4feb97fdbfe07,
            0x5c92a43f23a3989e,
            0x1ad829d92571a26a,
            0xd09dd31bf57bf618,
            0x00772f32162d2fd3,
            0x1c0dcedad1142715,
            0x677b857b2c9c2713,
            0xe64b41b0e187f9e7,
            0x4062f89c0309cd59,
            0x3edc673c30e10664,
            0x330b6c680b777302,
            0x423676df7dfd8ccf,
            0x347fe8b7dee8ca42,
            0x0fb4eb8629ef71c1,
            0x5e898abada4103ac,
            0x39820234bad59fb2,
            0xd780c6aeaaa89812,
            0x61bdb22ece416ba0,
            0x79f1821384ef3f44,
            0x803912aebf95eede,
        ];
        let got = lehmer_inv(v, MUHASH_PRIME.0).unwrap();
        assert_eq!(got.as_slice(), malachite_inv(v, MUHASH_PRIME.0).unwrap().as_slice());
        assert_eq!(mulmod(Uint3072(v), Uint3072(got), MUHASH_PRIME), Uint3072::from_u64(1));
    }

    #[test]
    fn hgcd2_none_when_top_word_below_two() {
        // The guess returns `None` (caller falls back to one exact division step)
        // exactly when a top word is `< 2`; otherwise it confirms a matrix.
        assert!(hgcd2(1, 0, 5, 0).is_none()); // ah < 2
        assert!(hgcd2(5, 0, 0, 9).is_none()); // bh < 2
        assert!(hgcd2(u64::MAX, 0, u64::MAX >> 1, 0).is_some()); // well-separated -> confirms a quotient
    }
}
