use kaspa_consensus_core::tx::TransactionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueScoreAcceptingRefData {
    /// The blue score of the block that accepted the transaction
    pub blue_score: u64,
    /// TxId found at the blue score
    pub transaction_id: TransactionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaaScoreIncludingRefData {
    /// The daa score of the block that includes the transaction
    pub daa_score: u64,
    /// TxId found at the daa score
    pub transaction_id: TransactionId,
}
