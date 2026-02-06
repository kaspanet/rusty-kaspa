use crate::core::hub::HubEvent;
use crate::core::peer::PeerOutboundType;
use crate::pb::RejectMessage;
use crate::pb::{KaspadMessage, kaspad_message::Payload as KaspadMessagePayload};
use crate::{KaspadMessagePayloadType, common::ProtocolError};
use crate::{Peer, make_message};
use kaspa_consensus_core::Hash;
use kaspa_core::{debug, error, info, trace, warn};
use kaspa_utils::networking::PeerId;
use parking_lot::{Mutex, RwLock};
use seqlock::SeqLock;
use std::collections::HashMap;
use std::fmt::{Debug, Display};
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Instant;
use tokio::select;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver as MpscReceiver, Sender as MpscSender, channel as mpsc_channel};
use tokio::sync::oneshot::{Sender as OneshotSender, channel as oneshot_channel};
use tonic::Streaming;

use super::peer::{PeerKey, PeerProperties};

pub struct IncomingRoute {
    rx: MpscReceiver<KaspadMessage>,
    id: u32,
}

// BLANK_ROUTE_ID is the value that is used in the p2p when no request or response IDs
// are needed. To support backward compatibility, this is set to the default gRPC value
// for uint32.
pub const BLANK_ROUTE_ID: u32 = 0;
static ROUTE_ID: AtomicU32 = AtomicU32::new(BLANK_ROUTE_ID + 1);

impl IncomingRoute {
    pub fn new(rx: MpscReceiver<KaspadMessage>) -> Self {
        let id = ROUTE_ID.fetch_add(1, Ordering::SeqCst);
        Self { rx, id }
    }

    pub fn id(&self) -> u32 {
        self.id
    }
}

impl Deref for IncomingRoute {
    type Target = MpscReceiver<KaspadMessage>;

    fn deref(&self) -> &Self::Target {
        &self.rx
    }
}

impl DerefMut for IncomingRoute {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.rx
    }
}

#[derive(Clone)]
pub struct SharedIncomingRoute(Arc<tokio::sync::Mutex<IncomingRoute>>);

impl SharedIncomingRoute {
    pub fn new(incoming_route: IncomingRoute) -> Self {
        Self(Arc::new(tokio::sync::Mutex::new(incoming_route)))
    }

    pub async fn recv(&mut self) -> Option<KaspadMessage> {
        self.0.lock().await.recv().await
    }
}

/// The policy for handling the case where route capacity is reached for a specific route type
pub enum IncomingRouteOverflowPolicy {
    /// Drop the incoming message
    Drop,

    /// Disconnect from this peer
    Disconnect,
}

impl From<KaspadMessagePayloadType> for IncomingRouteOverflowPolicy {
    fn from(msg_type: KaspadMessagePayloadType) -> Self {
        match msg_type {
            // Inv messages are unique in the sense that no harm is done if some of them are dropped
            KaspadMessagePayloadType::InvTransactions | KaspadMessagePayloadType::InvRelayBlock => IncomingRouteOverflowPolicy::Drop,
            _ => IncomingRouteOverflowPolicy::Disconnect,
        }
    }
}

#[derive(Debug, Default)]
struct RouterMutableState {
    /// Used on router init to signal the router receive loop to start listening
    start_signal: Option<OneshotSender<()>>,

    /// Used on router close to signal the router receive loop to exit
    shutdown_signal: Option<OneshotSender<()>>,

    /// Properties of the peer
    properties: Arc<PeerProperties>,

    /// Duration of the last ping to this peer
    last_ping_duration: u64,

    perigee_timestamps: HashMap<Hash, Instant>,
}

impl RouterMutableState {
    fn new(start_signal: Option<OneshotSender<()>>, shutdown_signal: Option<OneshotSender<()>>) -> Self {
        Self { start_signal, shutdown_signal, ..Default::default() }
    }
}

/// A router object for managing the communication to a network peer. It is named a router because it's responsible
/// for internally routing messages to P2P flows based on registration and message types
#[derive(Debug)]
pub struct Router {
    /// Internal identity of this peer
    identity: SeqLock<PeerId>,

    /// The socket address of this peer
    net_address: SocketAddr,

    /// Indicates whether this connection is an outbound connection, and if so under which outbound type
    outbound_type: Option<PeerOutboundType>,

    /// Time of creation of this object and the connection it holds
    connection_started: Instant,

    /// Routing map for mapping messages to subscribed flows
    routing_map_by_type: RwLock<HashMap<KaspadMessagePayloadType, MpscSender<KaspadMessage>>>,

    routing_map_by_id: RwLock<HashMap<u32, MpscSender<KaspadMessage>>>,

    /// The outgoing route for sending messages to this peer
    outgoing_route: MpscSender<KaspadMessage>,

