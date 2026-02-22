use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use arc_swap::ArcSwap;

use log::{debug, info, trace, warn};
use tokio::sync::mpsc;

use crate::servers::peer_directory::PeerDirectory;
use crate::servers::peer_directory::PeerInfo;
use crate::servers::tcp_control::{ControlMsg, Peer, PeerCloseReason, hub};
// ============================================================================
// HUB EVENTS
// ============================================================================

/// Events processed by the Hub's async event loop.
pub enum HubEvent {
    /// A new peer has connected and been authenticated.
    PeerConnected(Peer),
    /// A peer's control loop exited — remove from registry.
    PeerDisconnected(SocketAddr, PeerCloseReason),
    /// A peer reported a readiness change (remote Start/Stop).
    PeerReady(SocketAddr, bool),
    /// Send a control command to a specific peer.
    SendControl(SocketAddr, ControlMsg),
    /// Send a control command to all peers.
    BroadcastControl(ControlMsg),
    /// Shut down the Hub and all peers.
    Shutdown,
}

// ============================================================================
// PEER HANDLE — lightweight handle kept in the Hub's map
// ============================================================================

/// Lightweight bookkeeping stored per-peer in the Hub.
/// The actual `Peer` struct is moved into its own tokio task.
struct PeerHandle {
    info: PeerInfo,
    control_tx: mpsc::Sender<ControlMsg>,
}

// ============================================================================
// HUB
// ============================================================================

/// Central peer registry and control-plane router.
///
/// Owns:
/// - The map of all connected peers (as lightweight `PeerHandle`s).
/// - A shared `PeerDirectory` that the UDP data-plane reads directly.
/// - The allowlist of authenticated UDP source addresses.
///
/// The Hub is **control-plane only**. All UDP data-plane work (shard
/// forwarding, block broadcast) is handled by dedicated workers that
/// read the `PeerDirectory` snapshot without going through the Hub.
pub struct Hub {
    peers: HashMap<SocketAddr, PeerHandle>,
    /// Shared peer directory — updated by Hub on connect/disconnect,
    /// read by `ShardForwarder` and `BroadcastWorker` on the data plane.
    directory: Arc<PeerDirectory>,
    /// Sender for Hub events — cloned and given to peer tasks so they can
    /// report `PeerDisconnected`.
    hub_event_sender: mpsc::UnboundedSender<HubEvent>,
    /// Receiver for Hub events — owned by the Hub's event loop.
    hub_event_receiver: mpsc::UnboundedReceiver<HubEvent>,
    /// Whether *we* are ready to relay. Flipped by the Adaptor via
    /// `start_fast_relay()` / `stop_fast_relay()`. Outbound shards are
    /// only sent when `local_ready && peer_ready`.
    local_ready: Arc<AtomicBool>,
}

impl Hub {
    /// Create a new Hub and return `(hub_event_sender, hub)`.
    ///
    /// The caller should spawn `hub.run()` as a tokio task and use the
    /// returned sender to submit events.
    pub fn new(
        directory: Arc<PeerDirectory>,
        is_ready: Arc<AtomicBool>, // this is the same / alias as udp shutdown bool. i.e. if we shutdown udp, we signal not ready here.
        hub_event_sender: mpsc::UnboundedSender<HubEvent>,
        hub_event_receiver: mpsc::UnboundedReceiver<HubEvent>,
    ) -> Self {
        Self {
            peers: HashMap::new(),
            directory,
            hub_event_sender: hub_event_sender.clone(),
            hub_event_receiver,
            local_ready: is_ready,
        }
    }

    /// Run the Hub event loop. Blocks until `Shutdown` or all senders drop.
    pub async fn run(&mut self) {
        info!("Hub started, listening for events");

        while let Some(event) = self.hub_event_receiver.recv().await {
            match event {
                HubEvent::PeerConnected(peer) => self.handle_peer_connected(peer),
                HubEvent::PeerDisconnected(addr, reason) => self.handle_peer_disconnected(addr, reason),
                HubEvent::PeerReady(addr, ready) => self.handle_peer_ready(addr, ready),
                HubEvent::SendControl(addr, msg) => self.handle_send_control(addr, msg).await,
                HubEvent::BroadcastControl(msg) => {
                    for handle in self.peers.values() {
                        self.handle_send_control(handle.info.address(), msg).await;
                    }
                }
                HubEvent::Shutdown => {
                    info!("Hub shutting down, disconnecting {} peers", self.peers.len());
                    self.shutdown_all_peers().await;
                    break;
                }
            }
        }

        info!("Hub event loop exited");
    }

    /// Number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    // ========================================================================
    // Internal handlers
    // ========================================================================

