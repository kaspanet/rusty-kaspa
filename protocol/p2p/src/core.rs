use crate::pb::{
    kaspad_message::Payload as KaspadMessagePayload, p2p_client::P2pClient as ProtoP2pClient, p2p_server::P2p as ProtoP2p,
    p2p_server::P2pServer as ProtoP2pServer, KaspadMessage,
};
use futures::FutureExt;
use kaspa_core::{debug, error, info, trace, warn};
use std::net::ToSocketAddrs;
use std::pin::Pin;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use thiserror::Error;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio::sync::{Mutex, RwLock};
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::StreamExt;
use tonic::transport::{Error as TonicError, Server as TonicServer};
use tonic::{Request, Response, Status as TonicStatus, Streaming};
use uuid::Uuid;

#[repr(u8)]
#[derive(Debug, Copy, Clone)]
pub enum KaspadMessagePayloadType {
    Addresses = 0,
    Block,
    Transaction,
    BlockLocator,
    RequestAddresses,
    RequestRelayBlocks,
    RequestTransactions,
    IbdBlock,
    InvRelayBlock,
    InvTransactions,
    Ping,
    Pong,
    Verack,
    Version,
    TransactionNotFound,
    Reject,
    PruningPointUtxoSetChunk,
    RequestIbdBlocks,
    UnexpectedPruningPoint,
    IbdBlockLocator,
    IbdBlockLocatorHighestHash,
    RequestNextPruningPointUtxoSetChunk,
    DonePruningPointUtxoSetChunks,
    IbdBlockLocatorHighestHashNotFound,
    BlockWithTrustedData,
    DoneBlocksWithTrustedData,
    RequestPruningPointAndItsAnticone,
    BlockHeaders,
    RequestNextHeaders,
    DoneHeaders,
    RequestPruningPointUtxoSet,
    RequestHeaders,
    RequestBlockLocator,
    PruningPoints,
    RequestPruningPointProof,
    PruningPointProof,
    Ready,
    BlockWithTrustedDataV4,
    TrustedData,
    RequestIbdChainBlockLocator,
    IbdChainBlockLocator,
    RequestAnticone,
    RequestNextPruningPointAndItsAnticoneBlocks,
}

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
    connection_manager: ConnectionManager,

    /// Map of currently active peers
    active_peers: RwLock<HashMap<Uuid, Arc<Router>>>,
}

impl Adaptor {
    pub(crate) fn new(server_termination: Option<OneshotSender<()>>, connection_manager: ConnectionManager) -> Self {
        Self { _server_termination: server_termination, connection_manager, active_peers: RwLock::new(HashMap::new()) }
    }

