mod error;
mod result;

use async_trait::async_trait;
use clap::Parser;
use consensus_core::networktype::NetworkType;
use kaspa_wrpc_server::connection::Connection;
use kaspa_wrpc_server::router::Router;
use kaspa_wrpc_server::server::Server;
use result::Result;
use rpc_core::api::ops::RpcApiOps;
use std::net::SocketAddr;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;

pub struct KaspaRpcProxy {
    pub server: Server,
    // pub options: Arc<Options>,
}

impl KaspaRpcProxy {
    pub fn new(tasks: usize, proxy_network_type: NetworkType) -> KaspaRpcProxy {
        KaspaRpcProxy { server: Server::new(tasks, None, Some(proxy_network_type)) }
    }
}

#[async_trait]
impl RpcHandler for KaspaRpcProxy {
    type Context = Connection;

    async fn connect(self: Arc<Self>, _peer: &SocketAddr) -> WebSocketResult<()> {
        Ok(())
    }

    async fn handshake(
        self: Arc<Self>,
        peer: &SocketAddr,
        _sender: &mut WebSocketSender,
        _receiver: &mut WebSocketReceiver,
        messenger: Arc<Messenger>,
    ) -> WebSocketResult<Connection> {
        // TODO - discuss and implement handshake
        // handshake::greeting(
        //     std::time::Duration::from_millis(3000),
        //     sender,
        //     receiver,
        //     Box::pin(|msg| if msg != "kaspa" { Err(WebSocketError::NegotiationFailure) } else { Ok(()) }),
        // )
        // .await

        let connection = self.server.connect(peer, messenger).await.map_err(|err| err.to_string())?;
        Ok(connection)
    }

    /// Disconnect the websocket. Receives `Connection` (a.k.a `Self::Context`)
    /// before dropping it. This is the last chance to cleanup and resources owned by
    /// this connection. Delegate to ConnectoinManager.
    async fn disconnect(self: Arc<Self>, ctx: Self::Context, _result: WebSocketResult<()>) {
        self.server.disconnect(ctx).await;
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
    // #[clap(short, long)]
    // verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Args { network_type, proxy_port } = Args::parse();

    let kaspad_port = network_type.port();
    log_info!("");
    log_info!("Proxy routing to `{}` gRPC on port {}", network_type, kaspad_port);
    let tasks = num_cpus::get();
    let rpc_handler = Arc::new(KaspaRpcProxy::new(tasks, network_type));

    // let rpc_handler = KaspaRpcProxy::new(network_type, verbose);
    let router = Arc::new(Router::new(rpc_handler.server.clone()));
    let server =
        RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(Encoding::Borsh, rpc_handler, router.interface.clone());

    let addr = format!("0.0.0.0:{proxy_port}");
    log_info!("Kaspa wRPC server is listening on {}", addr);
    server.listen(&addr).await?;

    Ok(())
}
