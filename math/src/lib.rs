pub mod int;
pub mod uint;
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);
construct_uint!(Uint3072, 48);

impl Uint256 {
    #[inline]
    pub fn from_compact_target_bits(bits: u32) -> Self {
        // This is a floating-point "compact" encoding originally used by
        // OpenSSL, which satoshi put into consensus code, so we're stuck
        // with it. The exponent needs to have 3 subtracted from it, hence
        // this goofy decoding code:
        let (mant, expt) = {
            let unshifted_expt = bits >> 24;
            if unshifted_expt <= 3 {
                ((bits & 0xFFFFFF) >> (8 * (3 - unshifted_expt)), 0)
            } else {
                (bits & 0xFFFFFF, 8 * ((bits >> 24) - 3))
            }
        };
        // The mantissa is signed but may not be negative
        if mant > 0x7FFFFF {
            Uint256::ZERO
        } else {
            Uint256::from_u64(u64::from(mant)) << expt
        }
    }

    #[inline]
    /// Computes the target value in float format from BigInt format.
    pub fn compact_target_bits(self) -> u32 {
        let mut size = (self.bits() + 7) / 8;
        let mut compact = if size <= 3 {
            (self.as_u64() << (8 * (3 - size))) as u32
        } else {
            let bn = self >> (8 * (size - 3));
            bn.as_u64() as u32
        };

        if (compact & 0x00800000) != 0 {
            compact >>= 8;
            size += 1;
        }
        compact | (size << 24) as u32
    }
}

impl From<Uint256> for Uint320 {
    #[inline]
    fn from(u: Uint256) -> Self {
        let mut result = Uint320::ZERO;
        result.0[..4].copy_from_slice(&u.0);
        result
    }
}

impl TryFrom<Uint320> for Uint256 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint320) -> Result<Self, Self::Error> {
        if value.0[4] != 0 {
            Err(crate::uint::TryFromIntError)
        } else {
            let mut result = Uint256::ZERO;
            result.0.copy_from_slice(&value.0[..4]);
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Uint256, Uint3072};

    #[test]
    fn test_overflow_bug() {
        let a = Uint256::from_le_bytes([
            255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255,
        ]);
        let b = Uint256::from_le_bytes([
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0,
            0, 32, 0, 0, 0,
        ]);
        let c = a.overflowing_add(b).0;
        let expected = [254, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 255, 255, 71, 33, 0, 0, 0, 0, 0, 0, 0, 32, 0, 0, 0];
        assert_eq!(c.to_le_bytes(), expected);
    }
    #[rustfmt::skip]
    #[test]
    fn div_rem_u3072_bug() {
        let r = Uint3072([
            18446744073708447899, 18446744069733351423, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073642442751, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
        ]);
        let newr = Uint3072([
            0, 3976200192, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 67108864, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        let expected = Uint3072([
            18446744073709551614, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 18446744073709551615, 18446744073709551615,
            18446744073709551615, 18446744073709551615, 274877906943, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        assert_eq!(r / newr, expected);
    }
}
