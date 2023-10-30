use crate::{connection_handler::ConnectionHandler, manager::Manager};
use kaspa_core::debug;
use kaspa_notify::notifier::Notifier;
use kaspa_rpc_core::{api::rpc::DynRpcService, notify::connection::ChannelConnection, Notification, RpcResult};
use kaspa_utils::networking::NetAddress;
use kaspa_utils::tcp_limiter::Limit;
use std::{ops::Deref, sync::Arc};
use tokio::sync::{mpsc::channel as mpsc_channel, oneshot::Sender as OneshotSender};

pub struct Adaptor {
    /// If a server was started, it will get cleaned up when this sender is dropped or invoked
    _server_termination: Option<OneshotSender<()>>,

    /// An object for handling new connections coming from clients
    connection_handler: ConnectionHandler,

    /// An object for managing a list of active connections
    manager: Manager,

    /// The network address of the server
    serve_address: NetAddress,
}

impl Adaptor {
    fn new(
        server_termination: Option<OneshotSender<()>>,
        connection_handler: ConnectionHandler,
        manager: Manager,
        serve_address: NetAddress,
    ) -> Self {
        Self { _server_termination: server_termination, connection_handler, manager, serve_address }
    }

    pub fn server(
        serve_address: NetAddress,
        manager: Manager,
        core_service: DynRpcService,
        core_notifier: Arc<Notifier<Notification, ChannelConnection>>,
        tcp_limit: Option<Arc<Limit>>,
    ) -> Arc<Self> {
        let (manager_sender, manager_receiver) = mpsc_channel(Self::manager_channel_size());
        let connection_handler = ConnectionHandler::new(manager_sender, core_service.clone(), core_notifier);
        let server_termination = connection_handler.serve(serve_address, tcp_limit);
        let adaptor = Arc::new(Adaptor::new(Some(server_termination), connection_handler, manager, serve_address));
        adaptor.manager.clone().start_event_loop(manager_receiver);
        adaptor.start();
        adaptor
    }

    pub fn serve_address(&self) -> NetAddress {
        self.serve_address
    }

    pub fn start(&self) {
        self.connection_handler.start()
    }

    /// Terminates all connections and cleans up any additional async resources
    pub async fn stop(&self) -> RpcResult<()> {
        debug!("GRPC, Stopping the adaptor");
        self.terminate_all_connections();
        self.connection_handler.stop().await?;
        Ok(())
    }

    pub fn manager_channel_size() -> usize {
        128
    }
}

/// Expose all public `Manager` methods directly through the `Adaptor`
impl Deref for Adaptor {
    type Target = Manager;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}
