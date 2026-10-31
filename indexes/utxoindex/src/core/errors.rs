use kaspa_index_core::indexed_utxos::UtxoPageCursor;
use std::io;
use thiserror::Error;

use crate::IDENT;
use kaspa_database::prelude::StoreError;

/// Errors originating from the [`UtxoIndex`](crate::UtxoIndex).
#[derive(Error, Debug)]
pub enum UtxoIndexError {
    #[error("[{IDENT}]: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("[{IDENT}]: {0}")]
    DBResetError(#[from] io::Error),

    #[error("[{IDENT}]: {0} is an invalid cursor for the given script public keys")]
    InvalidCursor(UtxoPageCursor),

    #[error("[{IDENT}]: The given script public keys set is empty")]
    QueryingEmptyAddressSet,
}

/// Results originating from the [`UtxoIndex`](crate::UtxoIndex).
pub type UtxoIndexResult<T> = Result<T, UtxoIndexError>;
