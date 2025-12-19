use kaspa_consensus_core::errors::consensus::ConsensusError;
use thiserror::Error;

use crate::IDENT;
use kaspa_database::prelude::StoreError;

/// Errors originating from the [`TxIndex`].
#[derive(Error, Debug)]
pub enum TxIndexError {
    #[error("[{IDENT}]: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("[{IDENT}]: {0}")]
    ConsensusQueryError(#[from] ConsensusError),

    #[error("[{IDENT}]: {0}")]
    RocksDBError(#[from] rocksdb::Error),
}

/// Results originating from the [`TxIndex`].
pub type TxIndexResult<T> = Result<T, TxIndexError>;
