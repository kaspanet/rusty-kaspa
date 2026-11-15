//! Authenticated direct Kaspa sessions, with bounded relay circuits for DCUtR.
use crate::core::connection_handler::{ConnectionError, ConnectionHandler, Libp2pConnectInfo};
use either::Either;
use futures::StreamExt;
use kaspa_core::{debug, info};
use kaspa_utils::{
    connection::{EndpointAddress, LibP2pAddress, PeerAddress},
    networking::IpAddress,
};
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    core::{Endpoint, transport::PortUse, upgrade::ReadyUpgrade},
    dcutr, identify, identity,
    multiaddr::Protocol,
    noise, relay,
    swarm::{
        ConnectionDenied, ConnectionId, FromSwarm, NetworkBehaviour, NotifyHandler, OneShotHandler, Stream, StreamProtocol,
        SubstreamProtocol, SwarmEvent, THandlerInEvent, THandlerOutEvent, ToSwarm,
        dial_opts::{DialOpts, PeerCondition},
        dummy,
    },
    yamux,
};
use parking_lot::{Mutex, RwLock};
use std::future::Future;
use std::{
    collections::{HashMap, VecDeque},
    convert::Infallible,
    io,
    net::{IpAddr, SocketAddr},
    pin::Pin,
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::{Notify, mpsc, oneshot},
};
use tokio_util::{
    compat::{Compat, FuturesAsyncReadCompatExt},
    sync::CancellationToken,
    task::TaskTracker,
};

pub use kaspa_utils::connection::Libp2pId;

pub const PROTOCOL: StreamProtocol = StreamProtocol::new("/kaspa/p2p/grpc/1");
pub const RELAY_DURATION: Duration = Duration::from_secs(10);
pub const RELAY_BYTES: u64 = 1024 * 1024;
const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(20);
pub const MAX_PENDING_DIALS: usize = 32;