    /// A channel sender for internal event management. Used to send information from each router to a central hub object
    hub_sender: MpscSender<HubEvent>,

    /// Used for managing router mutable state
    mutable_state: Mutex<RouterMutableState>,
}

impl Display for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.net_address)
    }
}

impl From<&Router> for PeerKey {
    fn from(value: &Router) -> Self {
        Self::new(value.identity.read(), value.net_address.ip().into(), value.net_address.port())
    }
}

impl From<(&Router, bool)> for Peer {
    /// the bool indicates whether to include perigee data
    fn from(item: (&Router, bool)) -> Self {
        let (router, include_perigee_data) = item;
        Self::new(
            router.identity(),
            router.net_address,
            router.outbound_type,
            router.connection_started,
            router.properties(),
            router.last_ping_duration(),
            if include_perigee_data {
                let perigee_timestamps = router.perigee_timestamps();
                Arc::new(perigee_timestamps)
            } else {
                Arc::new(HashMap::new())
            },
        )
    }
}

fn message_summary(msg: &KaspadMessage) -> impl Debug {
    // TODO (low priority): display a concise summary of the message. Printing full messages
    // overflows the logs and is hardly useful, hence we currently only return the type
    msg.payload.as_ref().map(std::convert::Into::<KaspadMessagePayloadType>::into)
}

