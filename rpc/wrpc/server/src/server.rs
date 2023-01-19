use async_trait::async_trait;
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::asynchronous::result::RpcResult as Response;
use workflow_rpc::asynchronous::server::*;

use crate::placeholder::KaspaInterfacePlaceholder;
use crate::result::Result;
use crate::router::Router;

pub struct RpcConnection {
    // pub peer: SocketAddr,
}

pub struct Options {
    addr: String,
    verbose: bool,
}

pub struct KaspaRpcHandler {
    router: Router,
}

impl KaspaRpcHandler {
    // #[allow(dead_code)]
    pub fn try_new(router: Router) -> Result<KaspaRpcHandler> {
        Ok(KaspaRpcHandler { router })
    }

    pub async fn init(&self) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
// impl RpcHandlerBorsh<RpcApiOps> for Server
impl RpcHandler<RpcApiOps> for KaspaRpcHandler {
    type Context = RpcConnection;

    async fn connect(self: Arc<Self>, _peer: SocketAddr) -> WebSocketResult<Self::Context> {
        Ok(RpcConnection {
            // peer 
        })
    }

    async fn handle_request(self: Arc<Self>, _ctx: &mut Self::Context, op: RpcApiOps, data: &[u8]) -> Response {
        self.router.route(op, data).await
        // Ok(().try_to_vec()?)
    }
}

// pub async fn rpc_server_task(addr: &str, verbose : bool) -> Result<()> {
pub async fn rpc_server_task(options: Options) -> Result<()> {
    let Options { addr, verbose } = options;

    let iface: Arc<dyn RpcApi> = Arc::new(KaspaInterfacePlaceholder {});
    let router = Router::new(iface, verbose);
    let handler = Arc::new(KaspaRpcHandler::try_new(router)?);
    handler.init().await?;
    let server = RpcServer::new(handler);

    log_info!("Kaspa workflow RPC server is listening on {}", addr);
    server.listen(&addr).await?;

    Ok(())
}
