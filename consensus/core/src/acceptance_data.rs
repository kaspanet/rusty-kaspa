use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::tx::TransactionId;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceData {
    pub mergeset: Vec<MergesetBlockAcceptanceData>,
    pub accepting_blue_score: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergesetBlockAcceptanceData {
    pub block_hash: Hash,
    pub accepted_transactions: Vec<AcceptedTxEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptedTxEntry {
    pub transaction_id: TransactionId,
    pub index_within_block: u32,
}
