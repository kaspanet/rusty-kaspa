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
pub const LIMBS: usize = crate::ELEMENT_BYTE_SIZE / LIMB_SIZE_BYTES;

pub const PRIME_DIFF: Limb = 1103717;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct U3072 {
    limbs: [Limb; LIMBS],
}
impl U3072 {
    #[inline(always)]
    pub const fn zero() -> Self {
        Self { limbs: [0; LIMBS] }
    }

    #[inline(always)]
    pub const fn one() -> Self {
        let mut s = Self::zero();
        s.limbs[0] = 1;
        s
    }

    #[inline(always)]
    #[must_use]
    pub fn is_overflow(&self) -> bool {
        // If the smallest limb is smaller than MAX-PRIME_DIFF then it is not overflown.
        if self.limbs[0] <= Limb::MAX - PRIME_DIFF {
            return false;
        }
        // If all other limbs == MAX it is overflown.
        self.limbs[1..].iter().all(|&limb| limb == Limb::MAX)
    }

    #[inline(always)]
    pub fn from_le_bytes(bytes: [u8; ELEMENT_BYTE_SIZE]) -> Self {
        let mut res = Self::zero();
        bytes.chunks_exact(LIMB_SIZE_BYTES).zip(res.limbs.iter_mut()).for_each(|(chunk, word)| {
            *word = Limb::from_le_bytes(chunk.try_into().unwrap());
        });
        res
    }

    #[inline(always)]
    #[must_use]
    pub fn to_le_bytes(self) -> [u8; ELEMENT_BYTE_SIZE] {
        let mut res = [0u8; ELEMENT_BYTE_SIZE];
        self.limbs.iter().zip(res.chunks_exact_mut(LIMB_SIZE_BYTES)).for_each(|(limb, chunk)| {
            chunk.copy_from_slice(&limb.to_le_bytes());
        });
        res
    }

    #[inline(always)]
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
                (d0, d1, d2) =
                    muladd3(self.limbs[(LIMBS - 1 - j) / 2 + j + 1], self.limbs[LIMBS - 1 - (LIMBS - 1 - j) / 2], d0, d1, d2);
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
    #[inline(always)]
    fn div_assign(&mut self, rhs: Self) {
        self.div(&rhs);
    }
}

impl MulAssign for U3072 {
    #[inline(always)]
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
    #[inline(always)]
    fn default() -> Self {
        Self::zero()
    }
}

#[cfg(test)]
mod tests {
    use crate::u3072::{self, Limb, LIMBS, PRIME_DIFF, U3072};
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha8Rng;
    use std::iter;

    #[test]
    fn test_mul() {
        struct TestVector {
            a: Limb,
            b: Limb,
            expected_low: Limb,
            expected_high: Limb,
        }
        let tests = [
            TestVector { a: Limb::MAX, b: Limb::MAX, expected_low: 1, expected_high: 18446744073709551614 },
            TestVector { a: Limb::MAX - 100, b: Limb::MAX - 30, expected_low: 3131, expected_high: 18446744073709551484 },
        ];

        for test in tests {
            let (low, high) = u3072::mul_wide(test.a, test.b);
            assert_eq!(low, test.expected_low);
            assert_eq!(high, test.expected_high);
        }
    }

    #[test]
    fn test_mulnadd3() {
        struct TestVector {
            c0: Limb,
            c1: Limb,
            d0: Limb,
            d1: Limb,
            d2: Limb,
            n: Limb,
            expected_c0: Limb,
            expected_c1: Limb,
            expected_c2: Limb,
        }
        let tests = [
            TestVector {
                c0: Limb::MAX - 99,
                c1: Limb::MAX - 75,
                d0: Limb::MAX - 30,
                d1: Limb::MAX - 3452,
                d2: 829429,
                n: 48569320,
                expected_c0: 18446744072203902596,
                expected_c1: 18446743906048258900,
                expected_c2: 40284851087600,
            },
            TestVector {
                c0: 0,
                c1: Limb::MAX - 32432432,
                d0: Limb::MAX - 534532431432423,
                d1: 1,
                d2: 342356341,
                n: 878998734,
                expected_c0: 3687790413486659920,
                expected_c1: 1725539564,
                expected_c2: 300930790315872295,
            },
        ];
        for test in tests {
            println!("a");
            let (c0, c1, c2) = u3072::mulnadd3(test.c0, test.c1, test.d0, test.d1, test.d2, test.n);
            assert_eq!(c0, test.expected_c0);
            assert_eq!(c1, test.expected_c1);
            assert_eq!(c2, test.expected_c2);
        }
    }