impl Router {
    pub(crate) async fn new(
        net_address: SocketAddr,
        outbound_type: Option<PeerOutboundType>,
        hub_sender: MpscSender<HubEvent>,
        mut incoming_stream: Streaming<KaspadMessage>,
        outgoing_route: MpscSender<KaspadMessage>,
    ) -> Arc<Self> {
        let (start_sender, start_receiver) = oneshot_channel();
        let (shutdown_sender, mut shutdown_receiver) = oneshot_channel();

        let router = Arc::new(Router {
            identity: Default::default(),
            net_address,
            outbound_type,
            connection_started: Instant::now(),
            routing_map_by_type: RwLock::new(HashMap::new()),
            routing_map_by_id: RwLock::new(HashMap::new()),
            outgoing_route,
            hub_sender,
            mutable_state: Mutex::new(RouterMutableState::new(Some(start_sender), Some(shutdown_sender))),
        });

        let router_clone = router.clone();
        // Start the router receive loop
        tokio::spawn(async move {
            // Wait for a start signal before entering the receive loop
            let _ = start_receiver.await;
            loop {
                select! {
                    biased; // We use biased polling so that the shutdown signal is always checked first

                    _ = &mut shutdown_receiver => {
                        debug!("P2P, Router receive loop - shutdown signal received, exiting router receive loop, router-id: {}", router.identity());
                        break;
                    }

                    res = incoming_stream.message() => match res {
                        Ok(Some(msg)) => {
                            trace!("P2P msg: {:?}, router-id: {}, peer: {}", message_summary(&msg), router.identity(), router);
                            match router.route_to_flow(msg) {
                                Ok(()) => {},
                                Err(e) => {
                                    match e {
                                        ProtocolError::IgnorableReject(reason) => debug!("P2P, got reject message: {} from peer: {}", reason, router),
                                        ProtocolError::Rejected(reason) => warn!("P2P, got reject message: {} from peer: {}", reason, router),
                                        e => warn!("P2P, route error: {} for peer: {}", e, router),
                                    }
                                    break;
                                },
                            }
                        }
                        Ok(None) => {
                            info!("P2P, incoming stream ended from peer {}", router);
                            break;
                        }
                        Err(status) => {
                            if let Some(err) = match_for_io_error(&status) {
                                info!("P2P, network error: {} from peer {}", err, router);
                            } else {
                                info!("P2P, network error: {} from peer {}", status, router);
                            }
                            break;
                        }
                    }
                }
            }
            router.close().await;
            debug!("P2P, Router receive loop - exited, router-id: {}, router refs: {}", router.identity(), Arc::strong_count(&router));
        });

        router_clone
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> PeerId {
        self.identity.read()
    }

    pub fn set_identity(&self, identity: PeerId) {
        *self.identity.lock_write() = identity;
    }

    /// The socket address of this peer
    pub fn net_address(&self) -> SocketAddr {
        self.net_address
    }

    pub fn key(&self) -> PeerKey {
        self.into()
    }

    /// Indicates whether this connection is an outbound connection
    pub fn is_outbound(&self) -> bool {
        self.outbound_type.is_some()
    }

    pub fn is_user_supplied(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::UserSupplied))
    }

    pub fn is_perigee(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::Perigee))
    }

    pub fn is_random_graph(&self) -> bool {
        matches!(self.outbound_type, Some(PeerOutboundType::RandomGraph))
    }

    pub fn connection_started(&self) -> Instant {
        self.connection_started
    }

    pub fn time_connected(&self) -> u64 {
        Instant::now().duration_since(self.connection_started).as_millis() as u64
    }

    pub fn properties(&self) -> Arc<PeerProperties> {
        self.mutable_state.lock().properties.clone()
    }

    pub fn set_properties(&self, properties: Arc<PeerProperties>) {
        self.mutable_state.lock().properties = properties;
    }

    /// Sets the duration of the last ping
    pub fn set_last_ping_duration(&self, last_ping_duration: u64) {
        self.mutable_state.lock().last_ping_duration = last_ping_duration;
    }

    pub fn add_perigee_timestamp(&self, hash: Hash, timestamp: Instant) {
        self.mutable_state.lock().perigee_timestamps.insert(hash, timestamp);
    }

    pub fn clear_perigee_timestamps(&self) {
        self.mutable_state.lock().perigee_timestamps.clear();
    }

    pub fn perigee_timestamps(&self) -> HashMap<Hash, Instant> {
        self.mutable_state.lock().perigee_timestamps.clone()
    }

    pub fn last_ping_duration(&self) -> u64 {
        self.mutable_state.lock().last_ping_duration
    }

    pub fn incoming_flow_baseline_channel_size() -> usize {
        256
    }

    pub fn protocol_version(&self) -> u32 {
        self.mutable_state.lock().properties.protocol_version
    }

    /// Send a signal to start this router's receive loop
    pub fn start(&self) {
        // Acquire state mutex and send the start signal
        let op = self.mutable_state.lock().start_signal.take();
        if let Some(signal) = op {
            let _ = signal.send(());
        } else {
            debug!("P2P, Router start was called more than once, router-id: {}", self.identity())
        }
    }

    /// Subscribe to specific message types.
    ///
    /// This should be used by `ConnectionInitializer` instances to register application-specific flows
    pub fn subscribe(&self, msg_types: Vec<KaspadMessagePayloadType>) -> IncomingRoute {
        self.subscribe_with_capacity(msg_types, Self::incoming_flow_baseline_channel_size())
    }

    /// Subscribe to specific message types with a specific channel capacity.
    ///
    /// This should be used by `ConnectionInitializer` instances to register application-specific flows.
    pub fn subscribe_with_capacity(&self, msg_types: Vec<KaspadMessagePayloadType>, capacity: usize) -> IncomingRoute {
        let (sender, receiver) = mpsc_channel(capacity);
        let incoming_route = IncomingRoute::new(receiver);
        let mut map_by_type = self.routing_map_by_type.write();
        for msg_type in msg_types {
            match map_by_type.insert(msg_type, sender.clone()) {
                Some(_) => {
                    // Overrides an existing route -- panic
                    error!(
                        "P2P, Router::subscribe overrides an existing message type: {:?}, router-id: {}",
                        msg_type,
                        self.identity()
                    );
                    panic!("P2P, Tried to subscribe to an existing route");
                }
                None => {
                    trace!("P2P, Router::subscribe - msg_type: {:?} route is registered, router-id:{:?}", msg_type, self.identity());
                }
            }
        }
        let mut map_by_id = self.routing_map_by_id.write();
        match map_by_id.insert(incoming_route.id, sender.clone()) {
            Some(_) => {
                // Overrides an existing route -- panic
                error!(
                    "P2P, Router::subscribe overrides an existing route id: {:?}, router-id: {}",
                    incoming_route.id,
                    self.identity()
                );
                panic!("P2P, Tried to subscribe to an existing route");
            }
            None => {
                trace!(
                    "P2P, Router::subscribe - route id: {:?} route is registered, router-id:{:?}",
                    incoming_route.id,
                    self.identity()
                );
            }
        }
        incoming_route
    }

    /// Routes a message coming from the network to the corresponding registered flow
    pub fn route_to_flow(&self, msg: KaspadMessage) -> Result<(), ProtocolError> {
        if msg.payload.is_none() {
            debug!("P2P, Route to flow got empty payload, peer: {}", self);
            return Err(ProtocolError::Other("received kaspad p2p message with empty payload"));
        }
        let msg_type: KaspadMessagePayloadType = msg.payload.as_ref().expect("payload was just verified").into();
        // Handle the special case of a reject message ending the connection
        if msg_type == KaspadMessagePayloadType::Reject {
            let Some(KaspadMessagePayload::Reject(reject)) = msg.payload else { unreachable!() };
            return Err(ProtocolError::from_reject_message(reject.reason));
        }

        let op = if msg.response_id != BLANK_ROUTE_ID {
            self.routing_map_by_id.read().get(&msg.response_id).cloned()
        } else {
            self.routing_map_by_type.read().get(&msg_type).cloned()
        };

        if let Some(sender) = op {
            match sender.try_send(msg) {
                Ok(_) => Ok(()),
                Err(TrySendError::Closed(_)) => Err(ProtocolError::ConnectionClosed),
                Err(TrySendError::Full(_)) => {
                    let overflow_policy: IncomingRouteOverflowPolicy = msg_type.into();
                    match overflow_policy {
                        IncomingRouteOverflowPolicy::Drop => Ok(()),
                        IncomingRouteOverflowPolicy::Disconnect => {
                            Err(ProtocolError::IncomingRouteCapacityReached(msg_type, self.to_string()))
                        }
                    }
                }
            }
        } else {
            Err(ProtocolError::NoRouteForMessageType(msg_type))
        }
    }

    /// Enqueues a locally-originated message to be sent to the network peer
    pub async fn enqueue(&self, msg: KaspadMessage) -> Result<(), ProtocolError> {
        assert!(msg.payload.is_some(), "Kaspad P2P message should always have a value");
        match self.outgoing_route.try_send(msg) {
            Ok(_) => Ok(()),
            Err(TrySendError::Closed(_)) => Err(ProtocolError::ConnectionClosed),
            Err(TrySendError::Full(_)) => Err(ProtocolError::OutgoingRouteCapacityReached(self.to_string())),
        }
    }

    /// Based on the type of the protocol error, tries sending a reject message before shutting down the connection
    pub async fn try_sending_reject_message(&self, err: &ProtocolError) {
        if err.can_send_outgoing_message() {
            // Send an explicit reject message for easier tracing of logical bugs causing protocol errors.
            // No need to handle errors since we are closing anyway
            let _ = self.enqueue(make_message!(KaspadMessagePayload::Reject, RejectMessage { reason: err.to_reject_message() })).await;
        }
    }

    /// Closes the router, signals exit, and cleans up all resources so that underlying connections will be aborted correctly.
    /// Returns true of this is the first call to close
    pub async fn close(self: &Arc<Router>) -> bool {
        // Acquire state mutex and send the shutdown signal
        // NOTE: Using a block to drop the lock asap
        {
            let mut state = self.mutable_state.lock();

            // Make sure start signal was fired, just in case `self.start()` was never called
            if let Some(signal) = state.start_signal.take() {
                let _ = signal.send(());
            }

            if let Some(signal) = state.shutdown_signal.take() {
                let _ = signal.send(());
            } else {
                // This means the router was already closed
                trace!("P2P, Router close was called more than once, router-id: {}", self.identity());
                return false;
            }
        }

        // Drop all flow senders
        self.routing_map_by_type.write().clear();
        self.routing_map_by_id.write().clear();

        // Send a close notification to the central Hub
        self.hub_sender.send(HubEvent::PeerClosing(self.clone())).await.expect("hub receiver should never drop before senders");

        true
    }
}

