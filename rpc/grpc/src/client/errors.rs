use rpc_core::RpcError;
use thiserror::Error;

pub type BoxedStdError = Box<(dyn std::error::Error + Sync + std::marker::Send + 'static)>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    #[error("gRPC client error {0}")]
    TonicStatus(#[from] tonic::Status),

    /// RPC call timeout
    #[error("RPC request timeout")]
    Timeout,

    #[error("Endpoint connection error: {0}")]
    EndpointConnectionError(#[from] tonic::transport::Error),

    #[error("Notify error: {0}")]
    NotifyError(#[from] rpc_core::notify::errors::Error),

    #[error("RPC: channel receive error")]
    ChannelRecvError,

    #[error("RPC: channel send error")]
    ChannelSendError,

    #[error("Missing request payload")]
    MissingRequestPayload,

    #[error("Missing response payload")]
    MissingResponsePayload,
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

impl<T> From<tokio::sync::mpsc::error::SendError<T>> for Error {
    fn from(_: tokio::sync::mpsc::error::SendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl From<tokio::sync::oneshot::error::RecvError> for Error {
    fn from(_: tokio::sync::oneshot::error::RecvError) -> Self {
        Error::ChannelRecvError
    }
}