    /// Creates a P2P adaptor with only client-side support. Typical Kaspa nodes should use `Adaptor::bidirectional_connection`
    pub fn client_connection_only(initializer: Arc<dyn ConnectionInitializer>) -> Arc<Self> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_manager = ConnectionManager::new(hub_sender);
        let adaptor = Arc::new(Adaptor::new(None, connection_manager));
        adaptor.clone().start_hub_event_loop(hub_receiver, initializer);
        adaptor
    }

    /// Creates a bidirectional P2P adaptor with both a server serving at `serve_address` and with client support
    pub fn bidirectional_connection(
        serve_address: String,
        initializer: Arc<dyn ConnectionInitializer>,
    ) -> Result<Arc<Self>, TonicError> {
        let (hub_sender, hub_receiver) = mpsc_channel(128);
        let connection_manager = ConnectionManager::new(hub_sender);
        let server_termination = connection_manager.serve(serve_address)?;
        let adaptor = Arc::new(Adaptor::new(Some(server_termination), connection_manager));
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
                                router.identity,
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
        self.connection_manager.connect_with_retry(peer_address, 16, Duration::from_secs(2)).await.map(|r| r.identity())
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

/// Manages Router creation for both server and client-side new connections
#[derive(Clone)]
pub struct ConnectionManager {
    /// Cloned on each new connection so that routers can communicate with a central hub
    hub_sender: MpscSender<HubEvent>,
}

impl ConnectionManager {
    pub(crate) fn new(hub_sender: MpscSender<HubEvent>) -> Self {
        Self { hub_sender }
    }

    /// Launches a P2P server listener loop
    pub(crate) fn serve(&self, serve_address: String) -> Result<OneshotSender<()>, TonicError> {
        info!("P2P, Start Listener, ip & port: {:?}", serve_address);
        let (termination_sender, termination_receiver) = oneshot_channel::<()>();
        let connection_manager = self.clone();
        tokio::spawn(async move {
            debug!("P2P, Listener starting, ip & port: {:?}....", serve_address);
            let proto_server = ProtoP2pServer::new(connection_manager)
                .accept_compressed(tonic::codec::CompressionEncoding::Gzip)
                .send_compressed(tonic::codec::CompressionEncoding::Gzip);

            let serve_result = TonicServer::builder()
                .add_service(proto_server)
                .serve_with_shutdown(serve_address.to_socket_addrs().unwrap().next().unwrap(), termination_receiver.map(drop))
                .await;
            match serve_result {
                Ok(_) => debug!("P2P, Server stopped, ip & port: {:?}", serve_address),
                Err(err) => panic!("P2P, Server stopped with error: {err:?}, ip & port: {serve_address:?}"),
            }
        });
        Ok(termination_sender)
    }

    /// Connect to a new peer
    pub(crate) async fn connect(&self, peer_address: String) -> Result<Arc<Router>, TonicError> {
        let channel = tonic::transport::Endpoint::new(peer_address)?
            .timeout(Duration::from_millis(Self::communication_timeout()))
            .connect_timeout(Duration::from_millis(Self::connect_timeout()))
            .tcp_keepalive(Some(Duration::from_millis(Self::keep_alive())))
            .connect()
            .await?;

        let mut client = ProtoP2pClient::new(channel)
            .send_compressed(tonic::codec::CompressionEncoding::Gzip)
            .accept_compressed(tonic::codec::CompressionEncoding::Gzip);

        let (outgoing_route, tonic_receiver) = mpsc_channel(Self::incoming_network_channel_size());
        let incoming_stream = client.message_stream(ReceiverStream::new(tonic_receiver)).await.unwrap().into_inner();

        Ok(Router::new(self.hub_sender.clone(), incoming_stream, outgoing_route).await)
    }

    /// Connect to a new peer with `retry_attempts` retries and `retry_interval` duration between each attempt
    pub(crate) async fn connect_with_retry(
        &self,
        address: String,
        retry_attempts: u8,
        retry_interval: Duration,
    ) -> Option<Arc<Router>> {
        for counter in 0..retry_attempts {
            if let Ok(router) = self.connect(address.clone()).await {
                debug!("P2P, Client connected, ip & port: {:?}", address);
                return Some(router);
            } else {
                // Asynchronously sleep `retry_interval` time before retrying
                tokio::time::sleep(retry_interval).await;
                if counter % 2 == 0 {
                    debug!("P2P, Client connect retry #{}, ip & port: {:?}", counter, address);
                }
            }
        }
        warn!("P2P, Client connection retry #{} - all failed", retry_attempts);
        None
    }

    fn incoming_network_channel_size() -> usize {
        128
    }

    fn outgoing_network_channel_size() -> usize {
        128
    }

    fn communication_timeout() -> u64 {
        10_000
    }

    fn keep_alive() -> u64 {
        10_000
    }

    fn connect_timeout() -> u64 {
        10_000
    }
}

#[tonic::async_trait]
impl ProtoP2p for ConnectionManager {
    type MessageStreamStream = Pin<Box<dyn futures::Stream<Item = Result<KaspadMessage, TonicStatus>> + Send + 'static>>;

    /// Handle the new arriving **server** connections
    async fn message_stream(
        &self,
        request: Request<Streaming<KaspadMessage>>,
    ) -> Result<Response<Self::MessageStreamStream>, TonicStatus> {
        // Build the in/out pipes
        let (outgoing_route, tonic_receiver) = mpsc_channel(Self::outgoing_network_channel_size());
        let incoming_stream = request.into_inner();

        // Build the router object
        // NOTE: No need to explicitly handle the returned router, it will internally be sent to the central Hub
        let _router = Router::new(self.hub_sender.clone(), incoming_stream, outgoing_route).await;

        // Give tonic a receiver stream (messages sent to it will be forwarded to the network peer)
        Ok(Response::new(Box::pin(ReceiverStream::new(tonic_receiver).map(Ok)) as Self::MessageStreamStream))
    }
}

#[derive(Debug)]
struct RouterStateSignals {
    /// Used on router init to signal the router receive loop to start listening
    start_signal: Option<OneshotSender<()>>,

    /// Used on router close to signal the router receive loop to exit
    shutdown_signal: Option<OneshotSender<()>>,
}

#[derive(Debug)]
pub struct Router {
    /// Internal identity of this peer
    identity: Uuid,

    /// Routing map for mapping messages to subscribed flows
    routing_map: RwLock<HashMap<u8, MpscSender<KaspadMessage>>>,

    /// The outgoing route for sending messages to this peer
    outgoing_route: MpscSender<KaspadMessage>,

    /// A channel sender for internal event management. Used to send information from each router to a central hub object
    hub_sender: MpscSender<HubEvent>,

    /// Used for managing router mutable state
    state: Mutex<RouterStateSignals>,
}

impl Router {
    pub(crate) async fn new(
        hub_sender: MpscSender<HubEvent>,
        incoming_stream: Streaming<KaspadMessage>,
        outgoing_route: MpscSender<KaspadMessage>,
    ) -> Arc<Self> {
        let (start_sender, start_receiver) = oneshot_channel();
        let (shutdown_sender, shutdown_receiver) = oneshot_channel();

        let router = Arc::new(Router {
            identity: Uuid::new_v4(),
            routing_map: RwLock::new(HashMap::new()),
            outgoing_route,
            hub_sender,
            state: Mutex::new(RouterStateSignals { start_signal: Some(start_sender), shutdown_signal: Some(shutdown_sender) }),
        });

        let router_clone = router.clone();
        // Start the router receive loop
        tokio::spawn(async move {
            // Wait for a start signal before entering the receive loop
            let _ = start_receiver.await;

            // Transform the shutdown signal receiver to a stream
            let shutdown_stream = Box::pin(async_stream::stream! {
                  let _ = shutdown_receiver.await;
                  yield None;
            });

            // Merge the incoming stream with the shutdown stream so that they can be handled within the same loop
            let mut merged_stream = incoming_stream.map(Some).merge(shutdown_stream);
            while let Some(Some(res)) = merged_stream.next().await {
                match res {
                    Ok(msg) => {
                        trace!("P2P, Router receive loop - got message: {:?}, router-id: {}", msg, router.identity);
                        if !(router.route_to_flow(msg).await) {
                            debug!("P2P, Router receive loop - no route for message - exiting loop, router-id: {}", router.identity);
                            break;
                        }
                    }
                    Err(err) => {
                        warn!("P2P, Router receive loop - network error: {:?}, router-id: {}", err, router.identity);
                        break;
                    }
                }
            }
            router.close().await;
            debug!("P2P, Router receive loop - exited, router-id: {}, {}", router.identity, Arc::strong_count(&router));
        });

        // Notify the central Hub about the new peer
        router_clone.hub_sender.send(HubEvent::NewPeer(router_clone.clone())).await.unwrap();
        router_clone
    }

    pub fn identity(&self) -> Uuid {
        self.identity
    }

    fn payload_to_u8(payload: &KaspadMessagePayload) -> u8 {
        let res = match payload {
            KaspadMessagePayload::Addresses(_) => KaspadMessagePayloadType::Addresses,
            KaspadMessagePayload::Block(_) => KaspadMessagePayloadType::Block,
            KaspadMessagePayload::Transaction(_) => KaspadMessagePayloadType::Transaction,
            KaspadMessagePayload::BlockLocator(_) => KaspadMessagePayloadType::BlockLocator,
            KaspadMessagePayload::RequestAddresses(_) => KaspadMessagePayloadType::RequestAddresses,
            KaspadMessagePayload::RequestRelayBlocks(_) => KaspadMessagePayloadType::RequestRelayBlocks,
            KaspadMessagePayload::RequestTransactions(_) => KaspadMessagePayloadType::RequestTransactions,
            KaspadMessagePayload::IbdBlock(_) => KaspadMessagePayloadType::IbdBlock,
            KaspadMessagePayload::InvRelayBlock(_) => KaspadMessagePayloadType::InvRelayBlock,
            KaspadMessagePayload::InvTransactions(_) => KaspadMessagePayloadType::InvTransactions,
            KaspadMessagePayload::Ping(_) => KaspadMessagePayloadType::Ping,
            KaspadMessagePayload::Pong(_) => KaspadMessagePayloadType::Pong,
            KaspadMessagePayload::Verack(_) => KaspadMessagePayloadType::Verack,
            KaspadMessagePayload::Version(_) => KaspadMessagePayloadType::Version,
            KaspadMessagePayload::TransactionNotFound(_) => KaspadMessagePayloadType::TransactionNotFound,
            KaspadMessagePayload::Reject(_) => KaspadMessagePayloadType::Reject,
            KaspadMessagePayload::PruningPointUtxoSetChunk(_) => KaspadMessagePayloadType::PruningPointUtxoSetChunk,
            KaspadMessagePayload::RequestIbdBlocks(_) => KaspadMessagePayloadType::RequestIbdBlocks,
            KaspadMessagePayload::UnexpectedPruningPoint(_) => KaspadMessagePayloadType::UnexpectedPruningPoint,
            KaspadMessagePayload::IbdBlockLocator(_) => KaspadMessagePayloadType::IbdBlockLocator,
            KaspadMessagePayload::IbdBlockLocatorHighestHash(_) => KaspadMessagePayloadType::IbdBlockLocatorHighestHash,
            KaspadMessagePayload::RequestNextPruningPointUtxoSetChunk(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointUtxoSetChunk
            }
            KaspadMessagePayload::DonePruningPointUtxoSetChunks(_) => KaspadMessagePayloadType::DonePruningPointUtxoSetChunks,
            KaspadMessagePayload::IbdBlockLocatorHighestHashNotFound(_) => {
                KaspadMessagePayloadType::IbdBlockLocatorHighestHashNotFound
            }
            KaspadMessagePayload::BlockWithTrustedData(_) => KaspadMessagePayloadType::BlockWithTrustedData,
            KaspadMessagePayload::DoneBlocksWithTrustedData(_) => KaspadMessagePayloadType::DoneBlocksWithTrustedData,
            KaspadMessagePayload::RequestPruningPointAndItsAnticone(_) => KaspadMessagePayloadType::RequestPruningPointAndItsAnticone,
            KaspadMessagePayload::BlockHeaders(_) => KaspadMessagePayloadType::BlockHeaders,
            KaspadMessagePayload::RequestNextHeaders(_) => KaspadMessagePayloadType::RequestNextHeaders,
            KaspadMessagePayload::DoneHeaders(_) => KaspadMessagePayloadType::DoneHeaders,
            KaspadMessagePayload::RequestPruningPointUtxoSet(_) => KaspadMessagePayloadType::RequestPruningPointUtxoSet,
            KaspadMessagePayload::RequestHeaders(_) => KaspadMessagePayloadType::RequestHeaders,
            KaspadMessagePayload::RequestBlockLocator(_) => KaspadMessagePayloadType::RequestBlockLocator,
            KaspadMessagePayload::PruningPoints(_) => KaspadMessagePayloadType::PruningPoints,
            KaspadMessagePayload::RequestPruningPointProof(_) => KaspadMessagePayloadType::RequestPruningPointProof,
            KaspadMessagePayload::PruningPointProof(_) => KaspadMessagePayloadType::PruningPointProof,
            KaspadMessagePayload::Ready(_) => KaspadMessagePayloadType::Ready,
            KaspadMessagePayload::BlockWithTrustedDataV4(_) => KaspadMessagePayloadType::BlockWithTrustedDataV4,
            KaspadMessagePayload::TrustedData(_) => KaspadMessagePayloadType::TrustedData,
            KaspadMessagePayload::RequestIbdChainBlockLocator(_) => KaspadMessagePayloadType::RequestIbdChainBlockLocator,
            KaspadMessagePayload::IbdChainBlockLocator(_) => KaspadMessagePayloadType::IbdChainBlockLocator,
            KaspadMessagePayload::RequestAnticone(_) => KaspadMessagePayloadType::RequestAnticone,
            KaspadMessagePayload::RequestNextPruningPointAndItsAnticoneBlocks(_) => {
                KaspadMessagePayloadType::RequestNextPruningPointAndItsAnticoneBlocks
            }
        };
        res as u8
    }

    fn incoming_flow_channel_size() -> usize {
        128
    }

    /// Send a signal to start this router's receive loop
    pub async fn start(&self) {
        // Acquire state mutex and send the start signal
        let op = self.state.lock().await.start_signal.take();
        if let Some(signal) = op {
            let _ = signal.send(());
        } else {
            debug!("P2P, Router start was called more than once, router-id: {}", self.identity)
        }
    }

    /// Subscribe to specific message types. This should be used by `ClientInitializer` instances to register application-specific flows
    pub async fn subscribe(&self, msg_types: Vec<KaspadMessagePayloadType>) -> MpscReceiver<KaspadMessage> {
        let (sender, receiver) = mpsc_channel(Self::incoming_flow_channel_size());
        let mut map = self.routing_map.write().await;
        for msg_type in msg_types {
            let msg_id = msg_type as u8;
            match map.insert(msg_id, sender.clone()) {
                Some(_) => {
                    // Overrides an existing route -- panic
                    error!("P2P, Router::subscribe overrides an existing value: {:?}, router-id: {}", msg_type, self.identity);
                    panic!("P2P, Tried to subscribe to an existing route");
                }
                None => {
                    trace!("Router::subscribe - msg_type: {:?} route is registered, router-id:{:?}", msg_type, self.identity);
                }
            }
        }
        receiver
    }

    /// Routes a message coming from the network to the corresponding registered flow
    pub async fn route_to_flow(&self, msg: KaspadMessage) -> bool {
        if msg.payload.is_none() {
            debug!("P2P, Route to flow got empty payload, router-id: {}", self.identity);
            return false;
        }
        let key = Router::payload_to_u8(msg.payload.as_ref().unwrap());
        let map = self.routing_map.read().await;
        if let Some(sender) = map.get(&key) {
            sender.send(msg).await.is_ok()
        } else {
            false
        }
    }

    /// Routes a locally-originated message to the network peer
    pub async fn route_to_network(&self, msg: KaspadMessage) -> bool {
        assert!(msg.payload.is_some(), "Kaspad P2P message should always have a value");
        match self.outgoing_route.send(msg).await {
            Ok(_r) => true,
            Err(_e) => false,
        }
    }

    /// Broadcast a locally-originated message to all active network peers
    pub async fn broadcast(&self, msg: KaspadMessage) -> bool {
        self.hub_sender.send(HubEvent::Broadcast(Box::new(msg))).await.is_ok()
    }

    /// Closes the router, signals exit, and cleans up all resources so that underlying connections will be aborted correctly
    pub async fn close(&self) {
        // Acquire state mutex and send the shutdown signal
        // NOTE: Using a block to drop the lock asap
        {
            let op = self.state.lock().await.shutdown_signal.take();
            if let Some(signal) = op {
                let _ = signal.send(());
            } else {
                // This means the router was already closed
                debug!("P2P, Router close was called more than once, router-id: {}", self.identity);
                return;
            }
        }

        // Drop all flow senders
        self.routing_map.write().await.clear();

        // Downgrade outgoing sender
        self.outgoing_route.downgrade();

        // Send a close notification to the central Hub and downgrade
        self.hub_sender.send(HubEvent::PeerClosing(self.identity)).await.unwrap();
        self.hub_sender.downgrade();
    }
}
