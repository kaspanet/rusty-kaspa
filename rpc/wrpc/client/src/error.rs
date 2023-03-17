use thiserror::Error;
use workflow_core::channel::ChannelError;
use workflow_rpc::client::error::Error as RpcError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("RpcApi error: {0}")]
    RpcApiError(#[from] kaspa_rpc_core::error::RpcError),

    #[error("Notification subsystem error: {0}")]
    NotificationError(#[from] kaspa_notify::error::Error),

    #[error("Channel error: {0}")]
    ChannelError(String),
}

impl From<Error> for kaspa_rpc_core::error::RpcError {
    fn from(err: Error) -> Self {
        err.to_string().into()
    }
}

impl<T> From<ChannelError<T>> for Error {
    fn from(err: ChannelError<T>) -> Self {
        Error::ChannelError(err.to_string())
    }
}
