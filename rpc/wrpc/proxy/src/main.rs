mod error;
mod result;

use clap::Parser;
use consensus_core::networktype::NetworkType;
use kaspa_wrpc_server::connection::Connection;
use kaspa_wrpc_server::router::Router;
use kaspa_wrpc_server::server::Server;
use kaspa_wrpc_server::service::{KaspaRpcHandler, Options};
use result::Result;
use rpc_core::api::ops::RpcApiOps;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;

#[derive(Debug, Parser)]
#[clap(name = "proxy")]
#[clap(version)]
struct Args {
    /// network
    #[clap(name = "network", default_value = "mainnet")]
    network_type: NetworkType,
    #[clap(long, name = "port", default_value = "17110")]
    proxy_port: u16,
    #[clap(short, long)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { network_type, proxy_port, verbose } = Args::parse();

    let kaspad_port = network_type.port();

    let options = Arc::new(Options {
        listen_address: format!("127.0.0.1:{proxy_port}"),
        grpc_proxy_address: Some(format!("grpc://127.0.0.1:{kaspad_port}")),
        verbose,
        // ..Options::default()
    });
    log_info!("");
    log_info!("Proxy routing to `{}` gRPC on {}", network_type, options.grpc_proxy_address.as_ref().unwrap());

    let tasks = num_cpus::get();
    let rpc_handler = Arc::new(KaspaRpcHandler::proxy(tasks, options.clone()));

    let router = Arc::new(Router::new(rpc_handler.server.clone()));
    let server =
        RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(Encoding::Borsh, rpc_handler, router.interface.clone());

    log_info!("Kaspa wRPC server is listening on {}", options.listen_address);
    server.listen(&options.listen_address).await?;

    Ok(())
}
