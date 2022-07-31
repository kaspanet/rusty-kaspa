use crate::ELEMENT_BYTE_SIZE;
use std::ops::{DivAssign, MulAssign};

#[cfg(target_pointer_width = "64")]
pub(crate) type Limb = u64;
#[cfg(target_pointer_width = "64")]
pub(crate) type DoubleLimb = u128;

#[cfg(target_pointer_width = "32")]
pub(crate) type Limb = u32;
#[cfg(target_pointer_width = "32")]
pub(crate) type DoubleLimb = u64;

const LIMB_SIZE_BYTES: usize = std::mem::size_of::<Limb>();
const LIMB_SIZE: usize = std::mem::size_of::<Limb>() * 8;
pub(crate) const LIMBS: usize = crate::ELEMENT_BYTE_SIZE / LIMB_SIZE_BYTES;

pub(crate) const PRIME_DIFF: Limb = 1103717;

#[derive(Clone, Copy, Debug)]
pub(super) struct U3072 {
    limbs: [Limb; LIMBS],
}

impl U3072 {
    pub(crate) const MAX: Self = U3072 { limbs: [Limb::MAX; LIMBS] };

    #[inline(always)]
    pub(super) const fn zero() -> Self {
        Self { limbs: [0; LIMBS] }
    }

    #[inline(always)]
    pub(super) const fn one() -> Self {
        let mut s = Self::zero();
        s.limbs[0] = 1;
        s
    }
    #[inline]
    #[must_use]
    pub(super) fn is_overflow(&self) -> bool {
        // If the smallest limb is smaller than MAX-PRIME_DIFF then it is not overflown.
        if self.limbs[0] <= Limb::MAX - PRIME_DIFF {
            return false;
        }
        // If all other limbs == MAX it is overflown.
        self.limbs[1..]
            .iter()
            .all(|&limb| limb == Limb::MAX)
    }

    #[inline(always)]
    pub(super) fn from_le_bytes(bytes: [u8; ELEMENT_BYTE_SIZE]) -> Self {
        let mut res = Self::zero();
        bytes
            .chunks_exact(LIMB_SIZE_BYTES)
            .zip(res.limbs.iter_mut())
            .for_each(|(chunk, word)| {
                *word = Limb::from_le_bytes(chunk.try_into().unwrap());
            });
        res
    }

    #[inline(always)]
    #[must_use]
    pub(super) fn to_le_bytes(self) -> [u8; ELEMENT_BYTE_SIZE] {
        let mut res = [0u8; ELEMENT_BYTE_SIZE];
        self.limbs
            .iter()
            .zip(res.chunks_exact_mut(LIMB_SIZE_BYTES))
            .for_each(|(limb, chunk)| {
                chunk.copy_from_slice(&limb.to_le_bytes());
            });
        res
    }

    #[inline]
    fn full_reduce(&mut self) {
        let mut low = PRIME_DIFF;
        let mut high: Limb = 0;
        for limb in &mut self.limbs {
            let mut overflow;
            (low, overflow) = low.overflowing_add(*limb);
            (high, overflow) = high.overflowing_add(overflow as _);
            // Extract the result into self and shift the carries.
            (*limb, low, high) = (low, high, overflow as _);
        }
    }

    #[inline]
    fn mul(&mut self, other: &U3072) {
        let (mut carry_low, mut carry_high, mut carry_highest) = (0, 0, 0);
        let mut tmp = Self::one();

        // Compute limbs 0..N-2 of this*a into tmp, including one reduction.
        for j in 0..LIMBS - 1 {
            let (mut low, mut high) = mul_wide(self.limbs[j + 1], other.limbs[LIMBS + j - (1 + j)]);
            let mut carry = 0;
            for i in 2 + j..LIMBS {
                (low, high, carry) = muladd3(self.limbs[i], other.limbs[LIMBS + j - i], low, high, carry);
            }
            (carry_low, carry_high, carry_highest) = mulnadd3(carry_low, carry_high, low, high, carry, PRIME_DIFF);

            for i in 0..j + 1 {
                (carry_low, carry_high, carry_highest) =
                    muladd3(self.limbs[i], other.limbs[j - i], carry_low, carry_high, carry_highest);
            }

            // Extract the lowest limb of [low,high,carry] into n, and left shift the number by 1 limb
            (tmp.limbs[j], carry_low, carry_high, carry_highest) = (carry_low, carry_high, carry_highest, 0);
        }

        // Compute limb N-1 of a*b into tmp
        assert_eq!(carry_highest, 0);

        for i in 0..LIMBS {
            (carry_low, carry_high, carry_highest) =
                muladd3(self.limbs[i], other.limbs[LIMBS - 1 - i], carry_low, carry_high, carry_highest);
        }

        // Extract the lowest limb into temp and shift all the rest.
        (tmp.limbs[LIMBS - 1], carry_low, carry_high) = (carry_low, carry_high, carry_highest);

        // Perform a second reduction
        (carry_low, carry_high) = muln2(carry_low, carry_high, PRIME_DIFF);
        for i in 0..LIMBS {
            let mut overflow;
            (carry_low, overflow) = carry_low.overflowing_add(tmp.limbs[i]);
            (carry_high, overflow) = carry_high.overflowing_add(overflow as _);

            // Extract the result into self and shift the carries.
            (self.limbs[i], carry_low, carry_high) = (carry_low, carry_high, overflow as _);
        }
        assert_eq!(carry_high, 0);
        assert!(carry_low == 0 || carry_low == 1);
        //  Perform up to two more reductions if the internal state has already overflown the MAX of u3072
        //  or if it is larger than the modulus or if both are the case.

        if self.is_overflow() {
            self.full_reduce();
        }
        if carry_low != 0 {
            self.full_reduce();
        }
    }

