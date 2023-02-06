use thiserror::Error;
use tokio::sync::mpsc::error::TrySendError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RpcApi error: {0}")]
    RpcApiError(#[from] kaspa_rpc_core::error::RpcError),

    #[error("Notification subsystem error: {0}")]
    NotificationError(#[from] kaspa_rpc_core::notify::error::Error),
}

impl From<Error> for kaspa_rpc_core::error::RpcError {
    fn from(err: Error) -> Self {
        match err {
            Error::RpcApiError(err) => err,
            Error::NotificationError(err) => err.into(),
        }
    }
}

impl From<Error> for kaspa_rpc_core::notify::error::Error {
    fn from(err: Error) -> Self {
        match err {
            Error::RpcApiError(err) => kaspa_rpc_core::notify::error::Error::General(err.to_string()),
            Error::NotificationError(err) => err,
        }
    }
}

impl<T> From<TrySendError<T>> for Error {
    fn from(_: TrySendError<T>) -> Self {
        kaspa_rpc_core::notify::error::Error::ChannelSendError.into()
    }
}

pub type Result<T> = std::result::Result<T, Error>;
