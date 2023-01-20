use async_trait::async_trait;
use kaspa_core::{
    task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture},
    trace,
};
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use std::{sync::Arc, time::Duration};
use workflow_log::*;
use workflow_rpc::asynchronous::result::RpcResult as Response;
use workflow_rpc::asynchronous::server::*;

use crate::placeholder::KaspaInterfacePlaceholder;
use crate::result::Result;
use crate::router::Router;

pub struct RpcConnectionCtx {
    pub peer: SocketAddr,
}

pub struct Options {
    addr: String,
    verbose: bool,
}

pub struct KaspaRpcHandler {
    router: Router,
}

impl KaspaRpcHandler {
    pub fn new(router: Router) -> KaspaRpcHandler {
        KaspaRpcHandler { router }
    }
}

#[async_trait]
impl RpcHandler<RpcApiOps> for KaspaRpcHandler {
    type Context = RpcConnectionCtx;

    async fn connect(self: Arc<Self>, peer: SocketAddr) -> WebSocketResult<Self::Context> {
        Ok(RpcConnectionCtx { peer })
    }

    async fn handshake(
        self: Arc<Self>,
        _ctx: &mut Self::Context,
        sender: &mut WebSocketSender,
        receiver: &mut WebSocketReceiver,
        _sink: &WebSocketSink,
    ) -> WebSocketResult<()> {
        handshake::greeting(
            Duration::from_millis(3000),
            sender,
            receiver,
            Box::pin(|msg| {
                msg != "kaspa"
                // if msg != "kaspa" {
                //     Err(WebSocketError::NegotiationFailure)
                // } else {
                //     Ok(())
                // }
            }),
        )
        .await
        // Ok(())
    }

    async fn handle_request(self: Arc<Self>, _ctx: &mut Self::Context, op: RpcApiOps, data: &[u8]) -> Response {
        self.router.route(op, data).await
        // Ok(().try_to_vec()?)
    }
}

pub struct WrpcServer {
    options: Options,
    server: RpcServer<RpcApiOps, RpcConnectionCtx>,
}

impl WrpcServer {
    pub fn new(options: Options, rpc_api_iface: Arc<dyn RpcApi>) -> Self {
        let iface: Arc<dyn RpcApi> = Arc::new(KaspaInterfacePlaceholder {});
        let router = Router::new(iface, options.verbose);
        let handler = Arc::new(KaspaRpcHandler::new(router));
        let server = RpcServer::new(handler);

        WrpcServer { options, server }
    }

    async fn run(self: Arc<Self>) -> Result<()> {
        let addr = &self.options.addr;
        log_info!("Kaspa workflow RPC server is listening on {}", addr);
        self.server.listen(&addr).await?;
        Ok(())
    }
}

const WRPC_SERVER: &str = "WRPC_SERVER";

impl AsyncService for WrpcServer {
    fn ident(self: Arc<Self>) -> &'static str {
        WRPC_SERVER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { self.run().await.map_err(|err| AsyncServiceError::Service(format!("wRPC error: `{}`", err))) })
    }

    fn signal_exit(self: Arc<Self>) {
        self.server.shutdown()
        // self.shutdown.request.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.server.shutdown().await.unwrap_or_else(|err| log_trace!("wRPC shutdown error: `{}`", err));
            Ok(())
        })
    }
}

// pub async fn rpc_server_task(addr: &str, verbose : bool) -> Result<()> {
