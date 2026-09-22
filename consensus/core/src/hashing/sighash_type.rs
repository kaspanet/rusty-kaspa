use std::{fmt, ops::BitOr};

use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

pub const SIG_HASH_ALL: SigHashType = SigHashType(0b00000001);
pub const SIG_HASH_NONE: SigHashType = SigHashType(0b00000010);
pub const SIG_HASH_SINGLE: SigHashType = SigHashType(0b00000100);
pub const SIG_HASH_ANY_ONE_CAN_PAY: SigHashType = SigHashType(0b10000000);

/// SIG_HASH_MASK defines the number of bits of the hash type which are used
/// to identify which outputs are signed.
pub const SIG_HASH_MASK: u8 = 0b00000111;

const ALLOWED_SIG_HASH_TYPES_VALUES: [u8; 6] = [
    SIG_HASH_ALL.0,
    SIG_HASH_NONE.0,
    SIG_HASH_SINGLE.0,
    SIG_HASH_ALL.0 | SIG_HASH_ANY_ONE_CAN_PAY.0,
    SIG_HASH_NONE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0,
    SIG_HASH_SINGLE.0 | SIG_HASH_ANY_ONE_CAN_PAY.0,
];

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[wasm_bindgen]
pub struct SigHashType(pub(crate) u8);

impl SigHashType {
    pub fn is_sighash_all(self) -> bool {
        self.0 & SIG_HASH_MASK == SIG_HASH_ALL.0
    }

    pub fn is_sighash_none(self) -> bool {
        self.0 & SIG_HASH_MASK == SIG_HASH_NONE.0
    }

    pub fn is_sighash_single(self) -> bool {
        self.0 & SIG_HASH_MASK == SIG_HASH_SINGLE.0
    }

    pub fn is_sighash_anyone_can_pay(self) -> bool {
        self.0 & SIG_HASH_ANY_ONE_CAN_PAY.0 == SIG_HASH_ANY_ONE_CAN_PAY.0
    }

    pub fn to_u8(self) -> u8 {
        self.0
    }

    pub fn from_u8(val: u8) -> Result<Self, &'static str> {
        if !ALLOWED_SIG_HASH_TYPES_VALUES.contains(&val) {
            return Err("invalid sighash type");
        }

        Ok(Self(val))
    }
}

impl fmt::Display for SigHashType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self.0 {
            0b00000001 => "SIG_HASH_ALL",
            0b00000010 => "SIG_HASH_NONE",
            0b00000100 => "SIG_HASH_SINGLE",
            0b10000001 => "SIG_HASH_ALL | SIG_HASH_ANY_ONE_CAN_PAY",
            0b10000010 => "SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY",
            0b10000100 => "SIG_HASH_SINGLE | SIG_HASH_ANY_ONE_CAN_PAY",
            value => return write!(f, "UNKNOWN_SIG_HASH_TYPE (0x{value:02x})"),
        };

        f.write_str(name)
    }
}

impl BitOr for SigHashType {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        SigHashType(self.0 | rhs.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(SIG_HASH_ALL.to_string(), "SIG_HASH_ALL");
        assert_eq!(SIG_HASH_NONE.to_string(), "SIG_HASH_NONE");
        assert_eq!(SIG_HASH_SINGLE.to_string(), "SIG_HASH_SINGLE");
        assert_eq!((SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY).to_string(), "SIG_HASH_NONE | SIG_HASH_ANY_ONE_CAN_PAY");
    }
}