fn match_for_io_error(err_status: &tonic::Status) -> Option<&std::io::Error> {
    let mut err: &(dyn std::error::Error + 'static) = err_status;

    loop {
        if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
            return Some(io_err);
        }

        // h2::Error do not expose std::io::Error with `source()`
        // https://github.com/hyperium/h2/pull/462
        if let Some(h2_err) = err.downcast_ref::<h2::Error>() {
            if let Some(io_err) = h2_err.get_io() {
                return Some(io_err);
            }
        }

        err = err.source()?;
    }
}

// --- TEST UTILS ---
#[cfg(feature = "test-utils")]
pub trait RouterTestExt {
    fn test_new(
        identity: PeerId,
        net_address: std::net::SocketAddr,
        outbound_type: Option<super::peer::PeerOutboundType>,
        connection_started: std::time::Instant,
    ) -> std::sync::Arc<Self>
    where
        Self: Sized;
}

#[cfg(any(test, feature = "test-utils"))]
impl RouterTestExt for Router {
    fn test_new(
        identity: PeerId,
        net_address: std::net::SocketAddr,
        outbound_type: Option<super::peer::PeerOutboundType>,
        connection_started: std::time::Instant,
    ) -> std::sync::Arc<Self> {
        use tokio::sync::mpsc;
        let (hub_sender, _hub_receiver) = mpsc::channel(1);
        let (outgoing_route, _outgoing_receiver) = mpsc::channel(1);
        // Create a dummy streaming object (not actually used in this test context)
        // let dummy_stream = Streaming::<KaspadMessage>::new_empty(...); // not needed for struct
        std::sync::Arc::new(Router {
            identity: seqlock::SeqLock::new(identity),
            net_address,
            outbound_type,
            connection_started,
            routing_map_by_type: parking_lot::RwLock::new(std::collections::HashMap::new()),
            routing_map_by_id: parking_lot::RwLock::new(std::collections::HashMap::new()),
            outgoing_route,
            hub_sender,
            mutable_state: parking_lot::Mutex::new(RouterMutableState::new(None, None)),
        })
    }
}
