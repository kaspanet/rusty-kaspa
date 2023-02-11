use crate::hub::Hub;
use crate::{connection::ConnectionHandler, pb::KaspadMessage, Router};
use kaspa_core::error;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::mpsc::channel as mpsc_channel;
use tokio::sync::oneshot::Sender as OneshotSender;
use tonic::transport::Error as TonicError;
use uuid::Uuid;

#[derive(Error, Debug, Clone)]
pub enum ConnectionError {
    #[error("p2p logical protocol error: {0}")]
    ProtocolError(String),

    #[error("channel is closed")]
    ChannelClosed,
}

/// The main entrypoint for external usage of the P2P library. An impl of this trait is expected on P2P server
/// initialization and will be called on each new (in/out) P2P connection with a corresponding dedicated new router
#[tonic::async_trait]
pub trait ConnectionInitializer: Sync + Send {
    async fn initialize_connection(&self, new_router: Arc<Router>) -> Result<(), ConnectionError>;
}

/// The main object to create for managing a fully-fledged Kaspa P2P peer
pub struct Adaptor {
    //
    // Internal design & resource management: management of active peers was extracted to the `Hub` object
    // in order to decouple the memory usage from the `ConnectionHandler` and avoid indirect reference cycles.
    // This way, when the adaptor drops, the following chain of events is triggered (assuming all peer routers were dropped already):
    // - `self._server_termination` is dropped, making the server listener exit (`ConnectionHandler::serve`) thus releasing the handler -> `hub_sender`
    // - `self.connection_handler` is dropped from the adaptor as well, cleaning the last `hub_sender`
    // - Hub event loop (`Hub::start_event_loop`) exits because all senders were dropped
    // - `self.hub` is dropped
    //
    /// If a server was started, it will get cleaned up when this sender is dropped or invoked
    _server_termination: Option<OneshotSender<()>>,

    /// An object for creating new outbound connections as well as handling new connections coming from a server
    connection_handler: ConnectionHandler,

    /// An object for managing a list of active routers (peers), and allowing them to indirectly interact
    hub: Hub,
}

impl Adaptor {
    pub(crate) fn new(server_termination: Option<OneshotSender<()>>, connection_handler: ConnectionHandler) -> Self {
        Self { _server_termination: server_termination, connection_handler, hub: Hub::new() }
    }

    /// Creates a P2P adaptor with only client-side support. Typical Kaspa nodes should use `Adaptor::bidirectional_connection`
    pub fn client_only(initializer: Arc<dyn ConnectionInitializer>) -> Arc<Self> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_handler = ConnectionHandler::new(hub_sender);
        let adaptor = Arc::new(Adaptor::new(None, connection_handler));
        adaptor.hub.clone().start_event_loop(hub_receiver, initializer);
        adaptor
    }

    /// Creates a bidirectional P2P adaptor with a server serving at `serve_address` and with client support
    pub fn bidirectional(serve_address: String, initializer: Arc<dyn ConnectionInitializer>) -> Result<Arc<Self>, TonicError> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_handler = ConnectionHandler::new(hub_sender);
        let server_termination = connection_handler.serve(serve_address)?;
        let adaptor = Arc::new(Adaptor::new(Some(server_termination), connection_handler));
        adaptor.hub.clone().start_event_loop(hub_receiver, initializer);
        Ok(adaptor)
    }

    /// Connect to a new peer
    pub async fn connect_peer(&self, peer_address: String) -> Option<Uuid> {
        self.connection_handler.connect_with_retry(peer_address, 16, Duration::from_secs(2)).await.map(|r| r.identity())
    }

    /// Send a message to a specific peer
    pub async fn send(&self, peer_id: Uuid, msg: KaspadMessage) -> bool {
        self.hub.send(peer_id, msg).await
    }

    /// Broadcast a message to all peers. Note that broadcast can also be called on a specific router and will lead to the same outcome
    pub async fn broadcast(&self, msg: KaspadMessage) {
        self.hub.broadcast(msg).await
    }

    /// Terminate a specific peer
    pub async fn terminate(&self, peer_id: Uuid) {
        self.hub.terminate(peer_id).await
    }

    /// Terminate all peers
    pub async fn terminate_all_peers(&self) {
        self.hub.terminate_all_peers().await
    }

    /// Returns a list of ids for all currently active peers
    pub async fn get_active_peers(&self) -> Vec<Uuid> {
        self.hub.get_active_peers().await
    }

    /// Terminates all peers and cleans up any additional async resources
    pub async fn close(&self) {
        self.terminate_all_peers().await;
    }
}
