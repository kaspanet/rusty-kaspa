use rpc_core::RpcError as KaspaRpcError;
// use rpc_core::client::error::Error as KaspaRpcClientError;
use kaspa_wrpc_client::error::Error as KaspaWorkflowRpcError;
use workflow_rpc::asynchronous::client::error::Error as RpcError;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error: {0}")]
    String(String),

    #[error("RPC error: {0}")]
    KaspaRpcClientResult(#[from] KaspaRpcError),

    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),

    #[error("RPC error: {0}")]
    KaspaWorkflowRpcError(#[from] KaspaWorkflowRpcError),
}
