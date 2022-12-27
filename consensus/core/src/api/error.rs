use hashes::Hash;
use thiserror::Error;

use crate::tx::TransactionOutpoint;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ConsensusError {
    #[error("block has missing parents: {0:?}")]
    BlockMissingParents(Vec<Hash>),

    #[error("one or more of the transaction inputs outpoint is not present in utxo context")]
    TxMissingOutpoints(Vec<TransactionOutpoint>),

    #[error(
        "transaction input #{0} tried to spend coinbase outpoint {1} with daa score of {2} 
    while the merging block daa score is {3} and the coinbase maturity period of {4} hasn't passed yet"
    )]
    TxImmatureCoinbaseSpend(usize, TransactionOutpoint, u64, u64, u64),

    // #[error("{0}")]
    // BlockPermanentlyInvalid(String),

    // #[error("{0}")]
    // BlockCurrentlyRejected(String),
    #[error("{0}")]
    General(String),
}
