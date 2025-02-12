use std::time::Duration;

pub mod sync_rate_rule;

pub mod mining_rule;

pub struct ExtraData {
    pub finality_point_timestamp: u64,
    pub target_time_per_block: u64,
    pub has_sufficient_peer_connectivity: bool,
    pub finality_duration: u64,
    pub elapsed_time: Duration,
}
