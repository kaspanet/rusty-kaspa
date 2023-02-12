use crate::core::hub::HubEvent;
use crate::pb::KaspadMessage;
use crate::KaspadMessagePayloadType;
use kaspa_core::{debug, error, trace, warn};
use parking_lot::{Mutex, RwLock};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::{channel as mpsc_channel, Receiver as MpscReceiver, Sender as MpscSender};
use tokio::sync::oneshot::{channel as oneshot_channel, Sender as OneshotSender};
use tokio_stream::StreamExt;
use tonic::Streaming;
use uuid::Uuid;

#[derive(Debug)]
struct RouterMutableState {
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
    routing_map: RwLock<HashMap<KaspadMessagePayloadType, MpscSender<KaspadMessage>>>,

    /// The outgoing route for sending messages to this peer
    outgoing_route: MpscSender<KaspadMessage>,

    /// A channel sender for internal event management. Used to send information from each router to a central hub object
    hub_sender: MpscSender<HubEvent>,

    /// Used for managing router mutable state
    mutable_state: Mutex<RouterMutableState>,
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
            mutable_state: Mutex::new(RouterMutableState { start_signal: Some(start_sender), shutdown_signal: Some(shutdown_sender) }),
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

    fn incoming_flow_channel_size() -> usize {
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

    /// Subscribe to specific message types. This should be used by `ClientInitializer` instances to register application-specific flows
    pub fn subscribe(&self, msg_types: Vec<KaspadMessagePayloadType>) -> MpscReceiver<KaspadMessage> {
        let (sender, receiver) = mpsc_channel(Self::incoming_flow_channel_size());
        let mut map = self.routing_map.write();
        for msg_type in msg_types {
            match map.insert(msg_type, sender.clone()) {
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
        let msg_type: KaspadMessagePayloadType = msg.payload.as_ref().unwrap().into();
        let op = self.routing_map.read().get(&msg_type).cloned();
        if let Some(sender) = op {
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
            let op = self.mutable_state.lock().shutdown_signal.take();
            if let Some(signal) = op {
                let _ = signal.send(());
            } else {
                // This means the router was already closed
                debug!("P2P, Router close was called more than once, router-id: {}", self.identity);
                return;
            }
        }

        // Drop all flow senders
        self.routing_map.write().clear();

        // Send a close notification to the central Hub
        self.hub_sender.send(HubEvent::PeerClosing(self.identity)).await.unwrap();
    }
}
