use kaspa_consensus_core::tx::TransactionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueScoreAcceptingRefData {
    /// The blue score of the block that accepted the transaction
    pub accepting_blue_score: u64,
    /// TxId found at the blue score
    pub tx_id: TransactionId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaaScoreIncludingRefData {
    /// The blue score of the block that includes the transaction
    pub including_daa_score: u64,
    /// TxId found at the blue score
    pub tx_id: TransactionId,
}
