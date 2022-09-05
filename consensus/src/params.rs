use hashes::{Hash, HASH_SIZE};

use crate::model::stores::ghostdag::KType;

#[derive(Clone)]
pub struct Params {
    pub genesis_hash: Hash,
    pub ghostdag_k: KType,
    pub timestamp_deviation_tolerance: u64,
    pub target_time_per_block: u64,
    pub max_block_parents: u8,
    pub difficulty_window_size: usize,
    pub genesis_timestamp: u64,
    pub genesis_bits: u32,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
    pub finality_depth: u64,
}

const DEFAULT_GHOSTDAG_K: KType = 18;
pub const MAINNET_PARAMS: Params = Params {
    genesis_hash: Hash::from_bytes([1u8; HASH_SIZE]), // TODO: Use real mainnet genesis here
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: 132,
    target_time_per_block: 1000,
    max_block_parents: 10,
    difficulty_window_size: 2641,
    genesis_timestamp: 0,     // TODO: Use real value
    genesis_bits: 0x207fffff, // TODO: Use real value
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
};
