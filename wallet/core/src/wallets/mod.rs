#[allow(dead_code)]
mod gen0;
#[allow(dead_code)]
mod gen1;
use addresses::{Address, Prefix as AddressPrefix, Version};

pub fn dummy_address() -> Address {
    Address::new(AddressPrefix::Mainnet, Version::PubKey, &[0u8; 32])
}

pub use gen0::*;
pub use gen1::*;
