use std::sync::PoisonError;
use thiserror::Error;
use workflow_rpc::server::error::Error as RpcError;
use workflow_rpc::server::WebSocketError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] WebSocketError),

    #[error("Poison error")]
    PoisonError,
}

impl<T> From<PoisonError<T>> for Error {
    fn from(_: PoisonError<T>) -> Self {
        Error::PoisonError
    }
}
