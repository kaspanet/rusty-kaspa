use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::tx::{TransactionId, TransactionIndexType};

// A mergeset currently cannot have more then 512 blocks.
pub type MergesetIndexType = u16;

/// Holds a mergeset acceptance data, a list of all its merged block with their accepted transactions
pub type AcceptanceData = Vec<MergesetBlockAcceptanceData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Holds a merged block with its accepted transactions
pub struct MergesetBlockAcceptanceData {
    pub block_hash: Hash,
    pub accepted_transactions: Vec<AcceptedTxEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedTxEntry {
    pub transaction_id: TransactionId,
    pub index_within_block: TransactionIndexType,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::ALL_PARAMS;

    #[test]
    fn test_mergeset_idx_does_not_overflow() {
        for param in ALL_PARAMS.iter() {
            // make sure that MergesetIdx can hold mergeset_size_limit
            MergesetIndexType::try_from(param.mergeset_size_limit()).unwrap();
        }
    }
}
