pub mod error;
pub mod result;
pub mod wallet;
mod wallets;
mod wrapper;

pub use result::Result;
pub use wrapper::WalletWrapper;
pub use addresses::Address;
pub use wallets::dummy_address;
pub use wallet::Wallet;