    #[test]
    fn test_muln2() {
        struct TestVector {
            low: Limb,
            high: Limb,
            n: Limb,
            expected_low: Limb,
            expected_high: Limb,
        }
        let tests = [
            TestVector { low: Limb::MAX - 99, high: Limb::MAX - 75, n: Limb::MAX - 543, expected_low: 54400, expected_high: 40700 },
            TestVector {
                low: 0,
                high: Limb::MAX - 32432432,
                n: Limb::MAX - 546546456543,
                expected_low: 0,
                expected_high: 17725831333250691552,
            },
        ];
        for test in tests {
            let (low, high) = u3072::muln2(test.low, test.high, test.n);
            assert_eq!(low, test.expected_low);
            assert_eq!(high, test.expected_high);
        }
    }

    #[test]
    fn test_muladd3() {
        struct TestVector {
            a: Limb,
            b: Limb,
            low: Limb,
            high: Limb,
            carry: Limb,
            expected_low: Limb,
            expected_high: Limb,
            expected_carry: Limb,
        }
        let tests = [
            TestVector {
                a: Limb::MAX - 30,
                b: Limb::MAX - 3452,
                low: Limb::MAX - 99,
                high: Limb::MAX - 75,
                carry: Limb::MAX - 100,
                expected_low: 106943,
                expected_high: 18446744073709548057,
                expected_carry: 18446744073709551516,
            },
            TestVector {
                a: Limb::MAX - 534543534534,
                b: 1,
                low: 0,
                high: Limb::MAX - 32432432,
                carry: Limb::MAX - 534532431432423,
                expected_low: 18446743539166017081,
                expected_high: 18446744073677119183,
                expected_carry: 18446209541278119192,
            },
        ];
        for test in tests {
            let (low, high, carry) = u3072::muladd3(test.a, test.b, test.low, test.high, test.carry);
            assert_eq!(low, test.expected_low);
            assert_eq!(high, test.expected_high);
            assert_eq!(carry, test.expected_carry);
        }
    }

    #[test]
    fn test_mul_double_add() {
        struct TestVector {
            a: Limb,
            b: Limb,
            low: Limb,
            high: Limb,
            carry: Limb,
            expected_low: Limb,
            expected_high: Limb,
            expected_carry: Limb,
        }
        let tests = [
            TestVector {
                a: Limb::MAX - 30,
                b: Limb::MAX - 3452,
                low: Limb::MAX - 99,
                high: Limb::MAX - 75,
                carry: Limb::MAX - 100,
                expected_low: 213986,
                expected_high: 18446744073709544573,
                expected_carry: 18446744073709551517,
            },
            TestVector {
                a: Limb::MAX - 534543534534,
                b: 1,
                low: 0,
                high: Limb::MAX - 32432432,
                carry: Limb::MAX - 534532431432423,
                expected_low: 18446743004622482546,
                expected_high: 18446744073677119184,
                expected_carry: 18446209541278119192,
            },
        ];
        for test in tests {
            let (low, high, carry) = u3072::mul_double_add(test.low, test.high, test.carry, test.a, test.b);
            assert_eq!(low, test.expected_low);
            assert_eq!(high, test.expected_high);
            assert_eq!(carry, test.expected_carry);
        }
    }

