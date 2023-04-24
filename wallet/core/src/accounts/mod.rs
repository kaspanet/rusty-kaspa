pub mod account;
pub mod gen0;
pub mod gen1;
use kaspa_addresses::{Address, Prefix as AddressPrefix, Version};

pub fn dummy_address() -> Address {
    Address::new(AddressPrefix::Mainnet, Version::PubKey, &[0u8; 32])
}

pub use account::*;
