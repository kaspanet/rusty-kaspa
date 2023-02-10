use crate::{connection::ConnectionHandler, pb::KaspadMessage, Router};
use kaspa_core::{debug, error};
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver};
use tokio::sync::oneshot::Sender as OneshotSender;
use tokio::sync::RwLock;
use tonic::transport::Error as TonicError;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) enum HubEvent {
    NewPeer(Arc<Router>),
    PeerClosing(Uuid),
    Broadcast(Box<KaspadMessage>),
}

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
    /// If a server was started, it will get cleaned up when this sender is dropped or invoked
    _server_termination: Option<OneshotSender<()>>,

    /// An object for managing new outbound connections as well as handling new connections coming from a server
    connection_handler: ConnectionHandler,

    /// Map of currently active peers
    active_peers: RwLock<HashMap<Uuid, Arc<Router>>>,
}

impl Adaptor {
    pub(crate) fn new(server_termination: Option<OneshotSender<()>>, connection_handler: ConnectionHandler) -> Self {
        Self { _server_termination: server_termination, connection_handler, active_peers: RwLock::new(HashMap::new()) }
    }

    /// Creates a P2P adaptor with only client-side support. Typical Kaspa nodes should use `Adaptor::bidirectional_connection`
    pub fn client_connection_only(initializer: Arc<dyn ConnectionInitializer>) -> Arc<Self> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_handler = ConnectionHandler::new(hub_sender);
        let adaptor = Arc::new(Adaptor::new(None, connection_handler));
        adaptor.clone().start_hub_event_loop(hub_receiver, initializer);
        adaptor
    }

    /// Creates a bidirectional P2P adaptor with both a server serving at `serve_address` and with client support
    pub fn bidirectional_connection(
        serve_address: String,
        initializer: Arc<dyn ConnectionInitializer>,
    ) -> Result<Arc<Self>, TonicError> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_handler = ConnectionHandler::new(hub_sender);
        let server_termination = connection_handler.serve(serve_address)?;
        let adaptor = Arc::new(Adaptor::new(Some(server_termination), connection_handler));
        adaptor.clone().start_hub_event_loop(hub_receiver, initializer);
        Ok(adaptor)
    }

    /// Starts a loop for receiving central hub events from all peer connections. This mechanism is used for
    /// managing a collection of active peers and for supporting a broadcasting mechanism
    fn start_hub_event_loop(self: Arc<Self>, mut hub_receiver: MpscReceiver<HubEvent>, initializer: Arc<dyn ConnectionInitializer>) {
        tokio::spawn(async move {
            while let Some(new_event) = hub_receiver.recv().await {
                match new_event {
                    HubEvent::NewPeer(new_router) => {
                        match initializer.initialize_connection(new_router.clone()).await {
                            Ok(_) => {
                                self.active_peers.write().await.insert(new_router.identity(), new_router);
                            }
                            Err(err) => {
                                // Ignoring the router
                                debug!("P2P, flow initialization for router-id {:?} failed: {}", new_router.identity(), err);
                            }
                        }
                    }
                    HubEvent::PeerClosing(peer_id) => {
                        if let Some(router) = self.active_peers.write().await.remove(&peer_id) {
                            debug!(
                                "P2P, Hub event loop, removing peer, router-id: {}, {}",
                                router.identity(),
                                Arc::strong_count(&router)
                            );
                        }
                    }
                    HubEvent::Broadcast(msg) => {
                        self.broadcast(*msg).await;
                    }
                }
            }
        });
    }

    /// Connect to a new peer
    pub async fn connect_peer(&self, peer_address: String) -> Option<Uuid> {
        self.connection_handler.connect_with_retry(peer_address, 16, Duration::from_secs(2)).await.map(|r| r.identity())
    }

    /// Send a message to a specific peer
    pub async fn send(&self, peer_id: Uuid, msg: KaspadMessage) -> bool {
        if let Some(router) = self.active_peers.read().await.get(&peer_id).cloned() {
            router.route_to_network(msg).await
        } else {
            false
        }
    }

    /// Broadcast a message to all peers. Note that broadcast can also be called on a specific router and will lead to the same outcome
    pub async fn broadcast(&self, msg: KaspadMessage) {
        let peers = self.active_peers.read().await;
        for router in peers.values() {
            router.route_to_network(msg.clone()).await;
        }
    }

    /// Terminate a specific peer
    pub async fn terminate(&self, peer_id: Uuid) {
        if let Some(router) = self.active_peers.read().await.get(&peer_id).cloned() {
            router.close().await;
        }
    }

    /// Terminate all peers
    pub async fn terminate_all_peers(&self) {
        let mut peers = self.active_peers.write().await;
        for router in peers.values() {
            router.close().await;
        }
        peers.clear();
    }

    /// Returns a list of ids for all currently active peers
    pub async fn get_active_peers(&self) -> Vec<Uuid> {
        self.active_peers.read().await.keys().copied().collect()
    }
}
