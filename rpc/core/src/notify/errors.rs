use crate::RpcError;
use thiserror::Error;

pub type BoxedStdError = Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    #[error("Notification: channel receive error")]
    ChannelRecvError,

    #[error("Notification: channel send error")]
    ChannelSendError,
}

impl From<Error> for RpcError {
    fn from(value: Error) -> Self {
        RpcError::General(value.to_string())
    }
}

impl From<BoxedStdError> for Error {
    fn from(err: BoxedStdError) -> Self {
        Error::String(err.to_string())
    }
}

impl<T> From<async_std::channel::SendError<T>> for Error {
    fn from(_: async_std::channel::SendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl<T> From<async_std::channel::TrySendError<T>> for Error {
    fn from(_: async_std::channel::TrySendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl From<async_std::channel::RecvError> for Error {
    fn from(_: async_std::channel::RecvError) -> Self {
        Error::ChannelRecvError
    }
}
