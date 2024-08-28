use super::sighash_type::{self, SigHashType};
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

impl From<SighashType> for SigHashType {
    fn from(sighash_type: SighashType) -> SigHashType {
        match sighash_type {
            SighashType::All => sighash_type::SIG_HASH_ALL,
            SighashType::None => sighash_type::SIG_HASH_NONE,
            SighashType::Single => sighash_type::SIG_HASH_SINGLE,
            SighashType::AllAnyOneCanPay => sighash_type::SIG_HASH_ANY_ONE_CAN_PAY,
            SighashType::NoneAnyOneCanPay => SigHashType(sighash_type::SIG_HASH_NONE.0 | sighash_type::SIG_HASH_ANY_ONE_CAN_PAY.0),
            SighashType::SingleAnyOneCanPay => SigHashType(sighash_type::SIG_HASH_SINGLE.0 | sighash_type::SIG_HASH_ANY_ONE_CAN_PAY.0),
        }
    }
}
