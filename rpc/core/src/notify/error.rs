use crate::RpcError;
use async_channel::{RecvError, SendError, TrySendError};
use thiserror::Error;

pub type BoxedStdError = Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    General(String),

    #[error("Notification: channel receive error")]
    ChannelRecvError,

    #[error("Notification: channel send error")]
    ChannelSendError,

    #[error("object already stopped")]
    AlreadyStoppedError,
}

impl From<Error> for RpcError {
    fn from(value: Error) -> Self {
        RpcError::General(value.to_string())
    }
}

impl From<BoxedStdError> for Error {
    fn from(err: BoxedStdError) -> Self {
        Error::General(err.to_string())
    }
}

impl<T> From<SendError<T>> for Error {
    fn from(_: SendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl<T> From<TrySendError<T>> for Error {
    fn from(_: TrySendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl From<RecvError> for Error {
    fn from(_: RecvError) -> Self {
        Error::ChannelRecvError
    }
}
