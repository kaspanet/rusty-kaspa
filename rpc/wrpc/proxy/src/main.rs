mod error;
mod result;

use clap::Parser;
use consensus_core::networktype::NetworkType;
use kaspa_rpc_core::api::ops::RpcApiOps;
use kaspa_wrpc_server::connection::Connection;
use kaspa_wrpc_server::router::Router;
use kaspa_wrpc_server::server::Server;
use kaspa_wrpc_server::service::{KaspaRpcHandler, Options};
use result::Result;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;

#[derive(Debug, Parser)]
#[clap(name = "proxy")]
#[clap(version)]
struct Args {
    /// network type
    #[clap(name = "network", default_value = "mainnet")]
    network_type: NetworkType,
    // /// wRPC port
    /// interface:port for wRPC server (wrpc://127.0.0.1:17110)
    #[clap(long)]
    interface: Option<String>,
    /// Number of notification serializer threads
    #[clap(long)]
    threads: Option<usize>,
    /// Enable verbose logging
    #[clap(short, long)]
    verbose: bool,
    /// Protocol encoding
    #[clap(long)]
    encoding: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { network_type, interface, verbose, threads, encoding } = Args::parse();
    let proxy_port: u16 = 17110;

    let encoding: Encoding = encoding.unwrap_or_else(|| "borsh".to_owned()).parse()?;
    let kaspad_port = network_type.port();

    let options = Arc::new(Options {
        listen_address: interface.unwrap_or_else(|| format!("wrpc://127.0.0.1:{proxy_port}")),
        grpc_proxy_address: Some(format!("grpc://127.0.0.1:{kaspad_port}")),
        verbose,
        // ..Options::default()
    });
    log_info!("");
    log_info!("Proxy routing to `{}` on {}", network_type, options.grpc_proxy_address.as_ref().unwrap());

    let tasks = threads.unwrap_or_else(num_cpus::get);
    let rpc_handler = Arc::new(KaspaRpcHandler::proxy(tasks, encoding, options.clone()));

    let router = Arc::new(Router::new(rpc_handler.server.clone()));
    let server = RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(encoding, rpc_handler, router.interface.clone());

    log_info!("Kaspa wRPC server is listening on {}", options.listen_address);
    log_info!("Using `{encoding}` protocol encoding");
    server.listen(&options.listen_address).await?;

    Ok(())
}
