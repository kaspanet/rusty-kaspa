use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

pub type AcceptanceData = Vec<MergeSetBlockAcceptanceData>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeSetBlockAcceptanceData {
    pub block_hash: Hash,
    pub accepted_transactions: Vec<Hash>,
}
