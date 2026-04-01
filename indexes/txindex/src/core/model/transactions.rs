use kaspa_consensus_core::{Hash, acceptance_data::MergesetIndexType, tx::TransactionIndexType};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxInclusionData {
    /// The hash of the block that includes the transaction
    pub block_hash: Hash,
    /// This is the blue score of the block that includes the transaction
    pub daa_score: u64,
    /// The index within the block that this transaction occupies
    pub index_within_block: TransactionIndexType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TxAcceptanceData {
    /// The hash of the block that accepted the transaction
    pub block_hash: Hash,
    /// This is the blue score of the block that accepted the transaction
    pub blue_score: u64,
    /// The mergeset index to find the including block, from the pov of the the accepting block
    pub mergeset_index: MergesetIndexType,
}
