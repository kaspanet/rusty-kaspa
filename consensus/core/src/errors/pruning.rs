use super::{block::RuleError, tx::TxRuleError};
use kaspa_hashes::Hash;
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

    #[error("pruning import data lead to validation rule error")]
    PruningImportRuleError(#[from] RuleError),
}

pub type PruningImportResult<T> = std::result::Result<T, PruningImportError>;
