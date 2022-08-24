use hashes::{Hash, HASH_SIZE};

#[derive(Clone)]
pub struct Params {
    pub genesis_hash: Hash,
    pub ghostdag_k: u8,
    pub timestamp_deviation_tolerance: u64,
    pub target_time_per_block: u64,
    pub max_block_parents: u8,
    pub difficulty_window_size: usize,
}

pub const MAINNET_PARAMS: Params = Params {
    genesis_hash: Hash::from_bytes([1u8; HASH_SIZE]), // TODO: Use real mainnet genesis here
    ghostdag_k: 18,
    timestamp_deviation_tolerance: 132,
    target_time_per_block: 1000,
    max_block_parents: 10,
    difficulty_window_size: 2641,
};
