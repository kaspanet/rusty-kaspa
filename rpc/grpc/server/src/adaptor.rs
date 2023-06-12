use crate::{connection_handler::ConnectionHandler, manager::Manager};
use kaspa_notify::notifier::Notifier;
use kaspa_rpc_core::{api::rpc::DynRpcService, notify::connection::ChannelConnection, Notification, RpcResult};
use kaspa_utils::networking::NetAddress;
use std::sync::Arc;
use tokio::sync::oneshot::Sender as OneshotSender;

pub struct Adaptor {
    /// If a server was started, it will get cleaned up when this sender is dropped or invoked
    _server_termination: Option<OneshotSender<()>>,

    /// An object for handling new connections coming from clients
    connection_handler: Arc<ConnectionHandler>,

    /// The network address of the server
    serve_address: NetAddress,
}

impl Adaptor {
    fn new(
        server_termination: Option<OneshotSender<()>>,
        connection_handler: Arc<ConnectionHandler>,
        serve_address: NetAddress,
    ) -> Self {
        Self { _server_termination: server_termination, connection_handler, serve_address }
    }

    pub fn server(
        serve_address: NetAddress,
        core_service: DynRpcService,
        core_notifier: Arc<Notifier<Notification, ChannelConnection>>,
    ) -> Arc<Self> {
        let manager = Manager::new(Self::max_connections());
        let connection_handler = Arc::new(ConnectionHandler::new(core_service.clone(), core_notifier, manager));
        let server_termination = connection_handler.serve(serve_address);
        connection_handler.start();
        Arc::new(Adaptor::new(Some(server_termination), connection_handler, serve_address))
    }

    pub fn serve_address(&self) -> NetAddress {
        self.serve_address
    }

    pub async fn terminate(&self) -> RpcResult<()> {
        self.connection_handler.stop().await
    }

    pub fn max_connections() -> usize {
        24
    }
}
