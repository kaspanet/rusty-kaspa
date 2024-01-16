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
