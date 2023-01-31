use crate::connection::*;
use crate::result::Result;
use crate::router::*;
use crate::server::*;
use async_trait::async_trait;
use kaspa_core::task::service::{AsyncService, AsyncServiceError, AsyncServiceFuture};
use rpc_core::api::ops::RpcApiOps;
use rpc_core::api::rpc::RpcApi;
use std::sync::Arc;
use workflow_log::*;
use workflow_rpc::server::prelude::*;
pub use workflow_rpc::server::Encoding as WrpcEncoding;

/// Options for configuring the wRPC server
pub struct Options {
    pub listen_address: String,
    pub grpc_proxy_address: Option<String>,
    pub verbose: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { listen_address: "127.0.0.1:17110".to_owned(), verbose: false, grpc_proxy_address: None }
    }
}

/// ### KaspaRpcHandler
///
/// [`KaspaRpcHandler`] is a handler struct that implements the [`RpcHandler`] trait
/// allowing it to receive [`connect()`](RpcHandler::connect),
/// [`disconnect()`](RpcHandler::disconnect) and [`handshake()`](RpcHandler::handshake)
/// calls invoked by the [`RpcServer`].
///
/// [`RpcHandler::handshake`] is called by the [`RpcServer`] supplying the [`Messenger`]
/// and expecting user to return a `ServerContext` struct (or an `Arc` of) where
/// this struct will be supplied to each RPC method call.  Each RPC method call receives
/// 3 arguments - `ServerContext`, `ConnectionContext` and `Request`. Upon completion
/// the method should return a `Result`.
///
/// RPC method handling is implemented in the [`Router`].
///
pub struct KaspaRpcHandler {
    pub server: Server,
    pub options: Arc<Options>,
}

impl KaspaRpcHandler {
    pub fn new(tasks: usize, rpc_api: Option<Arc<dyn RpcApi>>, options: Arc<Options>) -> KaspaRpcHandler {
        KaspaRpcHandler { server: Server::new(tasks, rpc_api, options.clone()), options }
    }
    pub fn proxy(tasks: usize, options: Arc<Options>) -> KaspaRpcHandler {
        KaspaRpcHandler { server: Server::new(tasks, None, options.clone()), options }
    }
}

#[async_trait]
impl RpcHandler for KaspaRpcHandler {
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

///
///  wRPC Server - A wrapper around and an initializer of the RpcServer
///
pub struct WrpcService {
    options: Arc<Options>,
    server: RpcServer,
}

impl WrpcService {
    /// Create and initialize RpcServer
    pub fn new(tasks: usize, rpc_api: Arc<dyn RpcApi>, encoding: &Encoding, options: Options) -> Self {
        let options = Arc::new(options);
        // Create handle to manage connections
        let rpc_handler = Arc::new(KaspaRpcHandler::new(tasks, Some(rpc_api), options.clone()));
        // Create router (initializes Interface registering RPC method and notification handlers)
        let router = Arc::new(Router::new(rpc_handler.server.clone()));
        // Create a server
        // let server = RpcServer::new_with_encoding::<KaspaRpcHandlerReference, Connection, RpcApiOps, Id64>(
        let server =
            RpcServer::new_with_encoding::<Server, Connection, RpcApiOps, Id64>(*encoding, rpc_handler, router.interface.clone());

        WrpcService { options, server }
    }

    /// Start listening on the configured address (will yield an error if the the socket listen() fails)
    async fn run(self: Arc<Self>) -> Result<()> {
        let addr = &self.options.listen_address;
        log_info!("wRPC server is listening on {}", addr);
        self.server.listen(addr).await?;
        Ok(())
    }
}

const WRPC_SERVER: &str = "WRPC_SERVER";

impl AsyncService for WrpcService {
    fn ident(self: Arc<Self>) -> &'static str {
        WRPC_SERVER
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { self.run().await.map_err(|err| AsyncServiceError::Service(format!("wRPC error: `{err}`"))) })
    }

    fn signal_exit(self: Arc<Self>) {
        self.server.stop().unwrap_or_else(|err| log_trace!("wRPC unable to signal shutdown: `{err}`"));
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move {
            self.server.join().await.map_err(|err| AsyncServiceError::Service(format!("wRPC error: `{err}`")))?;
            Ok(())
        })
    }
}
