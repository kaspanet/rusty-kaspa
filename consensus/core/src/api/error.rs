use hashes::Hash;
use thiserror::Error;

use crate::tx::TransactionOutpoint;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum ConsensusError {
    #[error("block has missing parents: {0:?}")]
    BlockMissingParents(Vec<Hash>),

    #[error("one or more of the transaction inputs outpoint is not present in utxo context")]
    TxMissingOutpoints(Vec<TransactionOutpoint>),

    // #[error("{0}")]
    // BlockPermanentlyInvalid(String),

    // #[error("{0}")]
    // BlockCurrentlyRejected(String),
    #[error("{0}")]
    General(String),
}
