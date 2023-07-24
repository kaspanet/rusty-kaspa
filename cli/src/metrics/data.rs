use borsh::{BorshDeserialize, BorshSerialize};
use kaspa_rpc_core::{ConsensusMetrics, ProcessMetrics};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
pub enum MetricsData {
    Noop,
    Tps(u64),
    ConsensusMetrics(ConsensusMetrics),
    ProcessMetrics(ProcessMetrics),
}
