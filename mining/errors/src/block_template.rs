use kaspa_consensus_core::errors::{block::RuleError, coinbase::CoinbaseError};
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum BuilderError {
    /// A consensus rule error
    #[error(transparent)]
    ConsensusError(#[from] RuleError),

    /// A coinbase error
    #[error(transparent)]
    CoinbaseError(#[from] CoinbaseError),
}

pub type BuilderResult<T> = std::result::Result<T, BuilderError>;
