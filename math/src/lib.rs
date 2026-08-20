#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use borsh::{BorshDeserialize, BorshSerialize};
#[cfg(feature = "wasm32-sdk")]
use wasm_bindgen::JsValue;
#[cfg(feature = "wasm32-sdk")]
use workflow_core::sendable::Sendable;

pub mod int;
pub mod uint;
#[cfg(feature = "wasm32-sdk")]
pub mod wasm;

construct_uint!(Uint192, 3, BorshSerialize, BorshDeserialize);
construct_uint!(Uint256, 4);
construct_uint!(Uint320, 5);
construct_uint!(Uint3072, 48);

// Explicit discriminants pin each variant's in-memory `#[repr(u8)]` tag so it
// stays fixed across feature configurations (std vs no_std, with/without
// `wasm32-sdk`), regardless of which cfg-gated variants are compiled in. This is
// in-memory repr only: the enum is never serialized (no serde/Borsh derive), and
// both would key off the variant name/positional index rather than this tag anyway.
#[derive(thiserror::Error, Debug)]
#[repr(u8)]
pub enum Error {
    #[cfg(feature = "wasm32-sdk")]
    #[error("{0:?}")]
    JsValue(Sendable<JsValue>) = 0,

    #[error("Invalid hex string: {0:?}")]
    Hex(faster_hex::Error) = 1,

    #[error(transparent)]
    TryFromSliceError(#[from] uint::TryFromSliceError) = 2,
    // TryFromSliceError(#[from] std::array::TryFromSliceError),
    #[error("Utf8 error: {0}")]
    Utf8(#[from] core::str::Utf8Error) = 3,

    #[cfg(feature = "wasm32-sdk")]
    #[error(transparent)]
    WorkflowWasm(#[from] workflow_wasm::error::Error) = 4,

    #[cfg(feature = "wasm32-sdk")]
    #[error(transparent)]
    SerdeWasmBindgen(#[from] serde_wasm_bindgen::Error) = 5,

    #[cfg(feature = "wasm32-sdk")]
    #[error("{0:?}")]
    JsSys(Sendable<js_sys::Error>) = 6,

    #[error("Supplied value is not compatible with this type")]
    NotCompatible = 7,

    #[cfg(feature = "wasm32-sdk")]
    #[error("range error: {0:?}")]
    Range(Sendable<js_sys::RangeError>) = 8,
}

#[cfg(feature = "wasm32-sdk")]
impl From<js_sys::Error> for Error {
    fn from(err: js_sys::Error) -> Self {
        Error::JsSys(Sendable(err))
    }
}

#[cfg(feature = "wasm32-sdk")]
impl From<js_sys::RangeError> for Error {
    fn from(err: js_sys::RangeError) -> Self {
        Error::Range(Sendable(err))
    }
}

#[cfg(feature = "wasm32-sdk")]
impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        Error::JsValue(Sendable(err))
    }
}

impl From<faster_hex::Error> for Error {
    fn from(err: faster_hex::Error) -> Self {
        Error::Hex(err)
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
