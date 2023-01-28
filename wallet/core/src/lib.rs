pub mod error;
pub mod result;
pub mod wallet;
mod wallets;
mod wrapper;

pub use addresses::Address;
pub use result::Result;
pub use wallet::Wallet;
pub use wallets::dummy_address;
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
