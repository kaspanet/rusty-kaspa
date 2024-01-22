use kaspa_hashes::{Hash, HASH_SIZE};
use rand::{rngs::SmallRng, Rng};

pub fn generate_random_hash(rng: &mut SmallRng) -> Hash {
    let random_bytes = rng.gen::<[u8; HASH_SIZE]>();
    Hash::from_bytes(random_bytes)
}

pub fn generate_random_hashes(rng: &mut SmallRng, amount: usize) -> Vec<Hash> {
    let mut hashes = Vec::with_capacity(amount);
    let mut i = 0;
    while i < amount {
        hashes.push(generate_random_hash(rng));
        i += 1;
    }
    hashes
}
