use kaspa_notify::error::Error as NotifyError;
use kaspa_rpc_core::RpcError;
use thiserror::Error;

pub type BoxedStdError = Box<dyn std::error::Error + Sync + std::marker::Send + 'static>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    #[error("GRPC invalid address schema {0}")]
    GrpcAddressSchema(String),

    #[error("GRPC client error {0}")]
    TonicStatus(#[from] tonic::Status),

    /// RPC call timeout
    #[error("RPC request timeout")]
    Timeout,

    #[error("Endpoint connection error: {0}")]
    EndpointConnectionError(#[from] tonic::transport::Error),

    #[error("Notify error: {0}")]
    NotifyError(#[from] kaspa_notify::error::Error),

    #[error("RPC: channel receive error")]
    ChannelRecvError,

    #[error("RPC: channel send error")]
    ChannelSendError,

    #[error("Missing request payload")]
    MissingRequestPayload,

    #[error("Missing response payload")]
    MissingResponsePayload,

    #[error("Not connected to server")]
    NotConnected,
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

impl<T> From<async_channel::SendError<T>> for Error {
    fn from(_: async_channel::SendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl<T> From<async_channel::TrySendError<T>> for Error {
    fn from(_: async_channel::TrySendError<T>) -> Self {
        Error::ChannelSendError
    }
}

impl From<async_channel::RecvError> for Error {
    fn from(_: async_channel::RecvError) -> Self {
        Error::ChannelRecvError
    }
}

impl From<Error> for NotifyError {
    fn from(err: Error) -> Self {
        match err {
            Error::String(message) => NotifyError::General(message),
            Error::NotifyError(err) => err,
            Error::ChannelRecvError => NotifyError::ChannelRecvError,
            Error::ChannelSendError => NotifyError::ChannelSendError,
            Error::TonicStatus(err) => NotifyError::General(format!("{err}")),
            _ => NotifyError::General(format!("{err}")),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
