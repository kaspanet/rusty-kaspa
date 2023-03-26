extern crate alloc;
extern crate self as kaspa_wallet_core;

pub mod account;
pub mod accounts;
pub mod error;
pub mod result;
pub mod signer;
pub mod storage;
pub mod utxo;
pub mod wallet;
pub mod wrapper;

pub use accounts::dummy_address;
pub use addresses::Address;
pub use result::Result;
pub use signer::Signer;
pub use wallet::Wallet;
pub use wrapper::WalletWrapper;

#[macro_export]
macro_rules! hex {
    ($str: literal) => {{
        let len = $str.as_bytes().len() / 2;
        let mut dst = vec![0; len];
        dst.resize(len, 0);
        faster_hex::hex_decode_fallback($str.as_bytes(), &mut dst);
        dst
    }
    [..]};
}
