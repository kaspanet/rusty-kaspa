use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum ReturnAddressError {
    #[error("Transaction is already pruned")]
    AlreadyPruned,
    #[error("Transaction return address is coinbase")]
    TxFromCoinbase,
    #[error("Transaction not found at given accepting daa score")]
    NoTxAtScore,
    #[error("Transaction was found but not standard")]
    NonStandard,
    #[error("Transaction return address not found: {0}")]
    NotFound(String),
}
