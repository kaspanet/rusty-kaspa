macro_rules! construct_uint {
    ($name:ident, $n_words:literal) => {
        /// Little-endian large integer type
        #[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
        pub struct $name(pub [u64; $n_words]);

        impl $name {
            pub const ZERO: Self = $name([0; $n_words]);
            pub const MIN: Self = Self::ZERO;
            pub const MAX: Self = $name([u64::MAX; $n_words]);
            pub const BITS: u32 = $n_words * 64;

            #[inline]
            pub fn from_u128(n: u128) -> Self {
                let mut ret = Self::ZERO;
                ret.0[0] = n as u64;
                ret.0[1] = (n >> 64) as u64;
                ret
            }

            /// Return the least number of bits needed to represent the number
            #[inline]
            pub fn bits(&self) -> usize {
                let last_non_zero_index = self
                    .0
                    .iter()
                    .rev()
                    .position(|&w| w != 0)
                    .unwrap_or(0);
                (last_non_zero_index + 1) * 64 - self.0[last_non_zero_index].leading_zeros() as usize
            }

            #[inline]
            pub fn overflowing_shl(self, mut s: u32) -> ($name, bool) {
                let overflows = s >= Self::BITS;
                s &= Self::BITS - 1;
                let mut ret = [0u64; $n_words];
                let left_words = (s / 64) as usize;
                let left_shifts = s % 64;

                for i in left_words..$n_words {
                    ret[i] = self.0[i - left_words] << left_shifts;
                }
                if left_shifts > 0 {
                    let left_over = 64 - left_shifts;
                    for i in left_words + 1..$n_words {
                        ret[i] |= self.0[i - 1 - left_words] >> left_over;
                    }
                }
                (Self(ret), overflows)
            }

            #[inline]
            pub fn wrapping_shl(self, s: u32) -> $name {
                self.overflowing_shl(s).0
            }

            #[inline]
            pub fn overflowing_shr(mut self, mut s: u32) -> ($name, bool) {
                let overflows = s >= $n_words * 64;
                s &= Self::BITS - 1;
                if s == 0 {
                    return (self, overflows);
                }
                let left_words = (s / u64::BITS) as usize;
                let left_shifts = s % u64::BITS;
                let mut carry = 0;
                let left_over = u64::BITS - left_shifts;
                println!("s: {s}, left_words: {left_words}, left_shifts: {left_shifts}, left_over: {left_over}");
                for word in self.0.iter_mut() {
                    let tmp = *word;
                    *word = tmp.wrapping_shr(left_shifts) | carry;
                    carry = tmp.wrapping_shl(left_over);
                    println!("curr: {:064b}, new: {:064b}, carry: {:064b}", tmp, word, carry);
                }
                println!("after: {:064b}, {:064b}", self.0[0], self.0[1]);
                self.0[..$n_words - left_words].fill(0);
                (self, (carry != 0) | overflows)
            }

            #[inline]
            pub fn overflowing_add(mut self, other: $name) -> ($name, bool) {
                let mut carry = false;
                let mut carry_out;
                for i in 0..$n_words {
                    (self.0[i], carry_out) = self.0[i].overflowing_add(other.0[i]);
                    self.0[i] += carry as u64; // This cannot overflow as we are adding at most 2^64 - 1 to 2^64 - 1
                    carry = carry_out;
                }
                (self, carry)
            }

            #[inline]
            pub fn overflowing_sub(mut self, other: $name) -> ($name, bool) {
                let mut carry = false;
                let mut carry_out;
                for i in 0..$n_words {
                    (self.0[i], carry_out) = self.0[i].overflowing_sub(other.0[i]);
                    self.0[i] -= carry as u64; // This cannot overflow as we are subtracting at most 2^64 - 1 from 2^64 - 1
                    carry = carry_out;
                }
                (self, carry)
            }

            /// Multiplication by u64
            #[inline]
            pub fn overflowing_mul_u64(mut self, other: u64) -> ($name, bool) {
                let mut carry: u128 = 0;
                for i in 0..$n_words {
                    // TODO: Use `carrying_mul` when stabilized: https://github.com/rust-lang/rust/issues/85532
                    let n = carry + (other as u128) * (self.0[i] as u128);
                    self.0[i] = n as u64;
                    carry = (n >> 64) & u64::MAX as u128;
                }
                (self, carry != 0)
            }

            #[inline]
            pub fn overflowing_mul(self, other: $name) -> ($name, bool) {
                // We should probably replace this with a Montgomery multiplication algorithm
                let mut result = $name::ZERO;
                let mut carry_out = false;
                for j in 0..$n_words {
                    let mut carry = 0;
                    let mut i = 0;
                    while i + j < $n_words {
                        let n =
                            (self.0[i] as u128) * (other.0[j] as u128) + (result.0[i + j] as u128) + (carry as u128);
                        result.0[i + j] = n as u64;
                        carry = (n >> 64) as u64;
                        i += 1;
                    }
                    carry_out |= carry != 0;
                }
                (result, carry_out)
            }
            /// Creates big integer value from a byte slice using
            /// little-endian encoding
            #[inline(always)]
            pub fn from_le_bytes(bytes: [u8; $n_words * 8]) -> $name {
                let mut out = [0u64; $n_words];
                // This should optimize to basically a transmute.
                out.iter_mut()
                    .zip(bytes.chunks_exact(8))
                    .for_each(|(word, bytes)| *word = u64::from_le_bytes(bytes.try_into().unwrap()));
                Self(out)
            }

            /// Convert's the Uint into little endian byte array
            #[inline(always)]
            pub fn to_le_bytes(self) -> [u8; $n_words * 8] {
                let mut out = [0u8; $n_words * 8];
                // This should optimize to basically a transmute.
                out.chunks_exact_mut(8)
                    .zip(self.0)
                    .for_each(|(bytes, word)| bytes.copy_from_slice(&word.to_le_bytes()));
                out
            }

            // divmod like operation, returns (quotient, remainder)
            // #[inline]
            // fn div_rem(self, other: Self) -> (Self, Self) {
            //     let mut sub_copy = self;
            //     let mut shift_copy = other;
            //     let mut ret = [0u64; $n_words];
            //
            //     let my_bits = self.bits();
            //     let your_bits = other.bits();
            //
            //     // Check for division by 0
            //     assert!(your_bits != 0, "attempted to divide {} by zero", self);
            //
            //     // Early return in case we are dividing by a larger number than us
            //     if my_bits < your_bits {
            //         return ($name(ret), sub_copy);
            //     }
            //
            //     // Bitwise long division
            //     let mut shift = my_bits - your_bits;
            //     shift_copy = shift_copy << shift;
            //     loop {
            //         if sub_copy >= shift_copy {
            //             ret[shift / 64] |= 1 << (shift % 64);
            //             sub_copy = sub_copy - shift_copy;
            //         }
            //         shift_copy = shift_copy >> 1;
            //         if shift == 0 {
            //             break;
            //         }
            //         shift -= 1;
            //     }
            //
            //     ($name(ret), sub_copy)
            // }

            #[inline]
            pub fn iter_be_bits(self) -> impl ExactSizeIterator<Item = bool> + core::iter::FusedIterator {
                struct BinaryIterator {
                    array: [u64; $n_words],
                    word: usize,
                    bit: u32,
                }

                impl Iterator for BinaryIterator {
                    type Item = bool;

                    #[inline]
                    fn next(&mut self) -> Option<Self::Item> {
                        if self.bit == 64 {
                            self.word += 1;
                            self.bit = 0;
                        }
                        if self.word == $n_words {
                            return None;
                        }
                        let mut current_bit = self.array[$n_words - self.word - 1] & (1 << 64-self.bit-1);
                        self.bit += 1;
                        Some(current_bit != 0)
                    }

                    #[inline]
                    fn nth(&mut self, n: usize) -> Option<Self::Item> {
                        // TODO: add const assert that $n_words * 64 =< u32::MAX.
                        if n >= u32::MAX as usize {
                            return None;
                        }
                        // TODO: add const assert that usize::BITS >= u32::BITS.
                        let new_bit = self.bit + n as u32;
                        self.word += (new_bit / u64::BITS) as usize;
                        self.bit = new_bit % u64::BITS;
                        self.next()
                    }
                    #[inline]
                    fn size_hint(&self) -> (usize, Option<usize>) {
                        let remaining_bits = ($n_words - self.word) * 64 + (64 - self.bit as usize);
                        (remaining_bits, Some(remaining_bits))
                    }
                }
                impl ExactSizeIterator for BinaryIterator {}
                impl core::iter::FusedIterator for BinaryIterator {}

                BinaryIterator { array: self.0, word: 0, bit: 0 }
            }
        }

        impl PartialOrd for $name {
            #[inline]
            fn partial_cmp(&self, other: &$name) -> Option<core::cmp::Ordering> {
                Some(self.cmp(&other))
            }
        }

        impl Ord for $name {
            #[inline]
            fn cmp(&self, other: &$name) -> core::cmp::Ordering {
                // We need to manually implement ordering because we use little-endian
                // and the auto derive is a lexicographic ordering(i.e. memcmp)
                // which with numbers is equivalent to big-endian
                Iterator::cmp(self.0.iter().rev(), other.0.iter().rev())
            }
        }

        impl core::ops::Add<$name> for $name {
            type Output = $name;

            #[inline]
            fn add(self, other: $name) -> $name {
                let (sum, carry) = self.overflowing_add(other);
                debug_assert!(!carry, "attempt to add with overflow"); // Check in debug that it didn't overflow
                sum
            }
        }

        impl core::ops::Sub<$name> for $name {
            type Output = $name;

            #[inline]
            fn sub(self, other: $name) -> $name {
                let (sum, carry) = self.overflowing_sub(other);
                debug_assert!(!carry, "attempt to subtract with overflow"); // Check in debug that it didn't overflow
                sum
            }
        }

        impl core::ops::Mul<$name> for $name {
            type Output = $name;

            #[inline]
            fn mul(self, other: $name) -> $name {
                let (product, carry) = self.overflowing_mul(other);
                debug_assert!(!carry, "attempt to multiply with overflow"); // Check in debug that it didn't overflow
                product
            }
        }

        // impl core::ops::Div<$name> for $name {
        //     type Output = $name;
        //
        //     fn div(self, other: $name) -> $name {
        //         self.div_rem(other).0
        //     }
        // }

        // impl core::ops::Rem<$name> for $name {
        //     type Output = $name;
        //
        //     fn rem(self, other: $name) -> $name {
        //         self.div_rem(other).1
        //     }
        // }

        impl core::ops::BitAnd<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitand(mut self, other: $name) -> $name {
                self.0
                    .iter_mut()
                    .zip(other.0.iter())
                    .for_each(|(a, b)| *a &= *b);
                self
            }
        }

        impl core::ops::BitXor<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitxor(mut self, other: $name) -> $name {
                self.0
                    .iter_mut()
                    .zip(other.0.iter())
                    .for_each(|(a, b)| *a ^= *b);
                self
            }
        }

        impl core::ops::BitOr<$name> for $name {
            type Output = $name;

            #[inline]
            fn bitor(mut self, other: $name) -> $name {
                self.0
                    .iter_mut()
                    .zip(other.0.iter())
                    .for_each(|(a, b)| *a |= *b);
                self
            }
        }

        impl core::ops::Not for $name {
            type Output = $name;

            #[inline]
            fn not(mut self) -> $name {
                self.0.iter_mut().for_each(|a| *a = !*a);
                self
            }
        }

        impl core::ops::Shl<u32> for $name {
            type Output = $name;

            #[inline]
            fn shl(self, shift: u32) -> $name {
                let (res, carry) = self.overflowing_shl(shift);
                debug_assert!(!carry, "attempt to shift left with overflow"); // Check in debug that it didn't overflow
                res
            }
        }

        impl core::ops::Shr<u32> for $name {
            type Output = $name;

            #[inline]
            fn shr(self, shift: u32) -> $name {
                let (res, carry) = self.overflowing_shl(shift);
                debug_assert!(!carry, "attempt to shift left with overflow"); // Check in debug that it didn't overflow
                res
            }
        }

        impl core::fmt::LowerHex for $name {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                let mut hex = [0u8; $n_words * 8 * 2];
                let bytes = self.to_le_bytes();
                faster_hex::hex_encode(&bytes, &mut hex).expect("The output is exactly twice the size of the input");
                f.write_str(core::str::from_utf8(&hex).expect("hex is always valid UTF-8"))
            }
        }

        impl core::fmt::Binary for $name {
            #[inline]
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                const BIN_LEN: usize = $n_words * 64;
                let mut buf = [0u8; BIN_LEN];
                let mut first_one = BIN_LEN;
                for (index, (bit, char)) in self.iter_be_bits().zip(buf.iter_mut()).enumerate() {
                    *char = bit as u8 + b'0';
                    if first_one == BIN_LEN && bit {
                        first_one = index;
                    }
                }
                // We only wrote '0' and '1' so this is always valid UTF-8
                let buf_str = unsafe { std::str::from_utf8_unchecked(&buf[first_one..]) };
                f.pad_integral(true, "0b", buf_str)
            }
        }
    };
}
construct_uint!(Uint256, 4);
construct_uint!(Uint128, 2);
construct_uint!(Uint512, 8);
construct_uint!(Uint3072, 384);



