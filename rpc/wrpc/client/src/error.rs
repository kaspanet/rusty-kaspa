use thiserror::Error;
use workflow_rpc::asynchronous::client::error::Error as RpcError;

#[derive(Debug, Error)]
pub enum Error {
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
}

impl From<Error> for rpc_core::error::RpcError {
    fn from(err: Error) -> Self {
        err.to_string().into()
    }
}
