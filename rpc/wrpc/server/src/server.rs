use crate::result::Result;
use crate::router::Router;
use crate::router::RpcApiContainer;
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
use workflow_rpc::server::prelude::*;
pub use workflow_rpc::server::Encoding as WrpcEncoding;

pub struct Options {
    pub listen_address: String,
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { listen_address: "127.0.0.1:8080".to_owned(), verbose: false }
    }
}

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
type ConnectionContextReference = Arc<ConnectionContext>;

impl RpcApiContainer for ConnectionContextReference {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        panic!("Incorrect use: `server::ConnectionContext` does not carry RpcApi references")
    }
}

pub struct KaspaRpcHandler {
    pub rpc_api: Arc<dyn RpcApi>,
    pub sockets: Mutex<HashMap<SocketAddr, Arc<ConnectionContext>>>,
    pub options: Arc<Options>,
}
type KaspaRpcHandlerReference = Arc<KaspaRpcHandler>;

impl KaspaRpcHandler {
    pub fn new(rpc_api: Arc<dyn RpcApi>, options: Arc<Options>) -> KaspaRpcHandler {
        KaspaRpcHandler { rpc_api, sockets: Mutex::new(HashMap::new()), options }
    }
}

impl RpcApiContainer for KaspaRpcHandlerReference {
    fn get_rpc_api(&self) -> Arc<dyn RpcApi> {
        self.rpc_api.clone()
    }

    fn verbose(&self) -> bool {
        self.options.verbose
    }
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
        // TODO - discuss and implement handshake
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

pub struct ServerContext;

pub struct WrpcServer {
    options: Arc<Options>,
    server: RpcServer,
}

impl WrpcServer {
    pub fn new(rpc_api: Arc<dyn RpcApi>, encoding: &Encoding, options: Options) -> Self {
        let options = Arc::new(options);
        let rpc_handler = Arc::new(KaspaRpcHandler::new(rpc_api, options.clone()));
        let router = Arc::new(Router::<KaspaRpcHandlerReference, ConnectionContextReference>::new(rpc_handler.clone()));
        let server = RpcServer::new_with_encoding::<KaspaRpcHandlerReference, ConnectionContextReference, RpcApiOps, Id64>(
            *encoding,
            rpc_handler,
            router.interface.clone(),
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