#[cfg(test)]
mod tests {
    use super::{Uint128, Uint256, Uint3072};
    use rand_chacha::{
        rand_core::{RngCore, SeedableRng},
        ChaCha8Rng,
    };


    // #[test]
    // fn test_u256() {
    //         let mut rng = ChaCha8Rng::from_seed([0; 32]);
    //         let mut buf = [0u8; 32];
    //         for _ in 0..1_000_000 {
    //             rng.fill_bytes(&mut buf);
    //             let a = Uint256::from_le_bytes(buf);
    //             let b = U256Test::from_little_endian(&buf);
    //             rng.fill_bytes(&mut buf);
    //             let a2 = Uint256::from_le_bytes(buf);
    //             let b2 = U256Test::from_little_endian(&buf);
    //             assert_eq!(a.to_le_bytes(), le(b));
    //             assert_eq!(a2.to_le_bytes(), le(b2));
    //
    //             let a = a.overflowing_add(a2).0.overflowing_mul(a2).0;
    //             let b = b.overflowing_add(b2).0.overflowing_mul(b2).0;
    //             assert_eq!(a.to_le_bytes(), le(b));
    //             let shift = rng.next_u32() % 4096;
    //             let a_overflow_shl = a.overflowing_shl(shift);
    //             let b_overflow_shl = a.overflowing_shl(shift);
    //             assert_eq!(a_overflow_shl.1, b_overflow_shl.1);
    //             assert_eq!(a_overflow_shl.0.to_le_bytes(), b_overflow_shl.0.to_le_bytes());
    //             // println!("\nnum: {b}, shift: {shift}");
    //             // println!("\nmine:  {:0128b}", a);
    //             // println!("other: {:0128b}", b);
    //             // assert_eq!(a.overflowing_shr(shift).0.to_le_bytes(), le(b >> shift));
    //         }
    // }
    #[inline]
    fn shl_bitcoin<const N: usize>(mut original: [u64; N], shift: u32) -> [u64; N] {
        let a = original;
        original.fill(0);
        let k = (shift / 64) as usize;
        let shift = shift % 64;

        for i in 0..N {
            if i + k + 1 < N && shift != 0 {
                original[i + k + 1] |= (a[i] >> (64 - shift));
            }
            if i + k < N {
                original[i + k] |= (a[i] << shift);
            }
        }
        original
    }
    extern crate test;

