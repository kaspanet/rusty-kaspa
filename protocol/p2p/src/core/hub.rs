use crate::{common::ProtocolError, pb::KaspadMessage, ConnectionInitializer, Peer, Router};
use kaspa_core::{debug, info};
use parking_lot::RwLock;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc::Receiver as MpscReceiver;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) enum HubEvent {
    NewPeer(Arc<Router>),
    PeerClosing(Uuid),
    Broadcast(Box<KaspadMessage>),
}

/// Hub of active peers (represented as Router objects). Note that all public methods of this type are exposed through the Adaptor
#[derive(Debug, Clone)]
pub struct Hub {
    /// Map of currently active peers
    pub(crate) peers: Arc<RwLock<HashMap<Uuid, Arc<Router>>>>,
}

impl Hub {
    pub fn new() -> Self {
        Self { peers: Arc::new(RwLock::new(HashMap::new())) }
    }

    /// Starts a loop for receiving central hub events from all peer routers. This mechanism is used for
    /// managing a collection of active peers and for supporting a broadcast operation.
    pub(crate) fn start_event_loop(self, mut hub_receiver: MpscReceiver<HubEvent>, initializer: Arc<dyn ConnectionInitializer>) {
        tokio::spawn(async move {
            while let Some(new_event) = hub_receiver.recv().await {
                match new_event {
                    HubEvent::NewPeer(new_router) => {
                        match initializer.initialize_connection(new_router.clone()).await {
                            Ok(_) => {
                                info!("P2P Connected to {}", new_router);
                                self.peers.write().insert(new_router.identity(), new_router);
                            }
                            Err(err) => {
                                // Ignoring the router
                                new_router.close().await;
                                debug!("P2P, flow initialization for router-id {:?} failed: {}", new_router.identity(), err);
                            }
                        }
                    }
                    HubEvent::PeerClosing(peer_id) => {
                        if let Some(router) = self.peers.write().remove(&peer_id) {
                            debug!("P2P, Hub event loop, removing peer, router-id: {}", router.identity());
                        }
                    }
                    HubEvent::Broadcast(msg) => {
                        self.broadcast(*msg).await;
                    }
                }
            }
            debug!("P2P, Hub event loop exiting");
        });
    }

    /// Send a message to a specific peer
    pub async fn send(&self, peer_id: Uuid, msg: KaspadMessage) -> Result<bool, ProtocolError> {
        let op = self.peers.read().get(&peer_id).cloned();
        if let Some(router) = op {
            router.enqueue(msg).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Broadcast a message to all peers. Note that broadcast can also be called on a
    /// specific router and will eventually lead to the same call (via the hub event loop)
    pub async fn broadcast(&self, msg: KaspadMessage) {
        let peers = self.peers.read().values().cloned().collect::<Vec<_>>();
        for router in peers {
            let _ = router.enqueue(msg.clone()).await;
        }
    }

    /// Terminate a specific peer
    pub async fn terminate(&self, peer_id: Uuid) {
        let op = self.peers.read().get(&peer_id).cloned();
        if let Some(router) = op {
            // This will eventually lead to peer removal through the Hub event loop
            router.close().await;
        }
    }

    /// Terminate all peers
    pub async fn terminate_all_peers(&self) {
        let peers = self.peers.write().drain().map(|(_, r)| r).collect::<Vec<_>>();
        for router in peers {
            router.close().await;
        }
    }

    /// Returns a list of all currently active peers
    pub fn active_peers(&self) -> Vec<Peer> {
        self.peers.read().values().map(|r| r.as_ref().into()).collect()
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