/// Transport setup, authentication and lifecycle failures. Socket errors retain their original source.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    #[error("transport I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("identity decoding failed: {0}")]
    Identity(#[from] identity::DecodingError),
    #[error("Kaspa requires an Ed25519 peer identity: {0}")]
    IdentityType(#[from] identity::OtherVariantError),
    #[error("address signing failed: {0}")]
    Signing(#[from] identity::SigningError),
    #[error("Kaspa requires an inline Ed25519 peer identity")]
    NonInlineIdentity,
    #[error("unsupported transport destination: {0}")]
    InvalidDestination(SocketAddr),
    #[error("libp2p dialing requires a destination relay")]
    MissingRelay,
    #[error("self libp2p connection")]
    SelfConnection,
    #[error("address signing identity mismatch")]
    SigningIdentityMismatch,
    #[error("libp2p not ready")]
    NotReady,
    #[error("libp2p already started")]
    AlreadyStarted,
    #[error("libp2p stopped")]
    Stopped,
    #[error("libp2p command channel closed")]
    ChannelClosed,
    #[error("libp2p operation timed out: {0}")]
    Timeout(#[from] tokio::time::error::Elapsed),
    #[error("libp2p dial failed: {0}")]
    Dial(#[source] libp2p::swarm::DialError),
    #[error("libp2p transport setup failed: {0}")]
    Noise(#[from] noise::Error),
    #[error("cannot bind {listener} listener at {address}: {source}")]
    Listen {
        listener: &'static str,
        address: SocketAddr,
        #[source]
        source: libp2p::core::transport::TransportError<io::Error>,
    },
    #[error("relay reservation listener failed: {0}")]
    Reservation(#[source] libp2p::core::transport::TransportError<io::Error>),
    #[error("{listener} listener closed unexpectedly")]
    ListenerClosed {
        listener: &'static str,
        #[source]
        source: Option<io::Error>,
    },
    #[error("Kaspa stream negotiation failed: {0}")]
    Stream(#[source] libp2p::swarm::StreamUpgradeError<Infallible>),
}

/// Shared transport handle used by the adaptor, P2P service, and connection manager.
/// Sends commands to the driver task and exposes current addresses and shutdown controls.
pub struct Libp2pTransport {
    /// Authenticated Ed25519 public identity for this run, fixed at construction.
    id: Libp2pId,
    /// In-memory private key used to sign this node's advertised addresses.
    signing_key: identity::Keypair,
    /// Current local or observed UDP socket used when constructing address gossip.
    direct: RwLock<Option<SocketAddr>>,
    /// Home relay with an accepted reservation.
    home: RwLock<Option<LibP2pAddress>>,
    /// Serializes requests to the task that owns the Swarm.
    commands: RwLock<Option<mpsc::Sender<Command>>>,
    /// Ensures startup happens only once.
    started: AtomicBool,
    /// Outbound Kaspa sessions, including those whose hub registration is still queued.
    outbound_sessions: Arc<AtomicUsize>,
    /// Wakes the service to reconcile initialization or a changed home relay; no polling is needed.
    address_changed: Notify,
    /// Fatal listener failure, consumed by the P2P service before normal daemon shutdown.
    failure: Mutex<Option<TransportError>>,
    /// Configured QUIC UDP listener port, fixed at construction.
    relay_port: u16,
    /// Maximum established inbound Swarm connections, including relay clients.
    inbound_limit: u32,
    /// Enables actual private candidates in simnet/devnet.
    allow_private: bool,
    /// Cancels the Swarm, handshakes and session tasks.
    pub(crate) cancel: CancellationToken,
    /// Joins all transport-owned tasks during shutdown.
    tasks: TaskTracker,
}

impl Libp2pTransport {
    /// Creates a fresh in-memory identity for this transport, with a fixed relay port and destination policy.
    pub fn new(relay_port: u16, allow_private: bool) -> Result<Self, TransportError> {
        let signing_key = identity::Keypair::generate_ed25519();
        let id = public_id(signing_key.public().to_peer_id())?;
        Ok(Self {
            id,
            signing_key,
            direct: Default::default(),
            home: Default::default(),
            commands: Default::default(),
            started: AtomicBool::new(false),
            outbound_sessions: Arc::new(AtomicUsize::new(0)),
            address_changed: Notify::new(),
            failure: Mutex::new(None),
            relay_port,
            inbound_limit: 16,
            allow_private,
            cancel: CancellationToken::new(),
            tasks: TaskTracker::new(),
        })
    }

    /// Sets the maximum number of established incoming libp2p connections before startup.
    pub fn with_inbound_limit(mut self, inbound_limit: u32) -> Self {
        self.inbound_limit = inbound_limit;
        self
    }

    pub fn spawn(&self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        self.tasks.spawn(task);
    }

    pub async fn cancelled(&self) {
        self.cancel.cancelled().await;
    }

    pub fn id(&self) -> Libp2pId {
        self.id
    }

    pub fn is_ready(&self) -> bool {
        self.commands.read().is_some()
    }

    pub fn allow_private(&self) -> bool {
        self.allow_private
    }

    pub fn relay_port(&self) -> u16 {
        self.relay_port
    }

    pub async fn address_changed(&self) {
        self.address_changed.notified().await;
    }

    pub fn take_failure(&self) -> Option<TransportError> {
        self.failure.lock().take()
    }

    fn set_home_relay(&self, relay: Option<LibP2pAddress>) {
        let mut home = self.home.write();
        if *home != relay {
            *home = relay;
            self.address_changed.notify_one();
        }
    }

    pub fn is_self_relay(&self, address: SocketAddr) -> bool {
        self.relay_port() == address.port() && self.direct.read().is_some_and(|direct| direct.ip() == address.ip())
    }

    pub fn endpoint_address(&self) -> Option<EndpointAddress> {
        if !self.is_ready() {
            return None;
        }
        Some(EndpointAddress { id: self.id(), relay: *self.home.read() })
    }

    /// Locally learned candidate used when constructing our advertised address.
    pub fn direct_address(&self) -> Option<SocketAddr> {
        if !self.is_ready() {
            return None;
        }
        *self.direct.read()
    }

    /// Counts outbound Kaspa sessions independently of asynchronous hub registration.
    pub fn outbound_sessions(&self) -> usize {
        self.outbound_sessions.load(Ordering::Relaxed)
    }

    pub fn sign_address(&self, address: &PeerAddress, network: &str) -> Result<Vec<u8>, TransportError> {
        if Some(self.id) != address.endpoint_id() {
            return Err(TransportError::SigningIdentityMismatch);
        }
        Ok(self.signing_key.sign(&address.signing_bytes(network))?)
    }

    fn sender(&self) -> Result<mpsc::Sender<Command>, TransportError> {
        self.commands.read().clone().ok_or(TransportError::NotReady)
    }

    pub(crate) fn start(self: &Arc<Self>, listen_ip: IpAddress, handler: ConnectionHandler) -> Result<(), TransportError> {
        if self.cancel.is_cancelled() {
            return Err(TransportError::Stopped);
        }
        if self.started.swap(true, Ordering::Relaxed) {
            return Err(TransportError::AlreadyStarted);
        }
        let (driver, commands) = match Driver::new(self.clone(), listen_ip, handler) {
            Ok(initialized) => initialized,
            Err(error) => {
                self.started.store(false, Ordering::Relaxed);
                return Err(error);
            }
        };
        let state = self.clone();
        self.spawn(async move {
            driver.run(commands).await;

            // Cleanup after driver exit
            state.commands.write().take();
            state.home.write().take();
        });
        Ok(())
    }

    pub(crate) async fn connect(&self, record: &EndpointAddress) -> Result<Libp2pStream, ConnectionError> {
        validate_endpoint(record, self.allow_private())?;
        if self.id() == record.id {
            return Err(TransportError::SelfConnection.into());
        }
        let (tx, rx) = oneshot::channel();
        self.sender()?.send(Command::Dial(record.clone(), tx)).await.map_err(|_| TransportError::ChannelClosed)?;
        tokio::select! {
            _ = self.cancelled() => Err(TransportError::Stopped.into()),
            result = tokio::time::timeout(ATTEMPT_TIMEOUT, rx) => result.map_err(TransportError::Timeout)?.map_err(|_| TransportError::ChannelClosed)?
        }
    }

    pub async fn register_relay(&self, relay: LibP2pAddress) -> Result<(), ConnectionError> {
        validate_destination(relay.address, self.allow_private())?;
        if self.is_self_relay(relay.address) {
            return Err(TransportError::SelfConnection.into());
        }
        let (tx, rx) = oneshot::channel();
        self.sender()?.send(Command::RegisterRelay(relay, peer_id(relay.id)?, tx)).await.map_err(|_| TransportError::ChannelClosed)?;
        tokio::select! {
            _ = self.cancelled() => Err(TransportError::Stopped.into()),
            result = tokio::time::timeout(Duration::from_secs(15), rx) => result.map_err(TransportError::Timeout)?.map_err(|_| TransportError::ChannelClosed)?
        }
    }

    pub fn has_relay_reservation(&self, relay: LibP2pAddress) -> bool {
        *self.home.read() == Some(relay)
    }

    pub async fn remove_relay(&self, relay: LibP2pAddress) {
        if let Ok(sender) = self.sender() {
            let _ = sender.send(Command::RemoveRelay(relay)).await;
        }
    }

    pub async fn shutdown(&self) {
        self.cancel.cancel();
        self.tasks.close();
        self.tasks.wait().await;
        self.commands.write().take();
        self.home.write().take();
    }
}

type Reply<T> = oneshot::Sender<Result<T, ConnectionError>>;

enum Command {
    Dial(EndpointAddress, Reply<Libp2pStream>),
    RegisterRelay(LibP2pAddress, PeerId, Reply<()>),
    RemoveRelay(LibP2pAddress),
}

/// Tracks a peer dial until an authenticated direct stream is returned or the attempt expires.
struct PendingDial {
    relay: Option<LibP2pAddress>,
    reply: Reply<Libp2pStream>,
    connection: Option<ConnectionId>,
    /// Circuit dial ID, retained through establishment until attempt cleanup.
    circuit: Option<ConnectionId>,
}

/// Tracks registration with the selected home relay and its listener for incoming relay circuits.
struct Reservation {
    relay: LibP2pAddress,
    peer: PeerId,
    listener: Option<libp2p::core::transport::ListenerId>,
    connection: Option<ConnectionId>,
    reply: Option<Reply<()>>,
}

/// Metadata and session cancellation for an established direct QUIC connection.
struct TransportConnection {
    peer: PeerId,
    address: SocketAddr,
    cancel: CancellationToken,
    /// Weak handles let connection loss release slots even while tonic or a caller retains a stream.
    sessions: Vec<Weak<SessionHandle>>,
}

impl Drop for TransportConnection {
    fn drop(&mut self) {
        self.cancel.cancel();
        for session in self.sessions.iter().filter_map(Weak::upgrade) {
            session.close();
        }
    }
}

/// Owns the Swarm, connections, pending dials, and relay reservation in one async task.
/// Processes commands from the shared transport handle and reacts to network events.
struct Driver {
    state: Arc<Libp2pTransport>,
    swarm: Swarm<Behaviour>,
    handler: ConnectionHandler,
    pending: HashMap<PeerId, PendingDial>,
    connections: HashMap<ConnectionId, TransportConnection>,
    /// Dials initiated by libp2p behaviours, including relay connections that outlive a caller's request.
    dialing: HashMap<ConnectionId, PeerId>,
    reservation: Option<Reservation>,
    quic_listener: libp2p::core::transport::ListenerId,
}

impl Driver {
    /// Configures the QUIC Swarm, relay and hole-punching behaviours, and Kaspa stream handling.
    /// Creates its listener and command channel.
    fn new(
        state: Arc<Libp2pTransport>,
        listen_ip: IpAddress,
        handler: ConnectionHandler,
    ) -> Result<(Self, mpsc::Receiver<Command>), TransportError> {
        let listen = SocketAddr::new(listen_ip.into(), state.relay_port);
        let local = state.signing_key.public().to_peer_id();
        let mut swarm = SwarmBuilder::with_existing_identity(state.signing_key.clone())
            .with_tokio()
            .with_quic()
            .with_relay_client(noise::Config::new, yamux::Config::default)?
            .with_behaviour(|key, relay_client| Behaviour {
                relay_client,
                relay_server: relay::Behaviour::new(
                    local,
                    relay::Config {
                        max_reservations: 256,
                        max_circuits: 256,
                        max_circuit_duration: RELAY_DURATION,
                        max_circuit_bytes: RELAY_BYTES,
                        ..Default::default()
                    },
                ),
                identify: identify::Behaviour::new(identify::Config::new("kaspa/p2p/1".into(), key.public()).with_cache_size(0)),
                dcutr: dcutr::Behaviour::new(local),
                kaspa: KaspaStreams::default(),
                limits: libp2p::connection_limits::Behaviour::new(
                    libp2p::connection_limits::ConnectionLimits::default()
                        .with_max_pending_incoming_per_source(Some(32))
                        .with_max_pending_outgoing(Some(32))
                        .with_max_established_incoming(Some(state.inbound_limit))
                        .with_max_established(Some(512))
                        .with_max_established_per_peer(Some(4)),
                ),
            })
            .unwrap_or_else(|never| match never {})
            .build();
        let relay_address = listen;
        if relay_address.port() == 0 {
            return Err(TransportError::InvalidDestination(listen));
        }
        // Keep one QUIC socket: libp2p selects among listeners by destination,
        // which can otherwise change the source port observed during punching.
        let quic_listener = swarm.listen_on(multiaddr(relay_address)).map_err(|source| TransportError::Listen {
            listener: "QUIC",
            address: relay_address,
            source,
        })?;
        let (sender, commands) = mpsc::channel(64);
        *state.commands.write() = Some(sender);
        info!("libp2p endpoint started: {}", hex::encode(state.id()));
        let driver = Self {
            state,
            swarm,
            handler,
            pending: HashMap::new(),
            connections: HashMap::new(),
            dialing: HashMap::new(),
            reservation: None,
            quic_listener,
        };
        Ok((driver, commands))
    }

    /// Drives networking and transport requests until shutdown, then cancels active sessions.
    async fn run(mut self, mut commands: mpsc::Receiver<Command>) {
        let mut ticker = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = self.state.cancelled() => break,
                command = commands.recv() => if let Some(command) = command { self.command(command); } else { break; },
                event = self.swarm.select_next_some() => self.event(event),
                _ = ticker.tick() => self.cleanup_cancelled(),
            }
        }
        self.connections.clear();
    }

    /// Handles peer connection requests and home-relay reservation changes.
    fn command(&mut self, command: Command) {
        match command {
            Command::Dial(record, reply) => {
                // A caller may have timed out since the last cleanup tick. Let
                // its next route proceed immediately instead of rejecting it as
                // a duplicate and charging the usable route for that failure.
                self.cleanup_cancelled();
                let peer = match peer_id(record.id) {
                    Ok(peer) => peer,
                    Err(error) => {
                        let _ = reply.send(Err(error.into()));
                        return;
                    }
                };
                if self.pending.len() >= MAX_PENDING_DIALS || self.pending.contains_key(&peer) {
                    let _ = reply.send(Err(ConnectionError::AtCapacity));
                    return;
                }
                self.pending.insert(peer, PendingDial { relay: record.relay, reply, connection: None, circuit: None });
                // Reuse a live direct QUIC connection to open the Kaspa stream;
                // otherwise, use the destination's relay to coordinate hole punching.
                if let Some((&connection, _)) =
                    self.connections.iter().find(|(_, direct)| direct.peer == peer && !direct.cancel.is_cancelled())
                {
                    self.open(peer, connection);
                } else {
                    self.route(peer);
                }
            }
            Command::RegisterRelay(relay, peer, reply) => {
                if peer == *self.swarm.local_peer_id() {
                    let _ = reply.send(Err(TransportError::SelfConnection.into()));
                    return;
                }
                self.remove_registration();
                self.reservation = Some(Reservation { relay, peer, listener: None, connection: None, reply: Some(reply) });
                self.start_registration();
            }
            Command::RemoveRelay(relay) => {
                if self.reservation.as_ref().is_some_and(|reservation| reservation.relay == relay) {
                    self.remove_registration();
                }
            }
        }
    }

    /// Updates addresses, connections, and relay state from Swarm events and advances waiting Kaspa dials.
    fn event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            // Remember libp2p-initiated dials so waiting peers do not start duplicate relay connections.
            SwarmEvent::Dialing { peer_id: Some(peer), connection_id } => {
                self.dialing.insert(connection_id, peer);
            }
            // Seed hole punching and our address record from usable listener addresses,
            // so dials can start before a peer reports our observed address.
            SwarmEvent::NewListenAddr { listener_id, address } => {
                if listener_id == self.quic_listener {
                    info!("libp2p relay listener ready on {address}");
                }
                // Circuit listeners describe routes through relays, not direct UDP candidates.
                if !is_relayed(&address)
                    && let Some(socket) = socket(&address)
                    && validate_destination(socket, self.state.allow_private()).is_ok()
                {
                    // Seed DCUtR before marking the address external: Identify, libp2p's peer-info
                    // exchange, suppresses later candidate notifications for known external addresses.
                    // This notifies DCUtR directly; it does not dispatch the Swarm event arm below.
                    self.swarm.behaviour_mut().dcutr.on_swarm_event(FromSwarm::NewExternalAddrCandidate(
                        libp2p::swarm::behaviour::NewExternalAddrCandidate { addr: &address },
                    ));
                    self.candidate(socket);
                    self.swarm.add_external_address(address);
                    let peers: Vec<_> = self.pending.keys().copied().collect();
                    for peer in peers {
                        self.route(peer);
                    }
                }
            }
            // A peer's observation can reveal our UDP address after NAT translation.
            // Refresh the candidate and resume dials that need it for the hole-punching exchange.
            SwarmEvent::NewExternalAddrCandidate { address } => {
                if !is_relayed(&address)
                    && let Some(socket) = socket(&address)
                    && validate_destination(socket, self.state.allow_private()).is_ok()
                {
                    let mut current = self.state.direct.write();
                    // Accept newer observations within either family, while preferring IPv4 over IPv6.
                    if current.is_none_or(|previous| previous.is_ipv6() || socket.is_ipv4()) {
                        *current = Some(socket);
                    }
                    drop(current);
                    self.state.address_changed.notify_one();
                    self.swarm.add_external_address(address);
                    let peers: Vec<_> = self.pending.keys().copied().collect();
                    for peer in peers {
                        self.route(peer);
                    }
                }
            }
            // Track the direct connection, open any requested Kaspa stream, and resume
            // relay circuit dials or reservations waiting for this connection.
            SwarmEvent::ConnectionEstablished { peer_id, connection_id, endpoint, .. } => {
                self.dialing.remove(&connection_id);
                let address = endpoint.get_remote_address();
                // Open Kaspa streams only after a direct QUIC connection exists, including via hole punching,
                // so sustained block and transaction traffic does not consume relay bandwidth.
                if !endpoint.is_relayed()
                    && let Some(socket) = socket(address)
                {
                    self.connections.insert(
                        connection_id,
                        TransportConnection { peer: peer_id, address: socket, cancel: CancellationToken::new(), sessions: Vec::new() },
                    );
                    self.open(peer_id, connection_id);
                    // The relay connection is shared: resume all destinations that were waiting to open circuits through it.
                    let relay_id = public_id(peer_id).ok();
                    let peers: Vec<_> = self
                        .pending
                        .iter()
                        .filter(|(_, pending)| pending.relay.is_some_and(|relay| Some(relay.id) == relay_id))
                        .map(|(peer, _)| *peer)
                        .collect();
                    for peer in peers {
                        self.route(peer);
                    }
                    if self.reservation.as_ref().is_some_and(|reservation| reservation.connection == Some(connection_id)) {
                        self.start_registration();
                    }
                }
            }
            // Dropping the lost connection closes all its Kaspa streams and releases their outbound slots.
            SwarmEvent::ConnectionClosed { connection_id, .. } => {
                self.connections.remove(&connection_id);
            }
            // Reuse the existing gRPC/Kaspa handshake and message flows over the negotiated stream:
            // return outbound results to their caller, or start an inbound server.
            SwarmEvent::Behaviour(BehaviourEvent::Kaspa((peer, connection, result))) => self.stream(peer, connection, result),
            // Other peers can open circuits to us only after our relay reservation is accepted.
            // Make the accepted route available for advertisement and complete the registration request.
            SwarmEvent::Behaviour(BehaviourEvent::RelayClient(relay::client::Event::ReservationReqAccepted {
                relay_peer_id, ..
            })) => {
                if let Some(reservation) =
                    self.reservation.as_mut().filter(|reservation| reservation.listener.is_some() && reservation.peer == relay_peer_id)
                {
                    self.state.set_home_relay(Some(reservation.relay));
                    if let Some(reply) = reservation.reply.take() {
                        let _ = reply.send(Ok(()));
                        info!("libp2p relay registration succeeded: {}", reservation.relay.address);
                    }
                }
            }
            // The QUIC listener is required for libp2p, so losing it stops the transport.
            // A closed reservation clears the advertised route and allows relay reselection.
            SwarmEvent::ListenerClosed { listener_id, reason, .. } => {
                if listener_id == self.quic_listener {
                    let listener = "QUIC";
                    *self.state.failure.lock() = Some(TransportError::ListenerClosed { listener, source: reason.err() });
                    self.state.cancel.cancel();
                }
                if self.reservation.as_ref().is_some_and(|reservation| reservation.listener == Some(listener_id)) {
                    self.remove_registration();
                }
            }
            // DCUtR results are diagnostic here; ConnectionEstablished starts sessions once a direct path exists.
            SwarmEvent::Behaviour(BehaviourEvent::Dcutr(event)) => debug!("libp2p direct upgrade: {event:?}"),
            // Notify affected callers of dial failure and let other destinations waiting
            // on the same relay try to establish their own circuits.
            SwarmEvent::OutgoingConnectionError { peer_id, connection_id, error, .. } => {
                self.dialing.remove(&connection_id);
                debug!("libp2p dial failed: {error}");
                // Report local limits as AtCapacity so callers can retry without penalizing peers or relays.
                if is_local_capacity_error(&error) {
                    let relay_id = peer_id.and_then(|peer| public_id(peer).ok());
                    let peers: Vec<_> = self
                        .pending
                        .iter()
                        .filter(|(peer, pending)| {
                            Some(**peer) == peer_id || pending.relay.is_some_and(|relay| Some(relay.id) == relay_id)
                        })
                        .map(|(peer, _)| *peer)
                        .collect();
                    for peer in peers {
                        self.finish_dial(peer, Err(ConnectionError::AtCapacity));
                    }
                }
                // DCUtR retries its direct attempts internally, so only registration and
                // circuit failures end their requests here.
                if self
                    .reservation
                    .as_ref()
                    .is_some_and(|reservation| reservation.reply.is_some() && reservation.connection == Some(connection_id))
                {
                    self.fail_registration(dial_error(error));
                } else if let Some(peer) = peer_id
                    && self.pending.get(&peer).is_some_and(|pending| pending.circuit == Some(connection_id))
                {
                    self.finish_dial(peer, Err(dial_error(error)));
                }
                // Retry destinations held back while this relay connection was being established.
                if let Some(relay_id) = peer_id.and_then(|peer| public_id(peer).ok()) {
                    let peers: Vec<_> = self
                        .pending
                        .iter()
                        .filter(|(_, pending)| pending.relay.is_some_and(|relay| relay.id == relay_id) && pending.circuit.is_none())
                        .map(|(peer, _)| *peer)
                        .collect();
                    for peer in peers {
                        self.route(peer);
                    }
                }
            }
            // Identify exchanges and relay serving run inside libp2p behaviours, without driver coordination.
            _ => {}
        }
    }

    /// Connects to the selected relay socket, then requests a reservation for our node.
    fn start_registration(&mut self) {
        let Some(reservation) = self.reservation.as_mut().filter(|reservation| reservation.listener.is_none()) else {
            return;
        };
        // The libp2p relay client can reuse any connection to this PeerId.
        // Confirm the selected socket first, so another port cannot mask a failed dial.
        if self.connections.values().any(|connection| {
            connection.peer == reservation.peer && connection.address == reservation.relay.address && !connection.cancel.is_cancelled()
        }) {
            let route = multiaddr(reservation.relay.address).with(Protocol::P2p(reservation.peer)).with(Protocol::P2pCircuit);
            match self.swarm.listen_on(route) {
                Ok(listener) => reservation.listener = Some(listener),
                Err(error) => {
                    self.fail_registration(TransportError::Reservation(error).into());
                }
            }
        } else {
            // Do not extend this dial with addresses learned by other behaviours.
            let dial = DialOpts::peer_id(reservation.peer)
                .addresses(vec![multiaddr(reservation.relay.address)])
                .condition(PeerCondition::Always)
                .build();
            reservation.connection = Some(dial.connection_id());
            if let Err(error) = self.swarm.dial(dial) {
                self.fail_registration(dial_error(error));
            }
        }
    }

    /// Reports registration failure and removes the home-relay listener and route.
    fn fail_registration(&mut self, error: ConnectionError) {
        if let Some(reservation) = self.remove_registration()
            && let Some(reply) = reservation.reply
        {
            let _ = reply.send(Err(error));
        }
    }

    /// Stops accepting circuits through our home relay and clears its advertised route.
    /// Returns the removed registration so its waiting caller can be notified.
    fn remove_registration(&mut self) -> Option<Reservation> {
        let reservation = self.reservation.take();
        if let Some(listener) = reservation.as_ref().and_then(|reservation| reservation.listener) {
            self.swarm.remove_listener(listener);
        }
        self.state.set_home_relay(None);
        reservation
    }

    /// Updates the preferred local UDP candidate used for Kaspa address advertisements.
    fn candidate(&self, address: SocketAddr) {
        let mut current = self.state.direct.write();
        // Prefer IPv4, retaining the first candidate within the preferred family.
        if current.is_none_or(|previous| previous.is_ipv6() && address.is_ipv4()) {
            *current = Some(address);
            self.state.address_changed.notify_one();
        }
    }

    /// Dials the peer's complete relay route through libp2p's relay client.
    /// Waits for a local address candidate before opening the circuit used for hole punching.
    fn route(&mut self, peer: PeerId) {
        let Some(pending) = self.pending.get(&peer) else {
            return;
        };
        let Some(relay) = pending.relay else {
            return;
        };
        if pending.circuit.is_some() || pending.connection.is_some() {
            return;
        }
        // DCUtR snapshots candidates when the circuit opens.
        if self.state.direct.read().is_none() {
            return;
        }
        let relay_id = match peer_id(relay.id) {
            Ok(id) => id,
            Err(error) => {
                self.finish_dial(peer, Err(error.into()));
                return;
            }
        };
        // Dial this destination's circuit only if either:
        // 1) a live connection to the relay already exists; or
        // 2) neither a tracked relay dial nor another destination's circuit dial is establishing one.
        // Otherwise, wait to avoid overlapping initial relay dials, which libp2p rejects.
        // ConnectionEstablished resumes waiting destinations when the shared connection is ready.
        if !self.connections.values().any(|connection| connection.peer == relay_id && !connection.cancel.is_cancelled())
            && (self.dialing.values().any(|peer| *peer == relay_id)
                || self.pending.iter().any(|(destination, pending)| {
                    *destination != peer && pending.relay.is_some_and(|address| address.id == relay.id) && pending.circuit.is_some()
                }))
        {
            return;
        }

        let route = multiaddr(relay.address).with(Protocol::P2p(relay_id)).with(Protocol::P2pCircuit).with(Protocol::P2p(peer));
        let dial = DialOpts::peer_id(peer).addresses(vec![route]).condition(PeerCondition::Always).build();
        self.pending.get_mut(&peer).unwrap().circuit = Some(dial.connection_id());
        if let Err(error) = self.swarm.dial(dial) {
            self.finish_dial(peer, Err(dial_error(error)));
        }
    }

    /// Requests a Kaspa stream on a direct connection for a waiting outbound peer.
    fn open(&mut self, peer: PeerId, connection: ConnectionId) {
        if let Some(pending) = self.pending.get_mut(&peer)
            && pending.connection.is_none()
        {
            pending.connection = Some(connection);
            self.swarm.behaviour_mut().kaspa.open(peer, connection);
        }
    }

    /// Handles Kaspa stream negotiation results, completing outbound requests or serving inbound sessions.
    fn stream(&mut self, peer: PeerId, connection: ConnectionId, negotiation_result: Result<Stream, TransportError>) {
        // Negotiation can finish after the connection closes; ignore results for connections we no longer own.
        let Some(direct) = self.connections.get_mut(&connection) else {
            return;
        };
        // A matching pending request means we initiated the Kaspa session, regardless of the QUIC connection's direction.
        let outbound = self.pending.get(&peer).is_some_and(|pending| pending.connection == Some(connection));
        let stream = match negotiation_result {
            Ok(stream) => stream,
            Err(error) => {
                // Notify the outbound caller and release its pending dial, rather than leaving it waiting for a timeout.
                if outbound {
                    self.finish_dial(peer, Err(error.into()));
                }
                return;
            }
        };
        // Our identity format requires an inline Ed25519 public key from the authenticated libp2p PeerId.
        let id = match public_id(peer) {
            Ok(id) => id,
            Err(_) => {
                self.swarm.close_connection(connection);
                return;
            }
        };
        // Give each Kaspa stream its own cancellation token and outbound slot. A
        // rejected handshake must not close other sessions sharing this QUIC connection.
        let session =
            Arc::new(SessionHandle::new(direct.cancel.child_token(), outbound.then(|| self.state.outbound_sessions.clone())));
        direct.sessions.retain(|session| session.strong_count() != 0);
        direct.sessions.push(Arc::downgrade(&session));
        let info = Libp2pConnectInfo { address: direct.address, id, request_started: CancellationToken::new(), session };
        // Adapt the stream to Tokio I/O, interrupted by either session closure or loss of its parent connection.
        info!("libp2p peer {} direct connection: {} (QUIC)", hex::encode(id), direct.address);
        let stream = Libp2pStream { io: stream.compat(), cancelled: Box::pin(info.session.cancel.clone().cancelled_owned()), info };
        // Outbound callers perform the client-side gRPC setup; inbound streams need a server here.
        if outbound {
            self.finish_dial(peer, Ok(stream));
        } else {
            let handler = self.handler.clone();
            let cancel = self.state.cancel.clone();
            self.state.spawn(async move {
                if let Err(error) = handler.serve_libp2p(stream, cancel).await {
                    debug!("libp2p incoming Kaspa session failed: {error}");
                }
            });
        }
    }

    /// Completes an outbound attempt and releases its temporary circuit, leaving shared direct connections intact.
    fn finish_dial(&mut self, peer: PeerId, result: Result<Libp2pStream, ConnectionError>) {
        if let Some(pending) = self.pending.remove(&peer) {
            if let Some(circuit) = pending.circuit {
                self.swarm.close_connection(circuit);
            }
            let _ = pending.reply.send(result);
            // Resume another destination waiting on the same relay, so failure or cancellation
            // of the attempt establishing their shared relay connection does not strand the others.
            if let Some(next) = self
                .pending
                .iter()
                .find(|(_, waiting)| {
                    waiting.relay.map(|relay| relay.id) == pending.relay.map(|relay| relay.id)
                        && waiting.connection.is_none()
                        && waiting.circuit.is_none()
                })
                .map(|(peer, _)| *peer)
            {
                self.route(next);
            }
        }
    }

    /// Releases abandoned dials and registrations.
    fn cleanup_cancelled(&mut self) {
        // The caller owns the timeout. Dropping its reply receiver cancels the attempt.
        let cancelled: Vec<_> = self.pending.iter().filter(|(_, pending)| pending.reply.is_closed()).map(|(peer, _)| *peer).collect();
        for peer in cancelled {
            self.finish_dial(peer, Err(TransportError::ChannelClosed.into()));
        }
        if self.reservation.as_ref().is_some_and(|reservation| reservation.reply.as_ref().is_some_and(oneshot::Sender::is_closed)) {
            self.remove_registration();
        }
    }
}

/// Combines relay serving and registration, peer identification, hole punching,
/// Kaspa streams, and connection limits for the Swarm.
#[derive(NetworkBehaviour)]
struct Behaviour {
    relay_client: relay::client::Behaviour,
    relay_server: relay::Behaviour,
    identify: identify::Behaviour,
    dcutr: dcutr::Behaviour,
    kaspa: KaspaStreams,
    limits: libp2p::connection_limits::Behaviour,
}

type KaspaStreamEvent = (PeerId, ConnectionId, Result<Stream, TransportError>);
type Upgrade = ReadyUpgrade<StreamProtocol>;
type KaspaHandler = Either<OneShotHandler<Upgrade, Upgrade, Stream>, dummy::ConnectionHandler>;

/// Negotiates Kaspa streams only on authenticated direct connections.
/// Associates each negotiated stream with its owning connection.
#[derive(Default)]
struct KaspaStreams {
    events: VecDeque<ToSwarm<KaspaStreamEvent, Either<Upgrade, Infallible>>>,
}

impl KaspaStreams {
    /// Direct connections negotiate the Kaspa protocol and deliver streams for the normal gRPC session.
    /// Relayed connections offer no Kaspa protocol, keeping block and transaction traffic off relay circuits;
    /// other libp2p behaviours can still use those circuits to coordinate hole punching.
    fn handler(direct: bool) -> KaspaHandler {
        if direct {
            Either::Left(OneShotHandler::new(SubstreamProtocol::new(ReadyUpgrade::new(PROTOCOL), ()), Default::default()))
        } else {
            Either::Right(dummy::ConnectionHandler)
        }
    }

    fn open(&mut self, peer: PeerId, connection: ConnectionId) {
        self.events.push_back(ToSwarm::NotifyHandler {
            peer_id: peer,
            handler: NotifyHandler::One(connection),
            event: Either::Left(ReadyUpgrade::new(PROTOCOL)),
        });
    }
}

impl NetworkBehaviour for KaspaStreams {
    type ConnectionHandler = KaspaHandler;
    type ToSwarm = KaspaStreamEvent;

    fn handle_established_inbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        local: &Multiaddr,
        remote: &Multiaddr,
    ) -> Result<KaspaHandler, ConnectionDenied> {
        Ok(Self::handler(!is_relayed(local) && !is_relayed(remote)))
    }

    fn handle_established_outbound_connection(
        &mut self,
        _: ConnectionId,
        _: PeerId,
        address: &Multiaddr,
        _: Endpoint,
        _: PortUse,
    ) -> Result<KaspaHandler, ConnectionDenied> {
        Ok(Self::handler(!is_relayed(address)))
    }

    fn on_swarm_event(&mut self, _: FromSwarm) {}

    fn on_connection_handler_event(&mut self, peer: PeerId, connection: ConnectionId, event: THandlerOutEvent<Self>) {
        match event {
            Either::Left(result) => {
                self.events.push_back(ToSwarm::GenerateEvent((peer, connection, result.map_err(TransportError::Stream))))
            }
            Either::Right(never) => match never {},
        }
    }

    fn poll(&mut self, _: &mut Context<'_>) -> Poll<ToSwarm<Self::ToSwarm, THandlerInEvent<Self>>> {
        self.events.pop_front().map_or(Poll::Pending, Poll::Ready)
    }
}

/// Owns one Kaspa stream's cancellation and outbound slot, independently of its shared QUIC connection.
pub(crate) struct SessionHandle {
    pub cancel: CancellationToken,
    outbound_sessions: Option<Arc<AtomicUsize>>,
    closed: AtomicBool,
}

impl SessionHandle {
    fn new(cancel: CancellationToken, outbound_sessions: Option<Arc<AtomicUsize>>) -> Self {
        // Reserve before the handshake and asynchronous Hub registration become visible.
        if let Some(counter) = &outbound_sessions {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        Self { cancel, outbound_sessions, closed: AtomicBool::new(false) }
    }

    pub fn close(&self) {
        // I/O teardown and router cleanup may both close this session; release its slot once.
        if !self.closed.swap(true, Ordering::Relaxed) {
            self.cancel.cancel();
            if let Some(counter) = &self.outbound_sessions {
                counter.fetch_sub(1, Ordering::Relaxed);
            }
        }
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        self.close();
    }
}

/// Closes only this Kaspa stream and releases its slot when the router or response stream ends.
pub(crate) struct SessionCloseGuard(pub Arc<SessionHandle>);

impl Drop for SessionCloseGuard {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Adapts an authenticated direct libp2p stream to tonic's async I/O and connection metadata interfaces.
pub(crate) struct Libp2pStream {
    io: Compat<Stream>,
    cancelled: Pin<Box<tokio_util::sync::WaitForCancellationFutureOwned>>,
    pub info: Libp2pConnectInfo,
}

impl tonic::transport::server::Connected for Libp2pStream {
    type ConnectInfo = Libp2pConnectInfo;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.info.clone()
    }
}

impl Drop for Libp2pStream {
    fn drop(&mut self) {
        self.info.session.close();
    }
}

impl AsyncRead for Libp2pStream {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        if self.cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionAborted, "libp2p session closed")));
        }
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Libp2pStream {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if self.cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::ConnectionAborted, "libp2p session closed")));
        }
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

