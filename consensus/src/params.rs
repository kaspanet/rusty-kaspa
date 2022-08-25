use hashes::{Hash, HASH_SIZE};

#[derive(Clone)]
pub struct Params {
    pub genesis_hash: Hash,
    pub ghostdag_k: u8,
    pub timestamp_deviation_tolerance: u64,
    pub target_time_per_block: u64,
    pub max_block_parents: u8,
    pub difficulty_window_size: usize,
    pub genesis_timestamp: u64,
    pub genesis_bits: u32,
}

pub const MAINNET_PARAMS: Params = Params {
    genesis_hash: Hash::from_bytes([1u8; HASH_SIZE]), // TODO: Use real mainnet genesis here
    ghostdag_k: 18,
    timestamp_deviation_tolerance: 132,
    target_time_per_block: 1000,
    max_block_parents: 10,
    difficulty_window_size: 2641,
    genesis_timestamp: 0, // TODO: Use real value
    genesis_bits: 0,      // TODO: Use real value
};