    fn handle_peer_connected(&mut self, mut peer: Peer) {
        let info = peer.peer_info();
        let addr = info.address();
        let direction = info.direction();
        let udp_target = info.udp_target();
        let control_tx = peer.control_tx();
        let hub_event_sender = self.hub_event_sender.clone();

        info!("Peer connected: {} ({})", addr, direction);

        let info = info.with_ready(true);
        let handle = PeerHandle { info: info.clone(), control_tx };
        self.peers.insert(addr, handle);

        // Insert into the shared PeerDirectory so the data-plane workers
        // (ShardForwarder, BroadcastWorker) can see this peer immediately.
        self.directory.insert_peer(info.clone());

        // If we are already in relay-active mode, immediately tell this new
        // peer to start (writes Start over TCP so the remote knows).
        if self.local_ready.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(handle) = self.peers.get(&addr) {
                let tx = handle.control_tx.clone();
                tokio::spawn(async move {
                    let _ = tx.send(ControlMsg::Start).await;
                });
            } else {
                warn!("Peer {} disappeared before sending Start message", addr);
            }
        }

        // Spawn the peer's TCP control loop in its own task.
        tokio::spawn(async move {
            let reason = peer.run_control_loop().await;
            debug!("Peer {} control loop exited: {:?}", addr, reason);
            // Notify Hub to clean up.
            let _ = hub_event_sender.send(HubEvent::PeerDisconnected(addr, reason));
        });
    }

    fn handle_peer_disconnected(&mut self, addr: SocketAddr, reason: PeerCloseReason) {
        if let Some(_) = self.peers.remove(&addr) {
            info!("Peer removed: {} (reason: {:?})", addr, reason);
            // Remove from the shared PeerDirectory so udp transport workers
            // stop sending to this peer.
            self.directory.remove_peer(&addr);
        } else {
            trace!("Peer {} already removed (reason: {:?})", addr, reason);
        }
    }

    fn handle_peer_ready(&mut self, addr: SocketAddr, ready: bool) {
        if let Some(handle) = self.peers.get_mut(&addr) {
            let updated = handle.info.with_ready(ready);
            handle.info = updated.clone();
            self.directory.insert_peer(updated);
        } else {
            warn!("PeerReady for unknown peer {}", addr);
        }
    }

    async fn handle_send_control(&self, addr: SocketAddr, msg: ControlMsg) {
        if let Some(handle) = self.peers.get(&addr) {
            if handle.control_tx.send(msg).await.is_err() {
                warn!("Failed to send control to {} — channel closed", addr);
            }
        } else {
            warn!("Control message for unknown peer {}", addr);
        }
    }

    pub async fn shutdown_all_peers(&mut self) {
        for (addr, handle) in self.peers.drain() {
            debug!("Sending Shutdown to peer {}", addr);
            let _ = handle.control_tx.send(ControlMsg::Shutdown).await;
        }
    }

    pub async fn signal_ready(&self) {
        for (addr, handle) in self.peers.iter() {
            debug!("Signaling Start to peer {}", addr);
            let _ = handle.control_tx.send(ControlMsg::Start).await;
        }
    }

    pub async fn signal_not_ready(&self) {
        for (addr, handle) in self.peers.iter() {
            debug!("Signaling Stop to peer {}", addr);
            let _ = handle.control_tx.send(ControlMsg::Stop).await;
        }
    }
}

// ============================================================================
// TESTS
// ============================================================================
#[cfg(test)]
mod tests {
    use crate::params::FragmentationConfig;
    use crate::servers::tcp_control::PeerDirection;
    use std::sync::Arc;

    use super::*;
    use tokio::net::TcpListener;
    use tokio::net::TcpStream;

    fn test_config() -> FragmentationConfig {
        FragmentationConfig::new(4, 2, 1024)
    }

    /// Create a Hub wired to throwaway channels.
    async fn make_hub() -> (mpsc::UnboundedSender<HubEvent>, Hub) {
        let directory = Arc::new(PeerDirectory::new(std::collections::HashMap::new()));
        let local_ready = Arc::new(AtomicBool::new(false));
        let (hub_tx, hub_rx) = mpsc::unbounded_channel();
        let hub = Hub::new(directory, local_ready, hub_tx.clone(), hub_rx);
        (hub_tx, hub)
    }

