use super::sighash_type::{self, SigHashType as SigHashTypeNative};
use wasm_bindgen::prelude::*;

/// Kaspa Sighash types allowed by consensus
/// @category Consensus
#[wasm_bindgen]
pub enum SighashType {
    All,
    None,
    Single,
    AllAnyOneCanPay,
    NoneAnyOneCanPay,
    SingleAnyOneCanPay,
}

impl From<SighashType> for SigHashTypeNative {
    fn from(sighash_type: SighashType) -> SigHashTypeNative {
        match sighash_type {
            SighashType::All => sighash_type::SIG_HASH_ALL,
            SighashType::None => sighash_type::SIG_HASH_NONE,
            SighashType::Single => sighash_type::SIG_HASH_SINGLE,
            SighashType::AllAnyOneCanPay => sighash_type::SIG_HASH_ALL | sighash_type::SIG_HASH_ANY_ONE_CAN_PAY,
            SighashType::NoneAnyOneCanPay => sighash_type::SIG_HASH_NONE | sighash_type::SIG_HASH_ANY_ONE_CAN_PAY,
            SighashType::SingleAnyOneCanPay => sighash_type::SIG_HASH_SINGLE | sighash_type::SIG_HASH_ANY_ONE_CAN_PAY,
        }
    }
}
