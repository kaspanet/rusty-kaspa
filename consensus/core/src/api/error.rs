use hashes::Hash;
use thiserror::Error;

use crate::tx::{TransactionId, TransactionOutpoint};

#[derive(Error, Debug)]
pub enum ConsensusError {
    #[error("block has missing parents: {0:?}")]
    BlockMissingParents(Vec<Hash>),

    #[error("invalid transactions in new block template")]
    InvalidTransactionsInNewBlock(Vec<(TransactionId, ConsensusError)>),

    #[error("outpoints corresponding to some transaction inputs are missing from current utxo context")]
    TxMissingOutpoints,

    #[error(
        "transaction input #{0} tried to spend coinbase outpoint {1} with daa score of {2} 
    while the merging block daa score is {3} and the coinbase maturity period of {4} hasn't passed yet"
    )]
    TxImmatureCoinbaseSpend(usize, TransactionOutpoint, u64, u64, u64),

    #[error("{0}")]
    General(String),
}
