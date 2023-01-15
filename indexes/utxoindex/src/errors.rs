use crate::utxoindex::Signal;
use consensus::model::stores::errors::StoreError;
use thiserror::Error;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::error::RecvError::

#[derive(Error, Debug)]
pub enum UtxoIndexError {
    #[error("signal reciever error: {0}")]
    SignalRecieverDisconnecet(#[from] RecvError),

    #[error("consensus reciever error: {0}")]
    ConsensusReciverError(#[from] RecvError),

    #[error("utxoindex store error: {0}")]
    StoreError(#[from] StoreError),
}

pub trait StoreResultExtensions<T> {
    fn unwrap_option(self) -> Option<T>;
}

impl<T> StoreResultExtensions<T> for StoreResult<T> {
    fn unwrap_option(self) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(StoreError::KeyNotFound(_)) => None,
            Err(err) => panic!("Unexpected store error: {:?}", err),
        }
    }
}
