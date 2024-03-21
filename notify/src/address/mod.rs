pub mod error;
pub mod tracker;

pub mod test_helpers {
    use kaspa_addresses::Address;
    use kaspa_addresses::{Prefix, Version};

    pub const ADDRESS_PREFIX: Prefix = Prefix::Mainnet;

    pub fn get_3_addresses(sorted: bool) -> Vec<Address> {
        let mut addresses = vec![
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[1u8; 32]),
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[2u8; 32]),
            Address::new(ADDRESS_PREFIX, Version::PubKey, &[0u8; 32]),
        ];
        if sorted {
            addresses.sort()
        }
        addresses
    }
}
