use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::tx::TransactionId;

pub type AcceptanceData = Vec<MergesetBlockAcceptanceData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergesetBlockAcceptanceData {
    pub block_hash: Hash,
    pub accepted_transactions: Vec<AcceptedTxEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptedTxEntry {
    pub transaction_id: TransactionId,
    pub index_within_block: u32,
}
