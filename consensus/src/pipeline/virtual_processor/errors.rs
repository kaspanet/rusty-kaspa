use thiserror::Error;

use crate::processes::transaction_validator::errors::TxRuleError;
use hashes::Hash;

#[derive(Error, Debug, Clone)]
pub enum VirtualProcessorError {
    #[error("new pruning point has an invalid transaction {0}: {1}")]
    NewPruningPointTxError(Hash, TxRuleError),

    #[error("new pruning point transaction {0} is missing a UTXO entry")]
    NewPruningPointTxMissingUTXOEntry(Hash),

    #[error("the imported multiset hash was expected to be {0} and was actually {1}")]
    ImportedMultisetHashMismatch(Hash, Hash),
}

pub type VirtualProcessorResult<T> = std::result::Result<T, VirtualProcessorError>;
