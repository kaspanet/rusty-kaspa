use async_std::channel::{RecvError, SendError};
use consensus::model::stores::errors::StoreError;
use rocksdb::Error as RDBError;
use std::io;

use thiserror::Error;

use super::notify::UtxoIndexNotification;

#[derive(Error, Debug)]
pub enum UtxoIndexError {
    #[error("utxoindex error: {0}")]
    ConsensusRecieverUnreachableError(#[from] RecvError),

    #[error("utxoindex error: {0}")]
    StoreAccessError(#[from] StoreError),

    #[error("utxoindex error: {0}")]
    DBResetError(#[from] io::Error),

    #[error("utxoindex error: {0}")]
    DBDestroyError(#[from] RDBError),

    #[error("utxoindex error: {0}")]
    SendError(#[from] SendError<UtxoIndexNotification>),
}
