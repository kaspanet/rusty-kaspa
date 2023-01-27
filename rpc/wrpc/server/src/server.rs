use async_trait::async_trait;
use kaspa_core::task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture};
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
#[allow(unused_imports)]
use rpc_core::error::RpcResult;
#[allow(unused_imports)]
use rpc_core::notify::channel::*;
#[allow(unused_imports)]
use rpc_core::notify::listener::*;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use workflow_log::*;
// use workflow_rpc::result::ServerResult;
use crate::result::Result;
use crate::router::Router;
use workflow_rpc::server::prelude::*;
pub use workflow_rpc::server::Encoding as WrpcEncoding;

pub struct Options {
    pub listen_address: String,
    pub verbose: bool,
    pub encoding: Encoding,
}

impl Options {
    pub fn new(encoding: Encoding, listen_address: &str, verbose : bool) -> Self {
        Self { listen_address: listen_address.to_string(), encoding, verbose }
    }
}

// impl Default for Options {
//     fn default() -> Self {
//         Options {
//             listen_address: "127.0.0.1:8080".to_owned(),
//             verbose: false,
//         }
//     }
// }

#[derive(Debug)]
pub struct ConnectionContext {
    pub peer: SocketAddr,
    pub messenger: Arc<Messenger>,
}

impl ConnectionContext {
    pub fn new(peer: &SocketAddr, messenger: Arc<Messenger>) -> Self {
        ConnectionContext { peer: *peer, messenger }
    }
}

pub struct KaspaRpcHandler {
    router: Router<ConnectionContext>,
    // pub options: Options,
    pub sockets: Mutex<HashMap<SocketAddr, Arc<ConnectionContext>>>,
}

impl KaspaRpcHandler {
    pub fn new(router: Router<ConnectionContext>) -> KaspaRpcHandler {
        KaspaRpcHandler { router, sockets: Mutex::new(HashMap::new()) }
    }
    // pub fn new() -> KaspaRpcHandler {
    //     KaspaRpcHandler { sockets: Mutex::new(HashMap::new()) }
    // }
}

#[async_trait]
impl RpcHandler for KaspaRpcHandler {
    type Context = Arc<ConnectionContext>;

    async fn connect(self: Arc<Self>, _peer: &SocketAddr) -> WebSocketResult<()> {
        Ok(())
    }

    async fn handshake(
        self: Arc<Self>,
        peer: &SocketAddr,
        _sender: &mut WebSocketSender,
        _receiver: &mut WebSocketReceiver,
        messenger: Arc<Messenger>,
    ) -> WebSocketResult<Self::Context> {
        // handshake::greeting(
        //     std::time::Duration::from_millis(3000),
        //     sender,
        //     receiver,
        //     Box::pin(|msg| if msg != "kaspa" { Err(WebSocketError::NegotiationFailure) } else { Ok(()) }),
        // )
        // .await

        let ctx = Arc::new(ConnectionContext::new(peer, messenger));
        self.sockets.lock().unwrap().insert(*peer, ctx.clone());
        Ok(ctx)
    }

    async fn disconnect(self: Arc<Self>, ctx: Self::Context, _result: WebSocketResult<()>) {
        self.sockets.lock().unwrap().remove(&ctx.peer);
    }
}

struct ServerContext;

pub struct WrpcServer {
    options: Options,
    server: RpcServer,
}

impl WrpcServer {
    pub fn new(options: Options, rpc_api_iface: Arc<dyn RpcApi>) -> Self {
        let router = Router::new(rpc_api_iface, options.verbose);
        let handler = Arc::new(KaspaRpcHandler::new(router));
        // let server_ctx = Arc::new(ServerContext);

        // let mut interface = Interface::< KaspaRpcHandler, ConnectionContext,RpcApiOps>::new(handler.clone());

        // interface.method(
        //     TestOps::EvenOdd,
        //     method!(|_connection_ctx, _server_ctx, req: TestReq| async move {
        //         if req.v & 1 == 0 {
        //             Ok(TestResp::Even(req.v))
        //         } else {
        //             Ok(TestResp::Odd(req.v))
        //         }
        //     }),
        // );

        // interface.notification(
        //     TestOps::Notify,
        //     notification!(
        //         |_connection_ctx, _server_ctx, _req: TestNotify| async move {
        //             // Ok(TestResp::Increase(req.v + 100))
        //             Ok(())
        //         }
        //     ),
        // );

        // let interface = Arc::new(interface);

        let server = RpcServer::new_with_encoding::<Arc<dyn RpcApi>, Arc<ConnectionContext>, RpcApiOps, Id64>(
            options.encoding.clone(),
            handler,
            router.interface,
        );

        WrpcServer { options, server }
    }

    async fn run(self: Arc<Self>) -> Result<()> {
        let addr = &self.options.listen_address;
        log_info!("wRPC server is listening on {}", addr);
        self.server.listen(addr).await?;
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
        self.server.stop().unwrap_or_else(|err| log_trace!("wRPC unable to signal shutdown: `{}`", err));
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.server.join().await.map_err(|err| AsyncServiceError::Service(format!("wRPC error: `{}`", err)))?;
            Ok(())
        })
    }
}

// pub async fn rpc_server_task(addr: &str, verbose : bool) -> Result<()> {
