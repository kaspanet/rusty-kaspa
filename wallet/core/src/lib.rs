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
