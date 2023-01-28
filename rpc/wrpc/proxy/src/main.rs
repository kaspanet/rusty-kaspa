mod error;
mod result;

// use clap::{Parser, Subcommand};
use clap::Parser;
use consensus_core::networktype::NetworkType;
use kaspa_wrpc_server::router::Router;
use kaspa_wrpc_server::router::RpcApiContainer;
// use error::Error;
use result::Result;
// use rpc_core::api::rpc::RpcApi;
use async_trait::async_trait;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
use rpc_core::message::*;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use rpc_grpc::client::RpcApiGrpc;
use std::net::SocketAddr;
use std::sync::Arc;
use workflow_log::*;
// use workflow_rpc::result::RpcResult as Response;
use workflow_rpc::server::prelude::*;

// use crate::placeholder::KaspaInterfacePlaceholder;

pub struct ProxyConnectionInner {
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
    pub grpc_api: Arc<RpcApiGrpc>,
    // pub rpc_api: Arc<dyn RpcApi>,
}

#[derive(Clone)]
pub struct ProxyConnection {
    inner: Arc<ProxyConnectionInner>,
}

impl ProxyConnection {
    // pub fn new(peer: SocketAddr, messenger: Arc<Messenger>, rpc_api : Arc<dyn RpcApi>) -> ProxyConnection {
    pub fn new(peer: SocketAddr, messenger: Arc<Messenger>, grpc_api: Arc<RpcApiGrpc>) -> ProxyConnection {
        ProxyConnection { inner: Arc::new(ProxyConnectionInner { peer, messenger, grpc_api }) }
    }
}

impl RpcApiContainer for ProxyConnection {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.inner.grpc_api.clone()
    }
}

pub struct KaspaRpcProxyInner {
    network_type: NetworkType,
    verbose: bool,
}

#[derive(Clone)]
pub struct KaspaRpcProxy {
    inner: Arc<KaspaRpcProxyInner>,
}

impl KaspaRpcProxy {
    pub fn new(network_type: NetworkType, verbose: bool) -> KaspaRpcProxy {
        KaspaRpcProxy { inner: Arc::new(KaspaRpcProxyInner { network_type, verbose }) }
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
    ) -> WebSocketResult<Self::Context> {
        let port = self.inner.network_type.port();
        let grpc_address = format!("grpc://127.0.0.1:{port}");
        println!("starting grpc client on {}", grpc_address);
        let grpc = RpcApiGrpc::connect(grpc_address).await.map_err(|e| WebSocketError::Other(e.to_string()))?;
        grpc.start().await;
        let grpc = Arc::new(grpc);
        // let rpc_api: Arc<dyn RpcApi> = grpc_rpc_api.clone();
        // let rpc_api: Arc<dyn RpcApi> = Arc::new(grpc);
        Ok(ProxyConnection::new(*peer, messenger, grpc))
        // { peer: *peer, messenger, rpc_api }))
    }
    async fn connect(self: Arc<Self>, _peer: &SocketAddr) -> WebSocketResult<()> {
        Ok(())
    }
}

impl RpcApiContainer for KaspaRpcProxy {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        panic!("Incorrect use: `proxy::KaspaRpcProxy` does not carry RpcApi reference")
    }
}

#[derive(Debug, Parser)]
#[clap(name = "proxy")]
#[clap(version)]
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

    let kaspad_port = network_type.port();
    log_info!("");
    log_info!("Proxy routing to `{}` gRPC on port {}", network_type, kaspad_port);
    let rpc_handler = KaspaRpcProxy::new(network_type, verbose);
    let router = Arc::new(Router::<KaspaRpcProxy, ProxyConnection>::new(rpc_handler.clone()));
    let server = RpcServer::new_with_encoding::<KaspaRpcProxy, ProxyConnection, RpcApiOps, Id64>(
        Encoding::Borsh,
        Arc::new(rpc_handler),
        router.interface.clone(),
    );

    let addr = format!("0.0.0.0:{proxy_port}");
    log_info!("Kaspa wRPC server is listening on {}", addr);
    server.listen(&addr).await?;

    Ok(())
}
