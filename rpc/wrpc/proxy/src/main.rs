mod error;
mod result;

use consensus_core::networktype::NetworkType;
// use error::Error;
use result::Result;
use rpc_core::api::rpc::RpcApi;
use rpc_grpc::client::RpcApiGrpc;
use std::sync::Arc;
use std::time::Duration;
use workflow_core::task::*;

#[tokio::main]
async fn main() -> Result<()> {
    let port = NetworkType::Mainnet.port();
    let grpc_address = format!("grpc://127.0.0.1:{port}");
    println!("starting grpc client on {}", grpc_address);
    let grpc = RpcApiGrpc::connect(grpc_address).await?;
    grpc.start().await;

    let grpc_server: Arc<dyn RpcApi> = Arc::new(grpc);

    loop {
        let info = grpc_server.get_info().await;
        println!("info: {:?}", info);
        sleep(Duration::from_millis(5000)).await;
    }

    Ok(())
}
