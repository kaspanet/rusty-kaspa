use kaspa_consensus_core::errors::consensus::ConsensusError;

use thiserror::Error;

use crate::IDENT;
use kaspa_database::prelude::StoreError;

/// Errors originating from the [`UtxoIndex`].
#[derive(Error, Debug)]
pub enum ScoreIndexError {
    #[error("[{IDENT}]: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("[{IDENT}]: {0}")]
    ConsensusQueryError(#[from] ConsensusError),

    #[error("[{IDENT}]: {0}")]
    RocksDBError(#[from] rocksdb::Error),
}

/// Results originating from the [`UtxoIndex`].
pub type ScoreIndexResult<T> = Result<T, ScoreIndexError>;
