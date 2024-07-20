use kaspa_wrpc_client::error::Error as RpcError;
use thiserror::Error;
use toml::de::Error as TomlError;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("RPC error: {0}")]
    Rpc(#[from] RpcError),

    #[error("TOML error: {0}")]
    Toml(#[from] TomlError),

    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),

    #[error("Connection Metrics")]
    ConnectionMetrics,
    #[error("Metrics")]
    Metrics,
    #[error("Sync")]
    Sync,
    #[error("Status")]
    Status,

    #[error("Channel send error")]
    ChannelSend,
    #[error("Channel try send error")]
    TryChannelSend,
}

impl Error {
    pub fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl<T> From<workflow_core::channel::SendError<T>> for Error {
    fn from(_: workflow_core::channel::SendError<T>) -> Self {
        Error::ChannelSend
    }
}

impl<T> From<workflow_core::channel::TrySendError<T>> for Error {
    fn from(_: workflow_core::channel::TrySendError<T>) -> Self {
        Error::TryChannelSend
    }
}