fn is_local_capacity_error(error: &libp2p::swarm::DialError) -> bool {
    matches!(error, libp2p::swarm::DialError::Denied { cause }
        if cause.downcast_ref::<libp2p::connection_limits::Exceeded>().is_some())
}

fn dial_error(error: libp2p::swarm::DialError) -> ConnectionError {
    if is_local_capacity_error(&error) { ConnectionError::AtCapacity } else { TransportError::Dial(error).into() }
}

fn peer_id(id: Libp2pId) -> Result<PeerId, TransportError> {
    let key = identity::ed25519::PublicKey::try_from_bytes(&id)?;
    Ok(identity::PublicKey::from(key).to_peer_id())
}

fn public_id(peer: PeerId) -> Result<Libp2pId, TransportError> {
    if peer.as_ref().code() != 0 {
        return Err(TransportError::NonInlineIdentity);
    }
    Ok(identity::PublicKey::try_decode_protobuf(peer.as_ref().digest())?.try_into_ed25519()?.to_bytes())
}

pub fn validate_destination(address: SocketAddr, allow_private: bool) -> Result<(), TransportError> {
    let ip = address.ip().to_canonical();
    if address.port() == 0
        || ip.is_unspecified()
        || ip.is_multicast()
        || (!allow_private && !IpAddress::from(ip).is_publicly_routable())
    {
        return Err(TransportError::InvalidDestination(address));
    }
    Ok(())
}

