mod error;
mod result;

use async_trait::async_trait;
use clap::Parser;
use consensus_core::networktype::NetworkType;
use kaspa_wrpc_server::router::{MessengerContainer, Router, RouterTarget, RpcApiContainer};
use result::Result;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use rpc_grpc::client::RpcApiGrpc;
use std::net::SocketAddr;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;

pub struct ProxyConnectionInner {
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
    pub grpc_api: Arc<RpcApiGrpc>,
}

/// GRPC Proxy Connection.  Owns [`ProxyConnectionInner`] that owns:
/// - [`RpcGrpcApi`] representing 1:1 GRPC connection
/// - [`Messenger`] representing client wRPC connection
#[derive(Clone)]
pub struct ProxyConnection {
    inner: Arc<ProxyConnectionInner>,
}

impl ProxyConnection {
    pub fn new(peer: SocketAddr, messenger: Arc<Messenger>, grpc_api: Arc<RpcApiGrpc>) -> ProxyConnection {
        ProxyConnection { inner: Arc::new(ProxyConnectionInner { peer, messenger, grpc_api }) }
    }
}

impl RpcApiContainer for ProxyConnection {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.inner.grpc_api.clone()
    }
}

impl MessengerContainer for ProxyConnection {
    fn get_messenger(&self) -> Arc<Messenger> {
        self.inner.messenger.clone()
    }
}

pub struct KaspaRpcProxyInner {
    network_type: NetworkType,
    verbose: bool,
}

/// A handler struct for the [`RpcHandler`].  Receives connection events
/// and a handshake, used to create a `ProxyConnection`
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

    /// Called on incoming wRPC connection. Creates a [`ProxyConnection`]
    /// representing the client connection bound to the `RpcApiGrpc` instance.
    /// The supplied [`Messenger`] represents the connection allowing
    /// to relay notifications to the wRPC client.
    async fn handshake(
        self: Arc<Self>,
        peer: &SocketAddr,
        _sender: &mut WebSocketSender,
        _receiver: &mut WebSocketReceiver,
        messenger: Arc<Messenger>,
    ) -> WebSocketResult<Self::Context> {
        let port = self.inner.network_type.port();
        let grpc_address = format!("grpc://127.0.0.1:{port}");
        println!("starting grpc client on {grpc_address}");
        let grpc = RpcApiGrpc::connect(grpc_address).await.map_err(|e| WebSocketError::Other(e.to_string()))?;
        grpc.start().await;
        let grpc = Arc::new(grpc);
        Ok(ProxyConnection::new(*peer, messenger, grpc))
    }
    async fn connect(self: Arc<Self>, _peer: &SocketAddr) -> WebSocketResult<()> {
        Ok(())
    }
}

impl RpcApiContainer for KaspaRpcProxy {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        panic!("Incorrect use: `proxy::KaspaRpcProxy` does not carry RpcApi reference")
    }
    fn verbose(&self) -> bool {
        self.inner.verbose
    }
}

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
    let Args { network_type, verbose, proxy_port } = Args::parse();

    let kaspad_port = network_type.port();
    log_info!("");
    log_info!("Proxy routing to `{}` gRPC on port {}", network_type, kaspad_port);
    let rpc_handler = KaspaRpcProxy::new(network_type, verbose);
    let router = Arc::new(Router::<KaspaRpcProxy, ProxyConnection>::new(rpc_handler.clone(), RouterTarget::Connection));
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
