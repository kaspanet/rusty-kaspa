use std::io;
use thiserror::Error;

use crate::IDENT;
use kaspa_database::prelude::StoreError;

/// Errors originating from the [`TxIndex`](crate::TxIndex).
#[derive(Error, Debug)]
pub enum TxIndexError {
    #[error("[{IDENT}]: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("[{IDENT}]: {0}")]
    DBResetError(#[from] io::Error),
}

/// Results originating from the [`TxIndex`](crate::TxIndex).
pub type TxIndexResult<T> = Result<T, TxIndexError>;
