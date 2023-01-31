#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    GrpcApi(#[from] rpc_core::error::RpcError),

    #[error(transparent)]
    GrpcClient(#[from] rpc_grpc::client::errors::Error),

    #[error(transparent)]
    Wrpc(#[from] kaspa_wrpc_server::error::Error),

    #[error(transparent)]
    WebSocket(#[from] workflow_websocket::server::error::Error),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}
