use crate::{connection_handler::ConnectionHandler, manager::Manager};
use kaspa_notify::notifier::Notifier;
use kaspa_rpc_core::{api::rpc::DynRpcService, notify::connection::ChannelConnection, Notification, RpcResult};
use kaspa_utils::networking::NetAddress;
use std::{ops::Deref, sync::Arc};
use tokio::sync::oneshot::Sender as OneshotSender;

pub struct Adaptor {
    /// If a server was started, it will get cleaned up when this sender is dropped or invoked
    _server_termination: Option<OneshotSender<()>>,

    /// An object for handling new connections coming from clients
    connection_handler: Arc<ConnectionHandler>,

    /// An object for managing a list of active connections
    manager: Manager,

    /// The network address of the server
    serve_address: NetAddress,
}

impl Adaptor {
    fn new(
        server_termination: Option<OneshotSender<()>>,
        connection_handler: Arc<ConnectionHandler>,
        manager: Manager,
        serve_address: NetAddress,
    ) -> Self {
        Self { _server_termination: server_termination, connection_handler, manager, serve_address }
    }

    pub fn server(
        serve_address: NetAddress,
        core_service: DynRpcService,
        core_notifier: Arc<Notifier<Notification, ChannelConnection>>,
        max_connections: usize,
    ) -> Arc<Self> {
        let manager = Manager::new(max_connections);
        let connection_handler = Arc::new(ConnectionHandler::new(core_service.clone(), core_notifier, manager.clone()));
        let server_termination = connection_handler.serve(serve_address);
        connection_handler.start();
        Arc::new(Adaptor::new(Some(server_termination), connection_handler, manager, serve_address))
    }

    pub fn serve_address(&self) -> NetAddress {
        self.serve_address
    }

    pub async fn terminate(&self) -> RpcResult<()> {
        self.connection_handler.stop().await
    }
}

/// Expose all public `Manager` methods directly through the `Adaptor`
impl Deref for Adaptor {
    type Target = Manager;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}