pub fn validate_endpoint(record: &EndpointAddress, allow_private: bool) -> Result<(), TransportError> {
    peer_id(record.id)?;
    let relay = record.relay.ok_or(TransportError::MissingRelay)?;
    peer_id(relay.id)?;
    validate_destination(relay.address, allow_private)
}

fn multiaddr(address: SocketAddr) -> Multiaddr {
    let mut result = Multiaddr::empty();
    match address.ip().to_canonical() {
        IpAddr::V4(ip) => result.push(Protocol::Ip4(ip)),
        IpAddr::V6(ip) => result.push(Protocol::Ip6(ip)),
    }
    result.push(Protocol::Udp(address.port()));
    result.push(Protocol::QuicV1);
    result
}

fn socket(address: &Multiaddr) -> Option<SocketAddr> {
    let mut ip = None;
    let mut port = None;
    for part in address.iter() {
        match part {
            Protocol::Ip4(value) => ip = Some(IpAddr::V4(value)),
            Protocol::Ip6(value) => ip = Some(IpAddr::V6(value)),
            Protocol::Udp(value) | Protocol::Tcp(value) => port = Some(value),
            _ => {}
        }
    }
    Some(SocketAddr::new(ip?.to_canonical(), port?))
}

fn is_relayed(address: &Multiaddr) -> bool {
    address.iter().any(|part| matches!(part, Protocol::P2pCircuit))
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, reason = "Test code is exempt under docs/ARITH-SAFETY.md.")]
mod session_tests {
    use super::*;
    use crate::{
        Adaptor, ConnectionInitializer, Hub, KaspadHandshake, KaspadMessagePayloadType, Router,
        common::ProtocolError,
        convert::model::version::Version,
        make_message,
        pb::{self, kaspad_message::Payload},
    };
    use kaspa_utils::networking::{NetAddress, PeerId as KaspaPeerId};
    use tokio::sync::mpsc;

