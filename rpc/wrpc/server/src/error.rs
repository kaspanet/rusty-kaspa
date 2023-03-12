use kaspa_notify::error::Error as NotifyError;
use kaspa_rpc_core::RpcError;
use std::sync::PoisonError;
use thiserror::Error;
use workflow_rpc::server::{error::Error as RpcServerError, WebSocketError};

#[derive(Debug, Error)]
pub enum Error {
    #[error("RpcServer error: {0}")]
    RpcServerError(#[from] RpcServerError),

    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] WebSocketError),

    #[error("Poison error")]
    PoisonError,

    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("Notify error: {0}")]
    NotifyError(#[from] NotifyError),
}

impl<T> From<PoisonError<T>> for Error {
    fn from(_: PoisonError<T>) -> Self {
        Error::PoisonError
    }
}
