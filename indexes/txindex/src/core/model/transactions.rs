use kaspa_consensus_core::{acceptance_data::MergesetIndexType, tx::TransactionIndexType, Hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxInclusionData {
    /// This is the blue score of the block that includes the transaction
    pub blue_score: u64,
    /// The hash of the block that includes the transaction
    pub block_hash: Hash,
    /// The index within the block that this transaction occupies
    pub index_within_block: TransactionIndexType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxAcceptanceData {
    /// This is the blue score of the block that accepted the transaction
    pub blue_score: u64,
    /// The hash of the block that accepted the transaction
    pub block_hash: Hash,
    /// The mergeset index to find the including block, from the pov of the the accepting block
    pub mergeset_index: MergesetIndexType,
}
