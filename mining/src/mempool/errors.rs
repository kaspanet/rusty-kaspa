/// Re-export errors
pub use kaspa_mining_errors::mempool::*;

use crate::model::topological_index::TopologicalIndexError;

impl From<TopologicalIndexError> for RuleError {
    fn from(_: TopologicalIndexError) -> Self {
        RuleError::RejectCycleInMempoolTransactions
    }
}
