#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    GrpcApi(#[from] rpc_core::error::RpcError),

    #[error(transparent)]
    GrpcClient(#[from] rpc_grpc::client::errors::Error),

    #[error(transparent)]
    Wrpc(#[from] kaspa_wrpc_server::error::Error),
}
