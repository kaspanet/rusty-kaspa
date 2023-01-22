use thiserror::Error;
use workflow_rpc::server::error::Error as RpcError;
use workflow_websocket::server::Error as WebSocketError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("WebSocket error: {0}")]
    WebSocketError(#[from] WebSocketError),
}