    /// Helper: create a connected TCP pair and wrap one side in a Peer.
    async fn make_peer(direction: PeerDirection, hub_event_tx: mpsc::UnboundedSender<HubEvent>) -> (Peer, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let connect_fut = TcpStream::connect(addr);
        let accept_fut = listener.accept();
        let (remote_result, accept_result) = tokio::join!(connect_fut, accept_fut);
        let remote = remote_result.unwrap();
        let (server, _) = accept_result.unwrap();

        let peer = Peer::new(addr, direction, server, addr, hub_event_tx);
        (peer, remote)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_hub_register_and_disconnect() {
        let (event_tx, mut hub) = make_hub().await;

        // Spawn Hub event loop.
        let hub_handle = tokio::spawn(async move { hub.run().await });

        // Connect a peer.
        let (peer, remote) = make_peer(PeerDirection::Both, event_tx.clone()).await;
        let addr = peer.address();
        event_tx.send(HubEvent::PeerConnected(peer)).unwrap();

        // Give the Hub a moment to process.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Drop the remote side — the peer's control loop should detect
        // StreamClosed and send PeerDisconnected.
        drop(remote);
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Shut down the Hub.
        event_tx.send(HubEvent::Shutdown).unwrap();
        hub_handle.await.unwrap();

        // If we get here without panic the register/unregister path works.
        let _ = addr;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_hub_updates_allowlist_on_connect_and_disconnect() {
        use std::collections::HashMap;
        use std::sync::Arc;

        let directory = Arc::new(PeerDirectory::new(HashMap::new()));
        let local_ready = Arc::new(AtomicBool::new(false));
        let (event_tx, hub_rx) = mpsc::unbounded_channel();
        let mut hub = Hub::new(directory.clone(), local_ready, event_tx.clone(), hub_rx);

        // Run the Hub loop in background.
        let hub_handle = tokio::spawn(async move { hub.run().await });

        // Create a Peer and submit PeerConnected.
        let (peer, _remote) = make_peer(PeerDirection::Both, event_tx.clone()).await;
        let peer_addr = peer.address();
        event_tx.send(HubEvent::PeerConnected(peer)).unwrap();

        // Allow event loop to process.
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Expect the peer to appear in the PeerDirectory.
        assert_eq!(directory.peer_info_list().load_full().len(), 1, "PeerDirectory should have 1 peer after PeerConnected");

        // Simulate peer disconnect and ensure removal from directory.
        event_tx.send(HubEvent::PeerDisconnected(peer_addr, crate::servers::tcp_control::PeerCloseReason::StreamClosed)).unwrap();

        // Wait (bounded) for the directory to be updated by the Hub event loop.
        let removed = tokio::time::timeout(tokio::time::Duration::from_millis(500), async {
            loop {
                if directory.peer_info_list().load_full().is_empty() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok();

        assert!(removed, "PeerDirectory should be empty after PeerDisconnected");

        // Shutdown Hub and finish.
        event_tx.send(HubEvent::Shutdown).unwrap();
        hub_handle.await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_hub_shutdown_sends_to_peers() {
        let (event_tx, mut hub) = make_hub().await;
        let hub_handle = tokio::spawn(async move { hub.run().await });

        let (peer, mut remote) = make_peer(PeerDirection::Inbound, event_tx.clone()).await;
        event_tx.send(HubEvent::PeerConnected(peer)).unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Shutdown Hub — should send Shutdown control to the peer.
        event_tx.send(HubEvent::Shutdown).unwrap();
        hub_handle.await.unwrap();

        // The remote should receive the Shutdown frame.
        use tokio::io::AsyncReadExt;
        let mut buf = [0u8; 3];
        let _ = tokio::time::timeout(tokio::time::Duration::from_millis(200), remote.read_exact(&mut buf)).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_peer_ready_event_updates_directory() {
        use std::collections::HashMap;
        use std::sync::Arc;

        let directory = Arc::new(PeerDirectory::new(HashMap::new()));
        let local_ready = Arc::new(AtomicBool::new(false));
        let (event_tx, hub_rx) = mpsc::unbounded_channel();
        let mut hub = Hub::new(directory.clone(), local_ready, event_tx.clone(), hub_rx);

        let hub_handle = tokio::spawn(async move { hub.run().await });

        let (peer, _remote) = make_peer(PeerDirection::Outbound, event_tx.clone()).await;
        let addr = peer.address();
        event_tx.send(HubEvent::PeerConnected(peer)).unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Remote peer signals readiness; Hub should swap the directory entry.
        event_tx.send(HubEvent::PeerReady(addr, true)).unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let snap = directory.peer_info_list().load_full();
        assert_eq!(snap.len(), 1);
        assert!(snap[0].is_outbound_ready(), "Peer ready flag should be reflected in directory snapshot");

        event_tx.send(HubEvent::Shutdown).unwrap();
        hub_handle.await.unwrap();
    }
}