    /// Test initializer that completes the Kaspa handshake, answers pings, and records received messages.
    struct Initializer {
        node_id: KaspaPeerId,
        received: mpsc::Sender<(SocketAddr, pb::KaspadMessage)>,
    }

    #[tonic::async_trait]
    impl ConnectionInitializer for Initializer {
        async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
            let mut handshake = KaspadHandshake::new(&router);
            router.start();
            let version = Version::new(None, self.node_id, "kaspa-simnet".into(), None, 11);

            let peer: Version = handshake.handshake(version.into()).await?.try_into()?;
            if peer.network != "kaspa-simnet" || peer.protocol_version != 11 {
                return Err(ProtocolError::Other("Kaspa handshake validation failed"));
            }
            router.set_identity(peer.id);
            let mut incoming = router.subscribe(vec![
                KaspadMessagePayloadType::Ping,
                KaspadMessagePayloadType::Pong,
                KaspadMessagePayloadType::Block,
                KaspadMessagePayloadType::Transaction,
                KaspadMessagePayloadType::RequestIbdBlocks,
            ]);
            handshake.exchange_ready_messages().await?;
            let received = self.received.clone();
            tokio::spawn(async move {
                while let Some(message) = incoming.recv().await {
                    if let Some(Payload::Ping(ping)) = &message.payload {
                        let _ = router.enqueue(make_message!(Payload::Pong, pb::PongMessage { nonce: ping.nonce })).await;
                    }
                    if received.send((router.net_address(), message)).await.is_err() {
                        break;
                    }
                }
            });
            Ok(())
        }
    }

    /// Test node fixture keeping its adaptor, transport, message receiver, and temporary identity directory alive.
    struct Node {
        adaptor: Arc<Adaptor>,
        state: Arc<Libp2pTransport>,
        received: mpsc::Receiver<(SocketAddr, pb::KaspadMessage)>,
    }

    fn free_port_pair() -> u16 {
        loop {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            if port < u16::MAX
                && std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok()
                && std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
                && std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok()
            {
                return port;
            }
        }
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while !predicate() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn node_with_inbound_limit(inbound_limit: u32) -> Node {
        let (sent, received) = mpsc::channel(32);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received: sent });
        let port = free_port_pair();
        let listen = NetAddress::from(SocketAddr::from(([127, 0, 0, 1], port + 1)));
        let state = Arc::new(Libp2pTransport::new(listen.port, true).unwrap().with_inbound_limit(inbound_limit));
        let id = state.id();
        let adaptor = Adaptor::client_only(Hub::new(), initializer, Default::default(), state.clone());
        assert_eq!(state.id(), id);
        assert_eq!(state.relay_port(), listen.port);
        assert!(state.allow_private());
        assert!(!state.is_ready());
        adaptor.start_libp2p(listen.ip).unwrap();
        assert_eq!(state.id(), id);
        assert!(state.is_ready());
        wait_until(|| state.direct_address().is_some()).await;
        Node { adaptor, state, received }
    }

    async fn node() -> Node {
        node_with_inbound_limit(16).await
    }

    async fn relay_route(destination: &Node, relay: &Node) -> EndpointAddress {
        let address = SocketAddr::new(relay.state.direct_address().unwrap().ip(), relay.state.relay_port());
        destination.state.register_relay(LibP2pAddress { address, id: relay.state.id() }).await.unwrap();
        destination.state.endpoint_address().unwrap()
    }

    async fn close(node: &Node) {
        tokio::time::timeout(Duration::from_secs(10), node.adaptor.close()).await.unwrap();
        assert!(node.state.tasks.is_empty());
        assert!(!node.state.is_ready());
        assert_eq!(node.state.outbound_sessions(), 0);
        assert!(node.state.take_failure().is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inbound_connection_limit_rejects_excess_relay_clients_and_releases_capacity() {
        let relay = node_with_inbound_limit(1).await;
        let first = node().await;
        let second = node().await;
        let address = LibP2pAddress { address: relay.state.direct_address().unwrap(), id: relay.state.id() };

        first.state.register_relay(address).await.unwrap();
        assert!(first.state.has_relay_reservation(address));
        assert!(second.state.register_relay(address).await.is_err());
        assert!(!second.state.has_relay_reservation(address));

        close(&first).await;
        second.state.register_relay(address).await.unwrap();
        assert!(second.state.has_relay_reservation(address));
        close(&second).await;
        close(&relay).await;
    }

    #[tokio::test]
    async fn router_closure_notifies_all_waiters_without_stopping_transport() {
        let a = node().await;
        let b = node().await;
        let r = node().await;
        let route = relay_route(&b, &r).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        let router = a.adaptor.peers.read().get(&key).unwrap().clone();
        let first = router.closed();
        let second = router.closed();
        assert!(tokio::time::timeout(Duration::from_millis(20), first.clone()).await.is_err());
        a.adaptor.terminate(key).await;
        tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(first, second);
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(1), router.closed()).await.unwrap();
        assert!(router.is_closed());
        wait_until(|| a.state.outbound_sessions() == 0).await;
        assert!(!router.close().await, "closing an already closed router must stay idempotent");
        assert!(a.state.is_ready());
        assert!(!a.state.cancel.is_cancelled());
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn closing_an_extra_stream_preserves_the_established_kaspa_session() {
        let r = node().await;
        let mut a = node().await;
        let b = node().await;
        let route = relay_route(&b, &r).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1 && b.adaptor.active_peers_len() == 1).await;
        let original = a.adaptor.peers.read().get(&key).unwrap().clone();

        // The transport reuses the established QUIC connection. Closing the extra
        // Kaspa stream must leave the original router and its messages working.
        let extra = a.state.connect(&route).await.unwrap();
        assert_eq!(a.state.outbound_sessions(), 2);
        drop(SessionCloseGuard(extra.info.session.clone()));
        drop(extra);
        assert_eq!(a.state.outbound_sessions(), 1);
        assert!(!original.is_closed());
        assert!(a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 901 })).await.unwrap());
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 901 }))));
        assert_eq!(a.state.outbound_sessions(), 1);
        assert!(Arc::ptr_eq(a.adaptor.peers.read().get(&key).unwrap(), &original));

        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[test]
    fn session_closure_is_isolated_and_releases_each_outbound_slot_once() {
        let connection = CancellationToken::new();
        let counter = Arc::new(AtomicUsize::new(0));
        let first = Arc::new(SessionHandle::new(connection.child_token(), Some(counter.clone())));
        let second = Arc::new(SessionHandle::new(connection.child_token(), Some(counter.clone())));
        let inbound = Arc::new(SessionHandle::new(connection.child_token(), None));
        let direct = TransportConnection {
            peer: identity::Keypair::generate_ed25519().public().to_peer_id(),
            address: "127.0.0.1:16512".parse().unwrap(),
            cancel: connection.clone(),
            sessions: vec![Arc::downgrade(&first), Arc::downgrade(&second), Arc::downgrade(&inbound)],
        };
        assert_eq!(counter.load(Ordering::Relaxed), 2);

        drop(SessionCloseGuard(first.clone()));
        first.close();
        drop(first);
        assert_eq!(counter.load(Ordering::Relaxed), 1);
        assert!(!connection.is_cancelled());
        assert!(!second.cancel.is_cancelled());
        assert!(!inbound.cancel.is_cancelled());

        // Connection loss or shutdown must also release slots for streams still
        // held by a caller. Dropping those streams later must not release them twice.
        drop(direct);
        assert!(connection.is_cancelled());
        assert_eq!(counter.load(Ordering::Relaxed), 0);
        assert!(second.cancel.is_cancelled());
        assert!(inbound.cancel.is_cancelled());
        drop(second);
        drop(inbound);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rejecting_a_duplicate_handshake_preserves_the_established_session() {
        // Stop after Version exchange, like the normal duplicate check before Ready.
        struct RejectDuplicate(Arc<Adaptor>);
        #[tonic::async_trait]
        impl ConnectionInitializer for RejectDuplicate {
            async fn initialize_connection(&self, router: Arc<Router>) -> Result<(), ProtocolError> {
                let mut handshake = KaspadHandshake::new(&router);
                router.start();
                let version = Version::new(None, uuid::Uuid::new_v4().into(), "kaspa-simnet".into(), None, 11);
                let peer: Version = handshake.handshake(version.into()).await?.try_into()?;
                router.set_identity(peer.id);
                assert!(self.0.has_peer(router.key()));
                Err(ProtocolError::PeerAlreadyExists(router.key()))
            }
        }

        let r = node().await;
        let mut a = node().await;
        let b = node().await;
        let route = relay_route(&b, &r).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1 && b.adaptor.active_peers_len() == 1).await;
        let original = a.adaptor.peers.read().get(&key).unwrap().clone();
        let (hub, _events) = mpsc::channel(1);
        let handler = ConnectionHandler::new(hub, Arc::new(RejectDuplicate(a.adaptor.clone())), Default::default(), a.state.clone());

        // Bypass the adaptor's early duplicate check to exercise handshake failure
        // after the second stream has actually opened on the existing connection.
        assert!(matches!(
            handler.connect_libp2p(&route).await,
            Err(ConnectionError::ProtocolError(ProtocolError::PeerAlreadyExists(_)))
        ));
        assert_eq!(a.state.outbound_sessions(), 1);
        assert!(a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 902 })).await.unwrap());
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 902 }))));
        assert!(Arc::ptr_eq(a.adaptor.peers.read().get(&key).unwrap(), &original));
        assert!(!original.is_closed());

        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn hub_replacement_closes_the_old_session_without_closing_its_replacement() {
        let r = node().await;
        let a = node().await;
        let b = node().await;
        let route = relay_route(&b, &r).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1 && b.adaptor.active_peers_len() == 1).await;
        let original = a.adaptor.peers.read().get(&key).unwrap().clone();
        let remote_closed = b.adaptor.peers.read().values().next().unwrap().closed();

        // A separate adaptor bypasses the local duplicate check while sharing A's
        // transport. B's Hub then replaces its old router with the new session.
        let (received, mut messages) = mpsc::channel(32);
        let replacement = Adaptor::client_only(
            Hub::new(),
            Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received }),
            Default::default(),
            a.state.clone(),
        );
        assert_eq!(replacement.connect_libp2p_peer(&route).await.unwrap(), key);
        tokio::time::timeout(Duration::from_secs(5), remote_closed).await.unwrap();
        wait_until(|| original.is_closed() && a.state.outbound_sessions() == 1).await;
        wait_until(|| replacement.active_peers_len() == 1).await;
        assert!(replacement.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 903 })).await.unwrap());
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), messages.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 903 }))));
        assert!(!replacement.peers.read().get(&key).unwrap().is_closed());
        assert_eq!(b.adaptor.active_peers_len(), 1);

        replacement.close().await;
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test]
    async fn mandatory_quic_listener_closure_stops_transport_and_releases_socket() {
        let (received, _receiver) = mpsc::channel(1);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received });
        let (hub, _events) = mpsc::channel(1);
        let address = SocketAddr::from(([127, 0, 0, 1], free_port_pair()));
        let state = Arc::new(Libp2pTransport::new(address.port(), true).unwrap());
        let handler = ConnectionHandler::new(hub, initializer, Default::default(), state.clone());
        let (mut driver, commands) = Driver::new(state.clone(), address.ip().into(), handler).unwrap();
        let listener = driver.quic_listener;
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let event = driver.swarm.select_next_some().await;
                let ready = matches!(&event, SwarmEvent::NewListenAddr { listener_id, .. } if *listener_id == listener);
                driver.event(event);
                if ready {
                    break;
                }
            }
        })
        .await
        .unwrap();
        assert!(driver.swarm.remove_listener(listener));
        tokio::time::timeout(Duration::from_secs(5), driver.run(commands)).await.unwrap();
        assert!(state.cancel.is_cancelled());
        assert!(matches!(state.take_failure(), Some(TransportError::ListenerClosed { listener, .. }) if listener == "QUIC"));
        assert!(std::net::TcpListener::bind(address).is_ok());
        // Quinn releases its UDP socket through an asynchronous endpoint cleanup task.
        wait_until(|| std::net::UdpSocket::bind(address).is_ok()).await;
    }

    #[tokio::test]
    async fn transport_errors_preserve_causes_and_distinguish_lifecycle_failures() {
        use std::error::Error;
        let source = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let error = TransportError::Io(source);
        assert_eq!(error.source().unwrap().downcast_ref::<io::Error>().unwrap().kind(), io::ErrorKind::PermissionDenied);
        assert!(matches!(validate_destination("0.0.0.0:1".parse().unwrap(), true), Err(TransportError::InvalidDestination(_))));
        let id = Libp2pTransport::new(16512, false).unwrap().id();
        assert!(matches!(validate_endpoint(&EndpointAddress { id, relay: None }, true), Err(TransportError::MissingRelay)));
        let state = Libp2pTransport::new(16512, false).unwrap();
        assert!(matches!(state.sender(), Err(TransportError::NotReady)));
        let (sender, receiver) = mpsc::channel(1);
        *state.commands.write() = Some(sender);
        drop(receiver);
        assert!(matches!(
            state.register_relay(LibP2pAddress { address: "8.8.8.8:1".parse().unwrap(), id }).await,
            Err(ConnectionError::Libp2p(TransportError::ChannelClosed))
        ));
    }

    #[tokio::test]
    async fn stream_negotiation_failure_completes_outbound_request_without_consuming_quota() {
        let (received, _receiver) = mpsc::channel(1);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received });
        let (hub, _events) = mpsc::channel(1);
        let state = Arc::new(Libp2pTransport::new(free_port_pair(), true).unwrap());
        let handler = ConnectionHandler::new(hub, initializer, Default::default(), state.clone());
        let (mut driver, _commands) = Driver::new(state.clone(), std::net::Ipv4Addr::LOCALHOST.into(), handler).unwrap();
        let peer = identity::Keypair::generate_ed25519().public().to_peer_id();
        let connection = ConnectionId::new_unchecked(1);
        driver.connections.insert(
            connection,
            TransportConnection {
                peer,
                address: "127.0.0.1:16512".parse().unwrap(),
                cancel: CancellationToken::new(),
                sessions: Vec::new(),
            },
        );
        let (reply, mut receiver) = oneshot::channel();
        driver.pending.insert(peer, PendingDial { relay: None, reply, connection: Some(connection), circuit: None });

        driver.stream(peer, connection, Err(TransportError::Stream(libp2p::swarm::StreamUpgradeError::NegotiationFailed)));

        assert!(matches!(
            receiver.try_recv().unwrap(),
            Err(ConnectionError::Libp2p(TransportError::Stream(libp2p::swarm::StreamUpgradeError::NegotiationFailed)))
        ));
        assert!(driver.pending.is_empty());
        assert_eq!(state.outbound_sessions(), 0);
        assert!(!driver.connections[&connection].cancel.is_cancelled());
    }

    #[tokio::test]
    async fn local_dial_limits_return_capacity_error() {
        for libp2p_limit in [false, true] {
            let (received, _receiver) = mpsc::channel(1);
            let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received });
            let (hub, _events) = mpsc::channel(1);
            let state = Arc::new(Libp2pTransport::new(free_port_pair(), true).unwrap());
            let handler = ConnectionHandler::new(hub, initializer, Default::default(), state.clone());
            let (mut driver, _commands) = Driver::new(state, std::net::Ipv4Addr::LOCALHOST.into(), handler).unwrap();
            let relay = identity::Keypair::generate_ed25519().public().to_peer_id();
            let relay_address = LibP2pAddress { address: "127.0.0.1:9".parse().unwrap(), id: public_id(relay).unwrap() };
            driver.candidate("127.0.0.1:16512".parse().unwrap());
            let mut replies = Vec::new();
            if libp2p_limit {
                *driver.swarm.behaviour_mut().limits.limits_mut() =
                    libp2p::connection_limits::ConnectionLimits::default().with_max_pending_outgoing(Some(0));
            } else {
                // Do not poll the Swarm: all 32 distinct attempts stay pending.
                for _ in 0..MAX_PENDING_DIALS {
                    let id = public_id(identity::Keypair::generate_ed25519().public().to_peer_id()).unwrap();
                    let (reply, receiver) = oneshot::channel();
                    replies.push((peer_id(id).unwrap(), receiver));
                    driver.command(Command::Dial(EndpointAddress { id, relay: Some(relay_address) }, reply));
                }
                assert_eq!(driver.pending.len(), MAX_PENDING_DIALS);
            }
            let id = public_id(identity::Keypair::generate_ed25519().public().to_peer_id()).unwrap();
            let (reply, receiver) = oneshot::channel();
            driver.command(Command::Dial(EndpointAddress { id, relay: Some(relay_address) }, reply));
            assert!(matches!(receiver.await.unwrap(), Err(ConnectionError::AtCapacity)));
            assert!(!driver.pending.contains_key(&peer_id(id).unwrap()));

            *driver.swarm.behaviour_mut().limits.limits_mut() =
                libp2p::connection_limits::ConnectionLimits::default().with_max_pending_outgoing(Some(0));
            if !libp2p_limit {
                // A native relay-control capacity error defers all destinations using it.
                let dial = DialOpts::peer_id(relay).addresses(vec![multiaddr(relay_address.address)]).build();
                let connection_id = dial.connection_id();
                let error = driver.swarm.dial(dial).unwrap_err();
                driver.event(SwarmEvent::OutgoingConnectionError { peer_id: Some(relay), connection_id, error });
                for (_, receiver) in replies {
                    assert!(matches!(receiver.await.unwrap(), Err(ConnectionError::AtCapacity)));
                }
                assert!(driver.pending.is_empty());
            }

            let (reply, receiver) = oneshot::channel();
            driver.command(Command::RegisterRelay(relay_address, relay, reply));
            assert!(matches!(receiver.await.unwrap(), Err(ConnectionError::AtCapacity)));
            assert!(driver.reservation.is_none());

            // An accepted reservation requires no additional connection.
            let route = multiaddr(relay_address.address).with(Protocol::P2p(relay)).with(Protocol::P2pCircuit);
            let listener = driver.swarm.listen_on(route).unwrap();
            let (reply, receiver) = oneshot::channel();
            driver.reservation = Some(Reservation {
                relay: relay_address,
                peer: relay,
                listener: Some(listener),
                connection: None,
                reply: Some(reply),
            });
            driver.event(SwarmEvent::Behaviour(BehaviourEvent::RelayClient(relay::client::Event::ReservationReqAccepted {
                relay_peer_id: relay,
                renewal: false,
                limit: None,
            })));
            assert!(receiver.await.unwrap().is_ok());
            assert!(driver.reservation.is_some());
            assert_eq!(*driver.state.home.read(), Some(relay_address));
        }
    }

    #[test]
    fn kaspa_protocol_is_not_offered_on_relayed_connections() {
        use libp2p::core::upgrade::UpgradeInfo;
        use libp2p::swarm::ConnectionHandler as Libp2pHandler;
        assert_eq!(KaspaStreams::handler(false).listen_protocol().upgrade().protocol_info().count(), 0);
        assert_eq!(KaspaStreams::handler(true).listen_protocol().upgrade().protocol_info().count(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn http2_acceptor_stays_open_before_the_first_rpc() {
        use pb::p2p_client::P2pClient;
        let r = node().await;
        let a = node().await;
        let b = node().await;
        let route = relay_route(&b, &r).await;
        let stream = a.state.connect(&route).await.unwrap();
        // Give the server time to observe an empty accept queue before the HTTP/2 preface.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut stream = Some(stream);
        let channel = tonic::transport::Endpoint::from_static("http://kaspa.libp2p")
            .connect_with_connector(tower::service_fn(move |_| {
                let io = stream.take();
                async move {
                    io.map(hyper_util::rt::TokioIo::new)
                        .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stream already consumed"))
                }
            }))
            .await
            .unwrap();
        // The HTTP/2 connection must also remain open while no RPC is active.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut client = P2pClient::new(channel).send_compressed(tonic::codec::CompressionEncoding::Gzip);
        let (sender, receiver) = mpsc::channel(16);
        let mut incoming = client.message_stream(tokio_stream::wrappers::ReceiverStream::new(receiver)).await.unwrap().into_inner();
        let version = Version::new(None, uuid::Uuid::new_v4().into(), "kaspa-simnet".into(), None, 11);
        sender.send(make_message!(Payload::Version, version.into())).await.unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                match incoming.message().await.unwrap().unwrap().payload.unwrap() {
                    Payload::Version(_) => sender.send(make_message!(Payload::Verack, pb::VerackMessage {})).await.unwrap(),
                    Payload::Verack(_) => sender.send(make_message!(Payload::Ready, pb::ReadyMessage {})).await.unwrap(),
                    Payload::Ready(_) => break,
                    payload => panic!("unexpected handshake message: {payload:?}"),
                }
            }
        })
        .await
        .unwrap();
        drop(incoming);
        drop(sender);
        drop(client);
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inbound_handshakes_limit_the_direct_ip_across_different_endpoint_ids() {
        use pb::p2p_client::P2pClient;

        let r = node().await;
        let server = node().await;
        let a = node().await;
        let b = node().await;
        let c = node().await;
        let route = relay_route(&server, &r).await;
        // Opening streams alone does not reserve handshake slots. Admission starts at the Kaspa RPC.
        let mut clients = Vec::new();
        for node in [&a, &b, &c] {
            let mut stream = Some(node.state.connect(&route).await.unwrap());
            let channel = tonic::transport::Endpoint::from_static("http://kaspa.libp2p")
                .connect_with_connector(tower::service_fn(move |_| {
                    let io = stream.take();
                    async move {
                        io.map(hyper_util::rt::TokioIo::new)
                            .ok_or_else(|| io::Error::new(io::ErrorKind::BrokenPipe, "stream already consumed"))
                    }
                }))
                .await
                .unwrap();
            clients.push(P2pClient::new(channel));
        }
        // Leave two Kaspa handshakes unfinished, using distinct endpoint IDs on the same source IP.
        let first = clients[0].message_stream(futures::stream::pending()).await.unwrap();
        let second = clients[1].message_stream(futures::stream::pending()).await.unwrap();
        let error = clients[2].message_stream(futures::stream::pending()).await.unwrap_err();
        assert_eq!(error.code(), tonic::Code::ResourceExhausted, "a third endpoint must not bypass the per-IP limit");
        drop((first, second, clients));
        close(&a).await;
        close(&b).await;
        close(&c).await;
        close(&server).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn kaspa_handshake_messages_and_shutdown_after_relay_setup() {
        let r = node().await;
        let mut a = node().await;
        let b = node().await;
        let route = relay_route(&b, &r).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1 && b.adaptor.active_peers_len() == 1).await;
        assert_eq!(a.state.outbound_sessions(), 1);
        assert_eq!(b.state.outbound_sessions(), 0);
        let outgoing = &a.adaptor.active_peers()[0];
        assert_eq!(outgoing.net_address().ip(), b.state.direct_address().unwrap().ip());
        assert_eq!(outgoing.libp2p_id(), Some(route.id));
        let incoming = &b.adaptor.active_peers()[0];
        assert_eq!(incoming.libp2p_id(), Some(a.state.id()));
        assert_eq!(incoming.net_address().ip(), a.state.direct_address().unwrap().ip());
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 42 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 42 }))));
        a.adaptor.send(key, make_message!(Payload::Transaction, pb::TransactionMessage::default())).await.unwrap();
        assert!(a.adaptor.connect_libp2p_peer(&route).await.is_err());
        assert_eq!(a.adaptor.active_peers_len(), 1);
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 43 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 43 }))));
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_relay_supports_registration_and_direct_session() {
        /// Relay fixture using libp2p's relay server and a QUIC listener.
        #[derive(NetworkBehaviour)]
        struct QuicRelay {
            relay: relay::Behaviour,
            identify: identify::Behaviour,
        }

        let mut relay = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_quic()
            .with_behaviour(|key| QuicRelay {
                relay: relay::Behaviour::new(
                    key.public().to_peer_id(),
                    relay::Config { max_circuit_duration: RELAY_DURATION, max_circuit_bytes: RELAY_BYTES, ..Default::default() },
                ),
                identify: identify::Behaviour::new(identify::Config::new("kaspa/p2p/1".into(), key.public())),
            })
            .unwrap()
            .build();
        let id = public_id(*relay.local_peer_id()).unwrap();
        relay.listen_on(multiaddr("127.0.0.1:0".parse().unwrap())).unwrap();
        let address = loop {
            if let SwarmEvent::NewListenAddr { address, .. } = relay.select_next_some().await {
                relay.add_external_address(address.clone());
                break socket(&address).unwrap();
            }
        };
        let relay_task = tokio::spawn(async move { while relay.next().await.is_some() {} });
        let mut a = node().await;
        let b = node().await;
        tokio::time::timeout(Duration::from_secs(5), b.state.register_relay(LibP2pAddress { address, id })).await.unwrap().unwrap();
        assert!(b.state.has_relay_reservation(LibP2pAddress { address, id }));
        let route = b.state.endpoint_address().unwrap();
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 78 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 78 }))));
        relay_task.abort();
        let _ = relay_task.await;
        wait_until(|| !b.state.has_relay_reservation(LibP2pAddress { address, id })).await;
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 79 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 79 }))));
        close(&a).await;
        close(&b).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_connections_support_reservations_and_kaspa_sessions() {
        let b = node().await;
        let (sent, mut received) = mpsc::channel(32);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received: sent });
        let state = Arc::new(Libp2pTransport::new(free_port_pair(), true).unwrap());
        let (hub_sender, hub_receiver) = mpsc::channel(32);
        let hub = Hub::new();
        let handler = ConnectionHandler::new(hub_sender, initializer, Default::default(), state.clone());
        let adaptor = Arc::new(Adaptor::new(None, handler.clone(), hub.clone()));
        hub.start_event_loop(hub_receiver);
        let (mut driver, commands) = Driver::new(state.clone(), std::net::Ipv4Addr::LOCALHOST.into(), handler).unwrap();
        let address = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), b.state.relay_port());
        driver.swarm.dial(multiaddr(address)).unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while driver.connections.is_empty() {
                let event = driver.swarm.select_next_some().await;
                driver.event(event);
            }
        })
        .await
        .unwrap();

        let state_task = state.clone();
        state.spawn(async move {
            driver.run(commands).await;
            state_task.commands.write().take();
            state_task.home.write().take();
        });
        let route = EndpointAddress { id: b.state.id(), relay: Some(LibP2pAddress { address, id: b.state.id() }) };
        let key = adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| b.adaptor.active_peers_len() == 1 && adaptor.active_peers_len() == 1).await;
        assert_eq!(adaptor.active_peers()[0].libp2p_id(), Some(b.state.id()));
        assert_eq!(adaptor.active_peers()[0].net_address(), address);
        adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 76 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 76 }))));
        state.register_relay(LibP2pAddress { address, id: b.state.id() }).await.unwrap();
        assert!(state.has_relay_reservation(LibP2pAddress { address, id: b.state.id() }));
        // The direct session is independent of the relay reservation.
        state.remove_relay(LibP2pAddress { address, id: b.state.id() }).await;
        wait_until(|| !state.has_relay_reservation(LibP2pAddress { address, id: b.state.id() })).await;
        adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 77 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 77 }))));
        tokio::time::timeout(Duration::from_secs(10), adaptor.close()).await.unwrap();
        assert!(state.tasks.is_empty());
        assert!(!state.is_ready());
        close(&b).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn advertised_relay_identity_is_authenticated() {
        let r = node().await;
        let a = node().await;
        let b = node().await;
        let good = relay_route(&b, &r).await;
        let mut bad = good.clone();
        bad.relay.as_mut().unwrap().id = public_id(identity::Keypair::generate_ed25519().public().to_peer_id()).unwrap();
        assert!(tokio::time::timeout(Duration::from_secs(5), a.state.connect(&bad)).await.unwrap().is_err());
        assert_eq!(a.adaptor.active_peers_len(), 0);
        assert_eq!(b.adaptor.active_peers_len(), 0);
        // A failed identity must not prevent dialing the authenticated route.
        a.adaptor.connect_libp2p_peer(&good).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cancelled_dial_does_not_block_the_next_route_for_the_same_identity() {
        let r = node().await;
        let mut a = node().await;
        let b = node().await;
        let good = relay_route(&b, &r).await;
        let mut bad = good.clone();
        let blackhole = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        bad.relay.as_mut().unwrap().address = blackhole.local_addr().unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(20), a.state.connect(&bad)).await.is_err());
        let key = a.adaptor.connect_libp2p_peer(&good).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 202 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 202 }))));
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_bind_failure_returns_error_without_starting_libp2p_tasks() {
        let mut a = node().await;
        let (received, _receiver) = mpsc::channel(32);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received });
        let port = free_port_pair();
        let _occupied = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).unwrap();
        let address = NetAddress::from(SocketAddr::from(([127, 0, 0, 1], port)));
        let state = Arc::new(Libp2pTransport::new(port + 1, true).unwrap());
        let id = state.id();
        let server = Adaptor::bidirectional(address, Hub::new(), initializer, Default::default(), state.clone()).unwrap();
        let error = server.start_libp2p(address.ip).unwrap_err();
        assert!(error.to_string().contains("cannot bind QUIC listener"));
        assert!(state.tasks.is_empty());
        assert_eq!(state.id(), id);
        assert!(!state.is_ready());
        let key = a.adaptor.connect_peer_with_retries(address.to_string(), 5, Duration::from_millis(20)).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        assert!(a.adaptor.active_peers()[0].libp2p_id().is_none());
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 123 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 123 }))));
        close(&a).await;
        tokio::time::timeout(Duration::from_secs(10), server.close()).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn quic_bind_failure_can_retry_without_binding_tcp() {
        let (received, _receiver) = mpsc::channel(32);
        let initializer = Arc::new(Initializer { node_id: uuid::Uuid::new_v4().into(), received });
        let port = free_port_pair();
        let address = NetAddress::from(SocketAddr::from(([127, 0, 0, 1], port)));
        let occupied = std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).unwrap();
        let state = Arc::new(Libp2pTransport::new(port + 1, true).unwrap());
        let id = state.id();
        let adaptor = Adaptor::client_only(Hub::new(), initializer, Default::default(), state.clone());
        let error = adaptor.start_libp2p(address.ip).unwrap_err();
        assert!(error.to_string().contains("cannot bind QUIC listener"));
        assert!(state.tasks.is_empty());
        assert!(!state.is_ready());
        assert_eq!(state.id(), id);
        drop(occupied);
        wait_until(|| std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok()).await;
        adaptor.start_libp2p(address.ip).unwrap();
        assert_eq!(state.id(), id);
        assert!(state.is_ready());
        assert!(std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port + 1)).is_ok(), "libp2p must not bind TCP");
        tokio::time::timeout(Duration::from_secs(10), adaptor.close()).await.unwrap();
    }

    #[test]
    fn each_transport_has_a_fresh_in_memory_identity() {
        let first = Libp2pTransport::new(16512, true).unwrap();
        let second = Libp2pTransport::new(16512, true).unwrap();
        assert_ne!(first.id(), second.id());
        assert_eq!(public_id(peer_id(first.id()).unwrap()).unwrap(), first.id());
        assert_eq!(public_id(first.signing_key.public().to_peer_id()).unwrap(), first.id());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn registration_checks_selected_socket_and_preserves_existing_sessions() {
        let setup_relay = node().await;
        let r = node().await;
        let mut a = node().await;
        let route = relay_route(&r, &setup_relay).await;
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        let relay_id = r.state.id();
        let correct = SocketAddr::new(std::net::Ipv4Addr::LOCALHOST.into(), r.state.relay_port());
        let wrong = tokio::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let wrong_address = wrong.local_addr().unwrap();
        let (accepted, mut attempts) = mpsc::channel(2);
        let listener = tokio::spawn(async move {
            while wrong.recv_from(&mut [0; 2048]).await.is_ok() {
                let _ = accepted.send(()).await;
            }
        });
        // A direct connection must not make an unusable relay port pass.
        assert!(a.state.register_relay(LibP2pAddress { address: wrong_address, id: relay_id }).await.is_err());
        assert_eq!(a.state.endpoint_address().unwrap().relay, None);
        tokio::time::timeout(Duration::from_secs(5), attempts.recv()).await.unwrap().unwrap();
        a.state.register_relay(LibP2pAddress { address: correct, id: relay_id }).await.unwrap();
        // Nor may a cached control connection to the correct port mask it.
        assert!(a.state.register_relay(LibP2pAddress { address: wrong_address, id: relay_id }).await.is_err());
        assert!(!a.state.has_relay_reservation(LibP2pAddress { address: correct, id: relay_id }));
        assert_eq!(a.state.endpoint_address().unwrap().relay, None);
        tokio::time::timeout(Duration::from_secs(5), attempts.recv()).await.unwrap().unwrap();
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 201 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 201 }))));
        listener.abort();
        close(&a).await;
        close(&r).await;
        close(&setup_relay).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn registration_publishes_reservation_and_withdraws_on_stop() {
        let r = node().await;
        let a = node().await;
        let relay_id = r.state.id();
        let address = SocketAddr::new(r.state.direct_address().unwrap().ip(), r.state.relay_port());
        assert!(!a.state.has_relay_reservation(LibP2pAddress { address, id: relay_id }));
        assert_eq!(a.state.endpoint_address().unwrap().relay, None);
        a.state.register_relay(LibP2pAddress { address, id: relay_id }).await.unwrap();
        assert!(a.state.has_relay_reservation(LibP2pAddress { address, id: relay_id }));
        assert_eq!(a.state.endpoint_address().unwrap().relay, Some(LibP2pAddress { address, id: relay_id }));
        close(&r).await;
        wait_until(|| !a.state.has_relay_reservation(LibP2pAddress { address, id: relay_id })).await;
        assert_eq!(a.state.endpoint_address().unwrap().relay, None);
        close(&a).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_peers_share_relay_control_without_sharing_identity() {
        let r = node().await;
        let mut a = node().await;
        let relay_id = r.state.id();
        let relay = SocketAddr::new(r.state.direct_address().unwrap().ip(), r.state.relay_port());
        let mut destinations = Vec::new();
        // More than the libp2p per-peer connection limit: each dial must reuse
        // the relay's control connection rather than opening another helper.
        for _ in 0..5 {
            let destination = node().await;
            destination.state.register_relay(LibP2pAddress { address: relay, id: relay_id }).await.unwrap();
            destinations.push(destination);
        }
        let routes: Vec<_> = destinations.iter().map(|destination| destination.state.endpoint_address().unwrap()).collect();
        let keys = futures::future::try_join_all(routes.iter().map(|route| a.adaptor.connect_libp2p_peer(route))).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == destinations.len()).await;
        let ids: std::collections::HashSet<_> = a
            .adaptor
            .active_peers()
            .iter()
            .map(|peer| {
                assert_ne!(peer.net_address(), relay);
                peer.libp2p_id().expect("authenticated libp2p peer")
            })
            .collect();
        assert_eq!(ids.len(), destinations.len());
        close(&r).await;
        // Once punched, Kaspa sessions must survive loss of the shared relay.
        for (nonce, key) in keys.into_iter().enumerate() {
            let nonce = nonce as u64;
            a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce })).await.unwrap();
            let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
            assert!(matches!(message.payload, Some(Payload::Pong(pong)) if pong.nonce == nonce));
        }
        close(&a).await;
        for destination in &destinations {
            close(destination).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn missing_or_unreachable_relay_does_not_use_reachable_direct_candidate() {
        let a = node().await;
        let b = node().await;
        let mut route = b.state.endpoint_address().unwrap();
        let error = a.state.connect(&route).await.err().expect("missing relay must fail");
        assert!(error.to_string().contains("requires a destination relay"));
        let blackhole = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let relay_id = public_id(identity::Keypair::generate_ed25519().public().to_peer_id()).unwrap();
        route.relay = Some(LibP2pAddress { address: blackhole.local_addr().unwrap(), id: relay_id });
        assert!(tokio::time::timeout(Duration::from_secs(2), a.state.connect(&route)).await.is_err());
        assert_eq!(b.adaptor.active_peers_len(), 0);
        close(&a).await;
        close(&b).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn relay_assisted_kaspa_session_uses_a_direct_connection() {
        let r = node().await;
        let mut a = node().await;
        let b = node().await;
        let r_address = r.state.direct_address().unwrap();
        let relay_id = r.state.id();
        let relay = SocketAddr::new(r_address.ip(), r.state.relay_port());
        b.state.register_relay(LibP2pAddress { address: relay, id: relay_id }).await.unwrap();
        let decoy = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let mut advertised = PeerAddress {
            libp2p: Some(kaspa_utils::connection::Libp2pMetadata {
                endpoint_id: b.state.id(),
                port: decoy.local_addr().unwrap().port(),
                relay_port: b.state.relay_port(),
                nonce: 1,
                signature: Vec::new(),
            }),
            relay_address: Some(LibP2pAddress { address: relay, id: relay_id }),
            ..NetAddress::new(decoy.local_addr().unwrap().ip().into(), 9).into()
        };
        let signature = b.state.sign_address(&advertised, "kaspa-simnet").unwrap();
        advertised.libp2p.as_mut().unwrap().signature = signature;
        let route = advertised.endpoint().unwrap();
        let key = a.adaptor.connect_libp2p_peer(&route).await.unwrap();
        wait_until(|| a.adaptor.active_peers_len() == 1).await;
        let peer = &a.adaptor.active_peers()[0];
        assert_eq!(peer.libp2p_id(), Some(route.id));
        assert_ne!(peer.net_address(), decoy.local_addr().unwrap());
        assert_eq!(key, crate::PeerKey::Libp2p(route.id));
        assert_eq!(
            decoy.try_recv(&mut [0; 2048]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock,
            "advertised UDP candidate must not be dialed"
        );
        a.adaptor.send(key, make_message!(Payload::Ping, pb::PingMessage { nonce: 91 })).await.unwrap();
        let (_, message) = tokio::time::timeout(Duration::from_secs(5), a.received.recv()).await.unwrap().unwrap();
        assert!(matches!(message.payload, Some(Payload::Pong(pb::PongMessage { nonce: 91 }))));
        close(&a).await;
        close(&b).await;
        close(&r).await;
    }
}
