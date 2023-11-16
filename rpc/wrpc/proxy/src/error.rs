#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    GrpcApi(#[from] kaspa_rpc_core::error::RpcError),

    #[error(transparent)]
    GrpcClient(#[from] kaspa_grpc_client::error::Error),

    #[error(transparent)]
    Wrpc(#[from] kaspa_wrpc_server::error::Error),

    #[error(transparent)]
    WebSocket(#[from] workflow_rpc::server::WebSocketError),

    #[allow(clippy::enum_variant_names)]
    #[error(transparent)]
    RpcError(#[from] workflow_rpc::error::Error),
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}
