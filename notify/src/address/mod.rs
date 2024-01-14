pub mod hashing {
    use kaspa_addresses::Address;
    use kaspa_consensus_core::Hash;
    use kaspa_hashes::{AddressesHash, Hasher, HasherBase};

    pub fn hash(addresses: &[Address]) -> Hash {
        let mut hasher = AddressesHash::new();
        hasher.update((addresses.len() as u64).to_le_bytes());
        for address in addresses {
            write_address(&mut hasher, address);
        }
        hasher.finalize()
    }

    fn write_address<T: Hasher>(hasher: &mut T, address: &Address) {
        hasher.update((address.prefix as u8).to_le_bytes());
        hasher.update((address.version as u8).to_le_bytes());
        hasher.update(&address.payload);
    }
}

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
