mod error;
mod result;

use clap::Parser;
use kaspa_consensus_core::network::NetworkType;
use kaspa_rpc_core::api::ops::RpcApiOps;
use kaspa_wrpc_server::{
    connection::Connection,
    router::Router,
    server::Server,
    service::{KaspaRpcHandler, Options},
};
use result::Result;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;
use workflow_rpc::server::WebSocketCounters;

#[derive(Debug, Parser)]
#[clap(name = "proxy")]
#[clap(version)]
struct Args {
    /// proxy for testnet network
    #[clap(long)]
    testnet: bool,
    /// proxy for simnet network
    #[clap(long)]
    simnet: bool,
    /// proxy for devnet network
    #[clap(long)]
    devnet: bool,

    /// proxy:port for gRPC server (grpc://127.0.0.1:16110)
    #[clap(name = "grpc")]
    grpc_proxy_address: Option<String>,

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
    let Args { testnet, simnet, devnet, grpc_proxy_address, interface, verbose, threads, encoding } = Args::parse();

    let network_type = if testnet {
        NetworkType::Testnet
    } else if simnet {
        NetworkType::Simnet
    } else if devnet {
        NetworkType::Devnet
    } else {
        NetworkType::Mainnet
    };

    let kaspad_port = network_type.default_rpc_port();

    let encoding: Encoding = encoding.unwrap_or_else(|| "borsh".to_owned()).parse()?;
    let proxy_port = match encoding {
        Encoding::Borsh => network_type.default_borsh_rpc_port(),
        Encoding::SerdeJson => network_type.default_json_rpc_port(),
    };

    let options = Arc::new(Options {
        listen_address: interface.unwrap_or_else(|| format!("wrpc://127.0.0.1:{proxy_port}")),
        grpc_proxy_address: Some(grpc_proxy_address.unwrap_or_else(|| format!("grpc://127.0.0.1:{kaspad_port}"))),
        verbose,
        // ..Options::default()
    });
    log_info!("");
    log_info!("Proxy routing to `{}` on {}", network_type, options.grpc_proxy_address.as_ref().unwrap());

    let counters = Arc::new(WebSocketCounters::default());
    let tasks = threads.unwrap_or_else(num_cpus::get);
    let rpc_handler = Arc::new(KaspaRpcHandler::new(tasks, encoding, None, options.clone()));

    let router = Arc::new(Router::new(rpc_handler.server.clone()));
    let server = RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(
        encoding,
        rpc_handler.clone(),
        router.interface.clone(),
        Some(counters),
        false,
    );

    log_info!("Kaspa wRPC server is listening on {}", options.listen_address);
    log_info!("Using `{encoding}` protocol encoding");

    let config = WebSocketConfig { max_message_size: Some(1024 * 1024 * 1024), ..Default::default() };
    let listener = server.bind(&options.listen_address).await?;
    server.listen(listener, Some(config)).await?;

    Ok(())
}
