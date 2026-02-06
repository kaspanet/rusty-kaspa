use crate::ConnectionError;
use crate::common::ProtocolError;
use crate::core::hub::Hub;
use crate::core::peer::PeerOutboundType;
use crate::{Router, core::connection_handler::ConnectionHandler};
use kaspa_utils::networking::NetAddress;
use kaspa_utils_tower::counters::TowerConnectionCounters;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::channel as mpsc_channel;
use tokio::sync::oneshot::Sender as OneshotSender;

use super::peer::PeerKey;

/// The main entrypoint for external usage of the P2P library. An impl of this trait is expected on P2P server
/// initialization and will be called on each new (in/out) P2P connection with a corresponding dedicated new router
#[tonic::async_trait]
pub trait ConnectionInitializer: Sync + Send {
    async fn initialize_connection(&self, new_router: Arc<Router>) -> Result<(), ProtocolError>;
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
    pub(crate) fn new(server_termination: Option<OneshotSender<()>>, connection_handler: ConnectionHandler, hub: Hub) -> Self {
        Self { _server_termination: server_termination, connection_handler, hub }
    }

    /// Creates a P2P adaptor with only client-side support. Typical Kaspa nodes should use `Adaptor::bidirectional`
    pub fn client_only(hub: Hub, initializer: Arc<dyn ConnectionInitializer>, counters: Arc<TowerConnectionCounters>) -> Arc<Self> {
        let (hub_sender, hub_receiver) = mpsc_channel(Self::hub_channel_size());
        let connection_handler = ConnectionHandler::new(hub_sender, initializer.clone(), counters);
        let adaptor = Arc::new(Adaptor::new(None, connection_handler, hub));
        adaptor.hub.clone().start_event_loop(hub_receiver, initializer);
        adaptor
    }

    /// Creates a bidirectional P2P adaptor with a server serving at `serve_address` and with client support
    pub fn bidirectional(
        serve_address: NetAddress,
        hub: Hub,
        initializer: Arc<dyn ConnectionInitializer>,
        counters: Arc<TowerConnectionCounters>,
    ) -> Result<Arc<Self>, ConnectionError> {
        let (hub_sender, hub_receiver) = mpsc_channel(Self::hub_channel_size());
        let connection_handler = ConnectionHandler::new(hub_sender, initializer.clone(), counters);
        let server_termination = connection_handler.serve(serve_address)?;
        let adaptor = Arc::new(Adaptor::new(Some(server_termination), connection_handler, hub));
        adaptor.hub.clone().start_event_loop(hub_receiver, initializer);
        Ok(adaptor)
    }

    /// Connect to a new peer (no retries)
    pub async fn connect_peer(&self, peer_address: String, outbound_type: PeerOutboundType) -> Result<PeerKey, ConnectionError> {
        self.connection_handler.connect_with_retry(peer_address, 1, Default::default(), outbound_type).await.map(|r| r.key())
    }

    /// Connect to a new peer (with params controlling retry behavior)
    pub async fn connect_peer_with_retries(
        &self,
        peer_address: String,
        retry_attempts: u8,
        retry_interval: Duration,
        outbound_type: PeerOutboundType,
    ) -> Result<PeerKey, ConnectionError> {
        self.connection_handler.connect_with_retry(peer_address, retry_attempts, retry_interval, outbound_type).await.map(|r| r.key())
    }

    /// Terminates all peers and cleans up any additional async resources
    pub async fn close(&self) {
        self.terminate_all_peers().await;
    }

    pub fn hub_channel_size() -> usize {
        512
    }
}

/// Expose all public `Hub` methods directly through the `Adaptor`
impl Deref for Adaptor {
    type Target = Hub;

    fn deref(&self) -> &Self::Target {
        &self.hub
    }
}
