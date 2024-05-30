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

#[derive(Copy, Clone)]
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
