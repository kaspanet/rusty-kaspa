mod error;
mod result;

// use clap::{Parser, Subcommand};
use clap::Parser;
use consensus_core::networktype::NetworkType;
// use kaspa_wrpc_server::router::Router;
// use error::Error;
use result::Result;
// use rpc_core::api::rpc::RpcApi;
use rpc_grpc::client::RpcApiGrpc;
use std::net::SocketAddr;
use std::sync::Arc;
use async_trait::async_trait;
use rpc_core::api::ops::RpcApiOps;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use workflow_log::*;
// use workflow_rpc::result::RpcResult as Response;
use workflow_rpc::server::prelude::*;

// use crate::placeholder::KaspaInterfacePlaceholder;

pub struct ProxyConnection {
    pub peer: SocketAddr,
    pub grpc_server: Arc<RpcApiGrpc>,
    // pub router: Router,
}

pub struct KaspaRpcProxy {
    network_type: NetworkType,
    verbose: bool,
}

impl KaspaRpcProxy {
    pub fn try_new(network_type: NetworkType, verbose: bool) -> Result<KaspaRpcProxy> {
        Ok(KaspaRpcProxy { network_type, verbose })
    }

    pub async fn init(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl RpcHandler for KaspaRpcProxy {
    type Context = ProxyConnection;


    async fn handshake(
        self: Arc<Self>,
        peer: &SocketAddr,
        _sender: &mut WebSocketSender,
        _receiver: &mut WebSocketReceiver,
        messenger: Arc<Messenger>,
    ) -> WebSocketResult<Arc<Self::Context>> {
        let port = self.network_type.port();
        let grpc_address = format!("grpc://127.0.0.1:{port}");
        println!("starting grpc client on {}", grpc_address);
        let grpc = RpcApiGrpc::connect(grpc_address).await.map_err(|e| WebSocketError::Other(e.to_string()))?;
        grpc.start().await;
    
        // let grpc_server: Arc<dyn RpcApi> = Arc::new(grpc);
        let grpc_server = Arc::new(grpc);
        // let router = Router::new(grpc_server.clone(), self.verbose);
    
        Ok(Arc::new(ProxyConnection { peer : peer.clone(), grpc_server }))

    }
    async fn connect(self: Arc<Self>, _peer: &SocketAddr) -> WebSocketResult<()> {
        Ok(())
    }

    // async fn handle_request(self: Arc<Self>, ctx: &mut Self::Context, op: RpcApiOps, data: &[u8]) -> Response {
    //     ctx.router.route(op, data).await
    //     // Ok(().try_to_vec()?)
    // }
}

#[derive(Debug, Parser)] //clap::Args)]
#[clap(name = "proxy")]
#[clap(version)]
// #[clap(
//     setting = clap::AppSettings::DeriveDisplayOrder,
// )]
struct Args {
    /// network
    #[clap(name = "network", default_value = "mainnet")]
    network_type: NetworkType,
    #[clap(long, name = "port", default_value = "9292")]
    proxy_port: u16,
    #[clap(short, long)]
    verbose: bool,
}



#[tokio::main]
async fn main() -> Result<()> {
    let Args { network_type, verbose, proxy_port } = Args::parse();

    // workflow_log::set_log_level()
    let kaspad_port = network_type.port();
    log_info!("");
    log_info!("Proxy routing to `{}` gRPC on port {}", network_type, kaspad_port);
    let handler = Arc::new(KaspaRpcProxy::try_new(network_type, verbose)?);
    handler.init().await?;

    let mut interface = Interface::<ProxyConnection, KaspaRpcProxy, RpcApiOps>::new(handler.clone());

    let interface = Arc::new(interface);


    let server = RpcServer::new_with_encoding::<ProxyConnection, KaspaRpcProxy, RpcApiOps, Id64>(Encoding::Borsh, handler, interface);

    // let server = RpcServer::new(handler);

    let addr = format!("0.0.0.0:{proxy_port}");
    log_info!("Kaspa wRPC server is listening on {}", addr);
    server.listen(&addr).await?;

    Ok(())
}
