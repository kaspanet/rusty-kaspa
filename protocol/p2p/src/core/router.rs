use crate::core::hub::HubEvent;
use crate::pb::KaspadMessage;
use crate::NodeId;
use crate::{common::ProtocolError, KaspadMessagePayloadType};
use kaspa_core::{debug, error, info, trace, warn};
use parking_lot::{Mutex, RwLock};
use std::fmt::Display;
use std::net::SocketAddr;
use std::{collections::HashMap, sync::Arc};
use tokio::select;
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tonic::Streaming;
use uuid::Uuid;

pub type IncomingRoute = MpscReceiver<KaspadMessage>;

#[derive(Debug)]
struct RouterMutableState {
    /// Used on router init to signal the router receive loop to start listening
    start_signal: Option<OneshotSender<()>>,

    /// Used on router close to signal the router receive loop to exit
    shutdown_signal: Option<OneshotSender<()>>,
}

/// A router object for managing the communication to a network peer. It is named a router because it's responsible
/// for internally routing messages to P2P flows based on registration and message types
#[derive(Debug)]
pub struct Router {
    /// Internal identity of this peer
    identity: NodeId,

    /// The socket address of this peer
    net_address: SocketAddr,

    /// Indicates whether this connection is an outbound connection
    is_outbound: bool,

    /// Routing map for mapping messages to subscribed flows
    routing_map: RwLock<HashMap<KaspadMessagePayloadType, MpscSender<KaspadMessage>>>,

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

impl Router {
    pub(crate) async fn new(
        net_address: SocketAddr,
        is_outbound: bool,
        hub_sender: MpscSender<HubEvent>,
        mut incoming_stream: Streaming<KaspadMessage>,
        outgoing_route: MpscSender<KaspadMessage>,
    ) -> Arc<Self> {
        let (start_sender, start_receiver) = oneshot_channel();
        let (shutdown_sender, mut shutdown_receiver) = oneshot_channel();

        let router = Arc::new(Router {
            identity: Uuid::new_v4().into(),
            net_address,
            is_outbound,
            routing_map: RwLock::new(HashMap::new()),
            outgoing_route,
            hub_sender,
            mutable_state: Mutex::new(RouterMutableState { start_signal: Some(start_sender), shutdown_signal: Some(shutdown_sender) }),
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
                        debug!("P2P, Router receive loop - shutdown signal received, exiting router receive loop, router-id: {}", router.identity);
                        break;
                    }

                    res = incoming_stream.message() => match res {
                        Ok(Some(msg)) => {
                            trace!("P2P, Router receive loop - got message: {:?}, router-id: {}", msg, router.identity);
                            let msg_str = msg_type_string(&msg);
                            if !(router.route_to_flow(msg).await) {
                                warn!("P2P, Router receive loop - no route for message {} - exiting loop, router-id: {}", msg_str, router.identity);
                                break;
                            }
                        }
                        Ok(None) => {
                            info!("P2P, Router receive loop - incoming stream ended for peer {}", router);
                            break;
                        }
                        Err(err) => {
                            warn!("P2P, Router receive loop - network error: {:?} from peer {}", err, router);
                            break;
                        }
                    }
                }
            }
            router.close().await;
            debug!("P2P, Router receive loop - exited, router-id: {}, router refs: {}", router.identity, Arc::strong_count(&router));
        });

        // Notify the central Hub about the new peer
        router_clone
            .hub_sender
            .send(HubEvent::NewPeer(router_clone.clone()))
            .await
            .expect("hub receiver should never drop before senders");
        router_clone
    }

    /// Internal identity of this peer
    pub fn identity(&self) -> NodeId {
        self.identity
    }

    /// The socket address of this peer
    pub fn net_address(&self) -> SocketAddr {
        self.net_address
    }

    /// Indicates whether this connection is an outbound connection
    pub fn is_outbound(&self) -> bool {
        self.is_outbound
    }

    fn incoming_flow_channel_size() -> usize {
        // TODO: reevaluate when the node is fully functional
        128
    }

    /// Send a signal to start this router's receive loop
    pub fn start(&self) {
        // Acquire state mutex and send the start signal
        let op = self.mutable_state.lock().start_signal.take();
        if let Some(signal) = op {
            let _ = signal.send(());
        } else {
            debug!("P2P, Router start was called more than once, router-id: {}", self.identity)
        }
    }

    /// Subscribe to specific message types.
    ///
    /// This should be used by `ConnectionInitializer` instances to register application-specific flows
    pub fn subscribe(&self, msg_types: Vec<KaspadMessagePayloadType>) -> IncomingRoute {
        self.subscribe_with_capacity(msg_types, Self::incoming_flow_channel_size())
    }

    /// Subscribe to specific message types with a specific channel capacity.
    ///
    /// This should be used by `ConnectionInitializer` instances to register application-specific flows.
    pub fn subscribe_with_capacity(&self, msg_types: Vec<KaspadMessagePayloadType>, capacity: usize) -> IncomingRoute {
        let (sender, receiver) = mpsc_channel(capacity);
        let mut map = self.routing_map.write();
        for msg_type in msg_types {
            match map.insert(msg_type, sender.clone()) {
                Some(_) => {
                    // Overrides an existing route -- panic
                    error!("P2P, Router::subscribe overrides an existing value: {:?}, router-id: {}", msg_type, self.identity);
                    panic!("P2P, Tried to subscribe to an existing route");
                }
                None => {
                    trace!("P2P, Router::subscribe - msg_type: {:?} route is registered, router-id:{:?}", msg_type, self.identity);
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
        let msg_type: KaspadMessagePayloadType = msg.payload.as_ref().expect("payload was just verified").into();
        let op = self.routing_map.read().get(&msg_type).cloned();
        if let Some(sender) = op {
            sender.send(msg).await.is_ok()
        } else {
            false
        }
    }

    /// Enqueues a locally-originated message to be sent to the network peer
    pub async fn enqueue(&self, msg: KaspadMessage) -> Result<(), ProtocolError> {
        assert!(msg.payload.is_some(), "Kaspad P2P message should always have a value");
        match self.outgoing_route.send(msg).await {
            Ok(_r) => Ok(()),
            Err(_e) => Err(ProtocolError::ConnectionClosed),
        }
    }

    /// Closes the router, signals exit, and cleans up all resources so that underlying connections will be aborted correctly.
    /// Returns true of this is the first call to close
    pub async fn close(&self) -> bool {
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
                trace!("P2P, Router close was called more than once, router-id: {}", self.identity);
                return false;
            }
        }

        // Drop all flow senders
        self.routing_map.write().clear();

        // Send a close notification to the central Hub
        self.hub_sender.send(HubEvent::PeerClosing(self.identity)).await.expect("hub receiver should never drop before senders");

        true
    }
}

fn msg_type_string(msg: &KaspadMessage) -> String {
    match msg.payload.as_ref() {
        Some(payload) => {
            let payload_type: KaspadMessagePayloadType = payload.into();
            format!("{:?}", payload_type)
        }
        None => "<EMPTY_PAYLOAD>".to_owned(),
    }
}