    #[test]
    fn test_inverse() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        for _ in 0..5 {
            let mut element = U3072::zero();
            rng.fill(&mut element.limbs[..]);
            let inv = element.inverse();
            let again = inv.inverse();
            assert_eq!(again, element);
            element.mul(&inv);
            assert_eq!(element, U3072::one());
        }
    }

    fn is_one(v: &U3072) -> bool {
        v.limbs[0] == 1 && v.limbs[1..].iter().all(|&l| l == 0)
    }

    #[test]
    fn test_div_overflow() {
        let max = U3072 { limbs: [Limb::MAX; LIMBS] };
        let one = U3072::one();
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        // Randomly test a bunch of overflown numbers to make sure they're handled correctly.
        // There are only 1,103,717 overflowing numbers, so when our inversion algorithm gets faster (egcd)) we can exhuastively check them.
        iter::once(0).chain(rand::seq::index::sample(&mut rng, u3072::PRIME_DIFF as usize, 64)).map(|i| i + 1).for_each(|i| {
            let overflown = {
                let mut overflown = max;
                overflown.limbs[0] = Limb::MAX - i as Limb + 1;
                overflown
            };
            {
                let mut overflown_copy = overflown;
                overflown_copy /= one;
                assert_eq!(overflown_copy.limbs[0], u3072::PRIME_DIFF - i as Limb);
                assert!(overflown_copy.limbs[1..].iter().all(|&x| x == 0));
            }

            // Zero doesn't have a modular inverse
            if i as Limb != PRIME_DIFF {
                let mut lhs = overflown;
                let rhs = overflown;
                lhs /= rhs;
                assert!(is_one(&lhs));
            }
        })
    }

    #[test]
    fn test_mul_max() {
        let mut max = U3072 { limbs: [Limb::MAX; LIMBS] };
        max.limbs[0] -= u3072::PRIME_DIFF;
        let copy_max = max;
        max *= copy_max;
        assert!(is_one(&max), "(p-1)*(p-1) mod p should equal 1");
    }

    #[test]
    fn test_mul_div() {
        const LOOPS: usize = 64;

        let mut rng = ChaCha8Rng::seed_from_u64(1);

        let list: Vec<_> = (0..LOOPS)
            .map(|_| {
                let mut element = U3072::zero();
                rng.fill(&mut element.limbs[..]);
                element
            })
            .collect();

        let mut start = U3072::one();
        for &elem in list.iter() {
            start *= elem;
        }
        assert!(!is_one(&start));

        for &elem in list.iter() {
            start /= elem;
        }
        assert!(is_one(&start));
    }

    #[test]
    fn test_inverse_edge_case() {
        #[rustfmt::skip]
        let orig = U3072 {
            limbs: [
                7122228832992001076, 984226626229791276, 7630161757215403889, 6284986028532537849, 8045609952094061025,
                11960578682873843289, 13746438324198032094, 13918942278011779234, 17733507388171786846, 10563242470999117317,
                17037155475664456442, 17937456968131788544, 12599342294785769540, 13386260146859547870, 2817582499516127913,
                652557987984108933, 9669847560665129471, 17711760030167214508, 5376140856964249866, 18051557786492143716,
                2482926987284881227, 8605482545261324676, 7878786448874819977, 1266815984192471985, 2678516262590404672,
                14004775981272003760, 10357003870690124643, 2730710396948079405, 4635754375072562978, 13656184258619915136,
                803512205739688286, 11844116904145642840, 5760653310472302601, 15069027324939031326, 14913021043324743434,
                17567013163360751106, 6302557725767759643, 17458497366820989801, 3410551217786514778, 14182717432968305815,
                12471950523812677269, 2294197765573979691, 3220941588656114052, 605606616684921311, 1440136155000853957,
                16361481774333736133, 11385241783616172231, 13968855456762740410,
            ],
        };
        let inv = orig.inverse();
        assert_eq!(inv.inverse(), orig);
    }
}
