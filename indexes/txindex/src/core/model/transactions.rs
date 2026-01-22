use kaspa_consensus_core::{acceptance_data::MergesetIndexType, tx::TransactionIndexType, Hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxInclusionData {
    pub blue_score: u64,
    pub block_hash: Hash,
    pub index_within_block: TransactionIndexType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAcceptanceData {
    pub blue_score: u64,
    pub block_hash: Hash,
    pub mergeset_idx: MergesetIndexType,
}
