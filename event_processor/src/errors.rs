use crate::notify::Notification;
use crate::IDENT;
use async_channel::{RecvError, SendError};
use thiserror::Error;
use utxoindex::errors::UtxoIndexError;

/// Errors originating from the [`EventProcessor`].
#[derive(Error, Debug)]
pub enum EventProcessorError {
    #[error("[{IDENT}]: {0}")]
    UtxoIndexError(#[from] UtxoIndexError),

    #[error("[{IDENT}]: {0}")]
    EventRecvError(#[from] RecvError),

    #[error("[{IDENT}]: {0}")]
    NotificationSendError(#[from] SendError<Notification>),
}

/// Results originating from the [`EventProcessor`].
pub type EventProcessorResult<T> = Result<T, EventProcessorError>;