    use test::{black_box, Bencher};
    #[bench]
    fn bench_u256(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 32];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint256::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 512);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(int.overflowing_shl(shift).0);
        });
    }

    #[bench]
    fn bench_u256_shl(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 32];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint256::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 512);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(shl(int.0, shift));
        });
    }

    #[bench]
    fn bench_u256_shl_bitcoin(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 32];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint256::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 512);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(shl_bitcoin(int.0, shift));
        });
    }

    #[bench]
    fn bench_u3072(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 3072];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint3072::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 6144);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(int.overflowing_shl(shift).0);
        });
    }

    #[bench]
    fn bench_u3072_shl(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 3072];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint3072::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 6144);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(shl(int.0, shift));
        });
    }

    #[bench]
    fn bench_u3072_shl_bitcoin(b: &mut Bencher) {
        let mut rng = ChaCha8Rng::from_seed([0; 32]);
        let mut buf = [0u8; 3072];
        let mut ints = vec![];
        let mut shifts = vec![];
        for _ in 0..100 {
            rng.fill_bytes(&mut buf);
            ints.push(Uint3072::from_le_bytes(buf));
            shifts.push(rng.next_u32() % 6144);
        }
        b.iter(|| for (&shift, &int) in shifts.iter().zip(ints.iter()) {
            black_box(shl_bitcoin(int.0, shift));
        });
    }


    #[test]
    fn test_u128() {
        let b = 92345481025148679904203189876114489134;
        let shift = 1514;
        let a = Uint128::from_u128(b);

        println!("before: {:0128b}", a);
        println!("before: {:0128b}", b);
        println!("num: {b}, shift: {shift}");
        let mine = a.overflowing_shr(shift).0;
        let other = b.overflowing_shr(shift).0;
        println!("\nmine:  {:0128b}", mine);
        println!("other: {:0128b}", other);
        println!("other: {:064b}, {:064b}", other as u64, (other >> 64) as u64);
        assert_eq!(a.overflowing_shr(shift).0.to_le_bytes(), b.overflowing_shr(shift).0.to_le_bytes());
    //     let mut rng = ChaCha8Rng::from_seed([0; 32]);
    //     let mut buf = [0u8; 16];
    //     for _ in 0..1_000_000 {
    //         rng.fill_bytes(&mut buf);
    //         let a = Uint128::from_le_bytes(buf);
    //         let b = u128::from_le_bytes(buf);
    //         rng.fill_bytes(&mut buf);
    //         let a2 = Uint128::from_le_bytes(buf);
    //         let b2 = u128::from_le_bytes(buf);
    //         assert_eq!(a.to_le_bytes(), b.to_le_bytes());
    //         assert_eq!(a2.to_le_bytes(), b2.to_le_bytes());
    //
    //         let a = a.overflowing_add(a2).0.overflowing_mul(a2).0;
    //         let b = b.overflowing_add(b2).0.overflowing_mul(b2).0;
    //         assert_eq!(a.to_le_bytes(), b.to_le_bytes());
    //         let shift = rng.next_u32() % 4096;
    //         let a_overflow_shl = a.overflowing_shl(shift);
    //         let b_overflow_shl = a.overflowing_shl(shift);
    //         assert_eq!(a_overflow_shl.1, b_overflow_shl.1);
    //         assert_eq!(a_overflow_shl.0.to_le_bytes(), b_overflow_shl.0.to_le_bytes());
    //         println!("\nnum: {b}, shift: {shift}");
    //         println!("\nmine:  {:0128b}", a);
    //         println!("other: {:0128b}", b);
    //         assert_eq!(a.overflowing_shr(shift).0.to_le_bytes(), b.overflowing_shr(shift).0.to_le_bytes());
    //     }
    }

    // extern crate test;
    // #[bench]
    // fn bench_u128_mul(bench: &mut test::Bencher) {
    //     let mut rng = ChaCha8Rng::from_seed([0; 32]);
    //     let mut buf = [0u8; 16];
    //     rng.fill_bytes(&mut buf);
    //     let mut a = u128::from_le_bytes(buf);
    //     let mut b = u128::from_le_bytes(buf);
    //
    //     bench.iter(|| {
    //         for _ in 0..10_000 {
    //             a = a.overflowing_mul(test::black_box(b)).0;
    //             test::black_box(format!("{a:x}"));
    //         }
    //     });
    //     test::black_box(a);
    //
    // }

    // #[bench]
    // fn bench_U128_mul(bench: &mut test::Bencher) {
    //     let mut rng = ChaCha8Rng::from_seed([0; 32]);
    //     let mut buf = [0u8; 16];
    //     rng.fill_bytes(&mut buf);
    //     let mut a = Uint128::from_le_bytes(buf);
    //     let mut b = Uint128::from_le_bytes(buf);
    //
    //     bench.iter(|| {
    //         for _ in 0..10_000 {
    //             a = a.overflowing_mul(test::black_box(b)).0;
    //             test::black_box(format!("{a:x}"));
    //         }
    //     });
    //     test::black_box(a);
    // }
}
