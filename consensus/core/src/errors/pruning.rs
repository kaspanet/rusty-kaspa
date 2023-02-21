use super::tx::TxRuleError;
use hashes::Hash;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum PruningImportError {
    #[error("pruning proof validation failed")]
    ProofValidationError,

    #[error("new pruning point has an invalid transaction {0}: {1}")]
    NewPruningPointTxError(Hash, TxRuleError),

    #[error("new pruning point transaction {0} is missing a UTXO entry")]
    NewPruningPointTxMissingUTXOEntry(Hash),

    #[error("the imported multiset hash was expected to be {0} and was actually {1}")]
    ImportedMultisetHashMismatch(Hash, Hash),
}

pub type PruningImportResult<T> = std::result::Result<T, PruningImportError>;
