use async_channel::{RecvError, SendError, TrySendError};
use thiserror::Error;

pub type BoxedStdError = Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    General(String),

    #[error("channel receive error")]
    ChannelRecvError,

    #[error("channel send error")]
    ChannelSendError,

    #[error("object already stopped")]
    AlreadyStoppedError,

    #[error("connection closed")]
    ConnectionClosed,

    #[error("event type disabled")]
    EventTypeDisabled,
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

pub type Result<T> = std::result::Result<T, Error>;
