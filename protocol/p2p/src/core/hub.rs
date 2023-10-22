use crate::{common::ProtocolError, pb::KaspadMessage, ConnectionInitializer, Peer, Router};
use kaspa_core::{debug, info, warn};
use parking_lot::RwLock;
use std::{
    collections::{hash_map::Entry::Occupied, HashMap},
    sync::Arc,
};
use tokio::sync::mpsc::Receiver as MpscReceiver;

use super::peer::PeerKey;

#[derive(Debug)]
pub(crate) enum HubEvent {
    NewPeer(Arc<Router>),
    PeerClosing(Arc<Router>),
}

/// Hub of active peers (represented as Router objects). Note that all public methods of this type are exposed through the Adaptor
#[derive(Debug, Clone)]
pub struct Hub {
    /// Map of currently active peers
    ///
    /// Note: the map key holds the node id and IP to prevent node impersonating.
    pub(crate) peers: Arc<RwLock<HashMap<PeerKey, Arc<Router>>>>,
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
                        // If peer is outbound then connection initialization was already performed as part of the connect logic
                        if new_router.is_outbound() {
                            info!("P2P Connected to outgoing peer {}", new_router);
                            self.insert_new_router(new_router).await;
                        } else {
                            match initializer.initialize_connection(new_router.clone()).await {
                                Ok(()) => {
                                    info!("P2P Connected to incoming peer {}", new_router);
                                    self.insert_new_router(new_router).await;
                                }
                                Err(err) => {
                                    new_router.try_sending_reject_message(&err).await;
                                    // Ignoring the new router
                                    new_router.close().await;
                                    if matches!(err, ProtocolError::LoopbackConnection(_) | ProtocolError::PeerAlreadyExists(_)) {
                                        debug!("P2P, handshake failed for inbound peer {}: {}", new_router, err);
                                    } else {
                                        warn!("P2P, handshake failed for inbound peer {}: {}", new_router, err);
                                    }
                                }
                            }
                        }
                    }
                    HubEvent::PeerClosing(router) => {
                        if let Occupied(entry) = self.peers.write().entry(router.key()) {
                            // We search for the router by identity, but make sure to delete it only if it's actually the same object.
                            // This is extremely important in cases of duplicate connection rejection etc.
                            if Arc::ptr_eq(entry.get(), &router) {
                                entry.remove_entry();
                                debug!("P2P, Hub event loop, removing peer, router-id: {}", router.identity());
                            }
                        }
                    }
                }
            }
            debug!("P2P, Hub event loop exiting");
        });
    }

    async fn insert_new_router(&self, new_router: Arc<Router>) {
        let prev = self.peers.write().insert(new_router.key(), new_router);
        if let Some(previous_router) = prev {
            // This is not supposed to ever happen but can on rare race-conditions
            previous_router.close().await;
            warn!("P2P, Hub event loop, removing peer with duplicate key: {}", previous_router.key());
        }
    }

    /// Send a message to a specific peer
    pub async fn send(&self, peer_key: PeerKey, msg: KaspadMessage) -> Result<bool, ProtocolError> {
        let op = self.peers.read().get(&peer_key).cloned();
        if let Some(router) = op {
            router.enqueue(msg).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Broadcast a message to all peers
    pub async fn broadcast(&self, msg: KaspadMessage) {
        let peers = self.peers.read().values().cloned().collect::<Vec<_>>();
        for router in peers {
            let _ = router.enqueue(msg.clone()).await;
        }
    }

    /// Broadcast a vector of messages to all peers
    pub async fn broadcast_many(&self, msgs: Vec<KaspadMessage>) {
        if msgs.is_empty() {
            return;
        }
        let peers = self.peers.read().values().cloned().collect::<Vec<_>>();
        for router in peers {
            for msg in msgs.iter().cloned() {
                let _ = router.enqueue(msg).await;
            }
        }
    }

    /// Terminate a specific peer
    pub async fn terminate(&self, peer_key: PeerKey) {
        let op = self.peers.read().get(&peer_key).cloned();
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

    /// Returns whether there are currently active peers
    pub fn has_peers(&self) -> bool {
        !self.peers.read().is_empty()
    }

    /// Returns whether a peer matching `peer_key` is registered
    pub fn has_peer(&self, peer_key: PeerKey) -> bool {
        self.peers.read().contains_key(&peer_key)
    }
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}
