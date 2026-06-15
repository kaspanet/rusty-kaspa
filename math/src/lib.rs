use borsh::{BorshDeserialize, BorshSerialize};
use wasm_bindgen::JsValue;
use workflow_core::sendable::Sendable;

pub mod int;
pub mod lehmer;
pub mod uint;
pub mod wasm;

construct_uint!(Uint192, 3, BorshSerialize, BorshDeserialize);
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);
construct_uint!(Uint3072, 48);

/// Returns the ceiling of the base-2 logarithm of `x`, i.e. the smallest `k` such that `2^k >= x`.
///
/// # Panics
/// Panics if `x` is 0 (the base-2 logarithm of 0 is undefined).
#[inline]
pub const fn ceil_log_2(x: u64) -> u64 {
    // power of two -> floor; not a power of two -> floor + 1
    x.ilog2() as u64 + (!x.is_power_of_two()) as u64
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0:?}")]
    JsValue(Sendable<JsValue>),

    #[error("Invalid hex string: {0}")]
    Hex(#[from] faster_hex::Error),

    #[error(transparent)]
    TryFromSliceError(#[from] uint::TryFromSliceError),
    // TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("Utf8 error: {0}")]
    Utf8(#[from] std::str::Utf8Error),

    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error),

    #[error(transparent)]
    SerdeWasmBindgen(#[from] serde_wasm_bindgen::Error),

    #[error("{0:?}")]
    JsSys(Sendable<js_sys::Error>),

    #[error("Supplied value is not compatible with this type")]
    NotCompatible,

    #[error("range error: {0:?}")]
    Range(Sendable<js_sys::RangeError>),
}

impl From<js_sys::Error> for Error {
    fn from(err: js_sys::Error) -> Self {
        Error::JsSys(Sendable(err))
    }
}

impl From<js_sys::RangeError> for Error {
    fn from(err: js_sys::RangeError) -> Self {
        Error::Range(Sendable(err))
    }
}

impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        Error::JsValue(Sendable(err))
    }
}

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
        if mant > 0x7FFFFF { Uint256::ZERO } else { Uint256::from_u64(u64::from(mant)) << expt }
    }

    #[inline]
    /// Computes the target value in float format from BigInt format.
    pub fn compact_target_bits(self) -> u32 {
        let mut size = self.bits().div_ceil(8);
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
        compact | (size << 24)
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

impl TryFrom<Uint256> for Uint192 {
    type Error = crate::uint::TryFromIntError;

    #[inline]
    fn try_from(value: Uint256) -> Result<Self, Self::Error> {
        if value.0[3] != 0 {
            Err(crate::uint::TryFromIntError)
        } else {
            let mut result = Uint192::ZERO;
            result.0.copy_from_slice(&value.0[..3]);
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

#[cfg(test)]
mod ceil_log_2_tests {
    use crate::ceil_log_2;

    /// Independent reference: the smallest `k` such that `2^k >= x`. Computed in `u128` so it
    /// stays correct for `x` near `u64::MAX` (where the answer is 64).
    fn oracle(x: u64) -> u64 {
        assert!(x != 0);
        let mut k = 0u64;
        while (1u128 << k) < x as u128 {
            k += 1;
        }
        k
    }

    /// A spread of inputs exercising the dense low range, every power-of-2 boundary, and the top.
    fn sample_inputs() -> impl Iterator<Item = u64> {
        let dense = 1..=8192u64;
        let boundaries = (0..64u32).flat_map(|k| {
            let p = 1u64 << k;
            // p-1 (clamped away from 0), p, p+1
            [p.saturating_sub(1).max(1), p, p + 1]
        });
        dense.chain(boundaries).chain([u64::MAX])
    }

    #[test]
    fn known_values() {
        for (x, expected) in [
            (1u64, 0u64),
            (2, 1),
            (3, 2),
            (4, 2),
            (5, 3),
            (7, 3),
            (8, 3),
            (9, 4),
            (1023, 10),
            (1024, 10),
            (1025, 11),
            (1u64 << 63, 63),
            ((1u64 << 63) + 1, 64),
            (u64::MAX, 64),
        ] {
            assert_eq!(ceil_log_2(x), expected, "ceil_log_2({x})");
        }
    }

    #[test]
    fn correctness_matches_oracle() {
        for x in sample_inputs() {
            assert_eq!(ceil_log_2(x), oracle(x), "x={x}");
        }
    }

    #[test]
    fn compatibility_matches_malachite() {
        // Direct equivalence with the malachite `CeilingLogBase2` this replaced. malachite is a
        // dev-dependency only (the oracle); permanent coverage is `correctness_matches_oracle`.
        use malachite_base::num::arithmetic::traits::CeilingLogBase2;
        for x in sample_inputs() {
            assert_eq!(ceil_log_2(x), x.ceiling_log_base_2(), "x={x}");
        }
    }

    #[test]
    #[should_panic]
    fn panics_on_zero() {
        // Matches malachite's "Cannot take the base-2 logarithm of 0." panic.
        let _ = ceil_log_2(0);
    }
}
