#[allow(dead_code)]
mod gen0;
#[allow(dead_code)]
mod gen1;
use addresses::{Address, Prefix as AddressPrefix};

pub fn dummy_address() -> Address {
    Address {
        prefix: AddressPrefix::Mainnet,
        payload: vec![0u8; 32],
        version: 0u8,
    }
}

pub use gen0::*;
pub use gen1::*;