    fn square(&mut self) {
        let (mut c0, mut c1, mut c2) = (0, 0, 0);
        let mut tmp = Self::zero();
        // Compute limbs 0..N-2 of this*this into tmp, including one reduction
        for j in 0..LIMBS - 1 {
            let (mut d0, mut d1, mut d2) = (0, 0, 0);
            for i in 0..(LIMBS - 1 - j) / 2 {
                (d0, d1, d2) = mul_double_add(d0, d1, d2, self.limbs[i + j + 1], self.limbs[LIMBS - 1 - i]);
            }
            if (j + 1) & 1 == 1 {
                (d0, d1, d2) = muladd3(
                    self.limbs[(LIMBS - 1 - j) / 2 + j + 1],
                    self.limbs[LIMBS - 1 - (LIMBS - 1 - j) / 2],
                    d0,
                    d1,
                    d2,
                );
            }
            (c0, c1, c2) = mulnadd3(c0, c1, d0, d1, d2, PRIME_DIFF);

            for i in 0..(j + 1) / 2 {
                (c0, c1, c2) = mul_double_add(c0, c1, c2, self.limbs[i], self.limbs[j - i]);
            }
            if (j + 1) & 1 == 1 {
                (c0, c1, c2) = muladd3(self.limbs[(j + 1) / 2], self.limbs[j - ((j + 1) / 2)], c0, c1, c2);
            }

            (tmp.limbs[j], c0, c1, c2) = (c0, c1, c2, 0);
        }

        assert_eq!(c2, 0);

        for i in 0..LIMBS / 2 {
            (c0, c1, c2) = mul_double_add(c0, c1, c2, self.limbs[i], self.limbs[LIMBS - 1 - i]);
        }

        (tmp.limbs[LIMBS - 1], c0, c1) = (c0, c1, c2);

        // Perform a second reduction
        (c0, c1) = muln2(c0, c1, PRIME_DIFF);
        for i in 0..LIMBS {
            let mut overflow;
            (c0, overflow) = c0.overflowing_add(tmp.limbs[i]);
            (c1, overflow) = c1.overflowing_add(overflow as _);
            // Extract the result into self and shift the carries.
            (self.limbs[i], c0, c1) = (c0, c1, overflow as _);
        }

        assert_eq!(c1, 0);
        assert!(c0 == 0 || c0 == 1);

        // Perform up to two more reductions if the internal state has already overflown the MAX of Num3072
        // or if it is larger than the modulus or if both are the case.
        if self.is_overflow() {
            self.full_reduce();
        }
        if c0 != 0 {
            self.full_reduce();
        }
    }

    #[inline(always)]
    fn square_and_multiply(&mut self, sequence: usize, mul: &Self) {
        for _ in 0..sequence {
            self.square();
        }
        self.mul(mul);
    }

