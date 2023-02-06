use async_channel::{RecvError, SendError};
use rocksdb::Error as DBError;
use std::io;
use thiserror::Error;

use consensus::model::stores::errors::StoreError;

use crate::notify::UtxoIndexNotification;

///Errors originating from the [`UtxoIndex`].
#[derive(Error, Debug)]
pub enum UtxoIndexError {
    #[error("[Utxoindex] store-error: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("[Utxoindex] database reset error: {0}")]
    DBResetError(#[from] io::Error),
}
