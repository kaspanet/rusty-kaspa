use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub struct MetricsData {
    pub blocks_submitted: u64,
    pub header_counts: u64,
    pub dep_counts: u64,
    pub body_counts: u64,
    pub txs_counts: u64,
    pub chain_block_counts: u64,
    pub mass_counts: u64,
    // ---
    pub block_count: u64,
    pub header_count: u64,
    pub tip_hashes: usize, //Vec<RpcHash>,
    pub difficulty: f64,
    pub past_median_time: u64,        // NOTE: i64 in gRPC protowire
    pub virtual_parent_hashes: usize, //Vec<RpcHash>,
    pub virtual_daa_score: u64,
    // Noop,
    // TestData(f32),
    // Tps(u64),
    // ConsensusMetrics(ConsensusMetrics),
    // ProcessMetrics(ProcessMetrics),
}
