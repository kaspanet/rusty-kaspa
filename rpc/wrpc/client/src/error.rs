use thiserror::Error;
use workflow_rpc::asynchronous::client::error::Error as RpcError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
}

impl Into<rpc_core::errors::RpcError> for Error {
    fn into(self) -> rpc_core::errors::RpcError {
        self.to_string().into()
    }
}