    #[must_use]
    fn inverse(&self) -> Self {
        // TODO: Replace with a generic extended Euclidean algorithm.

        // For fast exponentiation a sliding window exponentiation with repunit
        // precomputation is utilized. See "Fast Point Decompression for Standard
        // Elliptic Curves" (Brumley, Järvinen, 2008).

        let mut p = [Self::zero(); 12]; // p[i] = a^(2^(2^i)-1)

        p[0] = *self;

        for i in 0..11 {
            p[i + 1] = p[i];
            for _ in 0..(1 << i) {
                p[i + 1].square();
            }

            // Due to the borrow checker we can't do `p[i + 1].mul(&p[i]);`
            // so instead we split the slice right in between so we can achieve the same without overhead.
            let (pi, pi1) = p.split_at_mut(i + 1);
            pi1[0].mul(&pi[i]);
        }

        let mut out = p[11];

        out.square_and_multiply(512, &p[9]);
        out.square_and_multiply(256, &p[8]);
        out.square_and_multiply(128, &p[7]);
        out.square_and_multiply(64, &p[6]);
        out.square_and_multiply(32, &p[5]);
        out.square_and_multiply(8, &p[3]);
        out.square_and_multiply(2, &p[1]);
        out.square_and_multiply(1, &p[0]);
        out.square_and_multiply(5, &p[2]);
        out.square_and_multiply(3, &p[0]);
        out.square_and_multiply(2, &p[0]);
        out.square_and_multiply(4, &p[0]);
        out.square_and_multiply(4, &p[1]);
        out.square_and_multiply(3, &p[0]);

        out
    }

    fn div(&mut self, other: &Self) {
        let inv = if other.is_overflow() {
            let mut new = *other;
            new.full_reduce();
            new.inverse()
        } else {
            other.inverse()
        };
        if self.is_overflow() {
            self.full_reduce();
        }

        self.mul(&inv);
        if self.is_overflow() {
            self.full_reduce();
        }
    }
}

impl DivAssign for U3072 {
    fn div_assign(&mut self, rhs: Self) {
        self.div(&rhs);
    }
}

impl MulAssign for U3072 {
    fn mul_assign(&mut self, rhs: Self) {
        self.mul(&rhs);
    }
}

#[inline(always)]
#[must_use]
// Input: [limb_0,limb_1,limb_2] Output: [limb_0,limb_1,limb_2] +=  2 * a * b
fn mul_double_add(limb_0: Limb, limb_1: Limb, mut limb_2: Limb, a: Limb, b: Limb) -> (Limb, Limb, Limb) {
    let (low, high) = mul_wide(a, b);

    let (limb_0, overflow) = limb_0.overflowing_add(low);
    let (limb_1, overflow) = limb_1.overflowing_add(high + overflow as Limb);
    limb_2 += overflow as Limb;

    let (limb_0, overflow) = limb_0.overflowing_add(low);
    let (limb_1, overflow) = limb_1.overflowing_add(high + overflow as Limb);
    limb_2 += overflow as Limb;

    (limb_0, limb_1, limb_2)
}

// TODO: Use https://github.com/rust-lang/rust/issues/85532 once stabilized.
#[inline(always)]
#[must_use]
fn mul_wide(a: Limb, b: Limb) -> (Limb, Limb) {
    let t = a as DoubleLimb * b as DoubleLimb;
    (t as Limb, (t >> LIMB_SIZE) as Limb)
}

/// Accepts a [c0, c1] integer, adds n * [d0, d1, d2] and returns the result including the carry
/// [c0,c1,c2] += n * [d0,d1,d2]. c2 is 0 initially
#[inline(always)]
#[must_use]
fn mulnadd3(c0: Limb, c1: Limb, d0: Limb, d1: Limb, d2: Limb, n: Limb) -> (Limb, Limb, Limb) {
    let mut t = d0 as DoubleLimb * n as DoubleLimb + c0 as DoubleLimb;
    let c0 = t as Limb;
    t >>= LIMB_SIZE;

    t += d1 as DoubleLimb * n as DoubleLimb + c1 as DoubleLimb;
    let c1 = t as Limb;
    t >>= LIMB_SIZE;
    let c2 = t as Limb + d2 * n;

    (c0, c1, c2)
}

/// accepts a,b and [low, high, carry] and returns a new [low, high, carry]
#[inline(always)]
#[must_use]
fn muladd3(a: Limb, b: Limb, low: Limb, high: Limb, mut carry: Limb) -> (Limb, Limb, Limb) {
    let (tl, mut th) = mul_wide(a, b);
    let (low, overflow) = low.overflowing_add(tl);
    th += overflow as Limb;
    let (high, overflow) = high.overflowing_add(th);
    carry += overflow as Limb;
    (low, high, carry)
}

/// [low,high] *= n and return [low, high]
#[inline(always)]
#[must_use]
fn muln2(low: Limb, high: Limb, n: Limb) -> (Limb, Limb) {
    let mut tmp = low as DoubleLimb * n as DoubleLimb;
    let low = tmp as Limb;

    tmp >>= LIMB_SIZE;
    tmp += high as DoubleLimb * n as DoubleLimb;

    (low, tmp as Limb)
}

impl Default for U3072 {
    fn default() -> Self {
        Self::zero()
    }
}
