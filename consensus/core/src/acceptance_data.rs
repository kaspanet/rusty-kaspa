use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::tx::TransactionId;

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
    pub index_within_block: u32,
}
