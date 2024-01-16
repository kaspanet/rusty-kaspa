pub mod hashing;
pub mod tracker;

pub mod test_helpers {
    use kaspa_addresses::Address;
    use kaspa_addresses::{Prefix, Version};

    pub fn get_3_addresses(sorted: bool) -> Vec<Address> {
        let mut addresses = vec![
            Address::new(Prefix::Mainnet, Version::PubKey, &[1u8; 32]),
            Address::new(Prefix::Mainnet, Version::PubKey, &[2u8; 32]),
            Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]),
        ];
        if sorted {
            addresses.sort()
        }
        addresses
    }
}
