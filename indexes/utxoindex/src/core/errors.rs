use async_std::channel::{RecvError, SendError};
use rocksdb::Error as DBError;
use std::io;
use thiserror::Error;

use consensus::model::stores::errors::StoreError;

use crate::notify::UtxoIndexNotification;

///Errors originating from the [`UtxoIndex`].
#[derive(Error, Debug)]
pub enum UtxoIndexError {
    #[error("utxoindex error: {0}")]
    ConsensusRecieverUnreachableError(#[from] RecvError),

    #[error("utxoindex error: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("utxoindex error: {0}")]
    DBResetError(#[from] io::Error),

    #[error("utxoindex error: {0}")]
    DBDestroyError(#[from] DBError),

    #[error("utxoindex error: {0}")]
    SendError(#[from] SendError<UtxoIndexNotification>),
}
