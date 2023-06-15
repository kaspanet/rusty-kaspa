use serde::{Deserialize, Serialize};

pub type TransactionRecordId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub id: TransactionRecordId,
    // TODO
}
