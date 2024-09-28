//!
//! Kaspa core wallet account variant implementations.
//!

pub mod bip32;
pub mod bip32watch;
pub mod keypair;
pub mod legacy;
pub mod multisig;
pub mod resident;

pub use bip32::BIP32_ACCOUNT_KIND;
pub use bip32watch::BIP32_WATCH_ACCOUNT_KIND;
pub use keypair::KEYPAIR_ACCOUNT_KIND;
pub use legacy::LEGACY_ACCOUNT_KIND;
pub use multisig::MULTISIG_ACCOUNT_KIND;
pub use resident::RESIDENT_ACCOUNT_KIND;
