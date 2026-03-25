use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use kaspa_core::{debug, info, warn};
use kaspa_hashes::Hash;
use kaspa_utils::networking::ContextualNetAddress;
use tokio::sync::Mutex as TokioMutex;

use crate::{
    model::ftr_block::FtrBlock,
    params::{FragmentationConfig, TransportParams},
    servers::{
        auth::TokenAuthenticator,
        peer_directory::PeerDirectory,
        tcp_control::{
            PeerDirection,
            runtime::{ControlRuntime, ControlRuntimeHandle},
        },
        udp_transport::runtime::TransportRuntime,
    },
};

pub const DEFAULT_UDP_PORT: u16 = 16114;
pub const DEFAULT_TCP_PORT: u16 = 16113;

#[derive(Clone)]
pub struct FastTrustedRelay {
    /// Shared mutable state for the UDP transport; cloned handles refer to the
    /// same runtime. Protected by mutex for thread-safe start/stop.
    udp_runtime: Arc<TokioMutex<Option<TransportRuntime>>>,
    /// Handle for controlling the TCP control runtime.
    tcp_handle: ControlRuntimeHandle,
    /// Task handle for awaiting TCP runtime shutdown.
    tcp_task: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    authenticator: Arc<TokenAuthenticator>,
    directory: Arc<PeerDirectory>,
    params: TransportParams,
    fragmentation_config: FragmentationConfig,
    listen_address: SocketAddr,
    /// Notifier to wake up `recv_block` waiters when state changes.
    receive_block_waker: Arc<tokio::sync::Notify>,
    udp_port: u16,
    tcp_port: u16,
}

impl FastTrustedRelay {
    /// Create a new relay instance. Public so callers outside the crate can
    /// construct the relay; the actual transport initialization happens via
    /// [`start_control_runtime`].
    pub fn new(
        params: TransportParams,
        fragmentation_config: FragmentationConfig,
        listen_address: SocketAddr,
        secret: Vec<u8>,
        incoming_peers: Vec<ContextualNetAddress>,
        outgoing_peers: Vec<ContextualNetAddress>,
    ) -> Self {
        // create the allowlist if in incoming only -> PeerDirection::Inbound, outgoing only -> PeerDirection::Outbound, or both -> PeerDirection::Both modes.
        let mut allowlist = HashSet::new();
        let incoming_peers_set: HashSet<_> = incoming_peers.iter().collect();
        let outgoing_peers_set: HashSet<_> = outgoing_peers.iter().collect();

        for peer in incoming_peers {
            if outgoing_peers.contains(&peer) {
                allowlist.insert((SocketAddr::from((peer)).ip(), PeerDirection::Both));
            } else {
                allowlist.insert((SocketAddr::from((peer)).ip(), PeerDirection::Inbound));
            }
        }
        for peer in outgoing_peers {
            allowlist.insert((SocketAddr::from((peer)).ip(), PeerDirection::Outbound));
        }

        let directory =
            Arc::new(PeerDirectory::new(allowlist.iter().cloned().map(|(addr, direction)| (addr.into(), direction)).collect()));
        let authenticator = Arc::new(TokenAuthenticator::new(secret));
        let receive_block_waker = Arc::new(tokio::sync::Notify::new());
        let tcp_runtime = ControlRuntime::new(listen_address, directory.clone(), authenticator.clone());
        let (tcp_handle, tcp_task) = tcp_runtime.spawn();
        Self {
            listen_address,
            tcp_handle,
            tcp_task: Arc::new(TokioMutex::new(Some(tcp_task))),
            udp_runtime: Arc::new(TokioMutex::new(None)),
            authenticator,
            directory,
            params,
            fragmentation_config,
            udp_port: DEFAULT_UDP_PORT,
            tcp_port: DEFAULT_TCP_PORT,
            receive_block_waker,
        }
    }

    /// Stop the UDP relay. If the runtime is already inactive, returns false.
    /// Returns true if the stop was successful.
    ///
    /// This method uses async-safe shutdown to properly await worker thread completion.
    pub async fn stop_fast_relay(&self) -> bool {
        let mut guard = self.udp_runtime.lock().await;
        let old_runtime = guard.take();
        drop(guard); // Release lock before awaiting shutdown

        if let Some(mut runtime) = old_runtime {
            // Use async shutdown to properly await thread completion
            runtime.shutdown_async().await;

            // Wake any waiting receivers
            self.receive_block_waker.notify_waiters();

            // Tell peers that we're no longer ready
            self.tcp_handle.signal_not_ready();

            info!("fast trusted relay UDP transport stopped");
            true
        } else {
            debug!("UDP relay is already inactive.");
            false
        }
    }

    /// Start the UDP relay. If the runtime is already active, returns false.
    /// Returns true if the start was successful.
    pub async fn start_fast_relay(&self) -> bool {
        let mut udp_runtime = self.udp_runtime.lock().await;
        if udp_runtime.is_some() {
            debug!("UDP runtime is already active");
            return false;
        }

        let mut transport = TransportRuntime::new(
            self.params,
            self.listen_address,
            self.fragmentation_config,
            self.directory.clone(),
            self.authenticator.clone(),
        );

        if !transport.start() {
            warn!("UDP transport failed to start");
            return false;
        }

        *udp_runtime = Some(transport);
        drop(udp_runtime); // Release lock before notifying

        // Wake receivers after committing state
        self.receive_block_waker.notify_waiters();

        // Signal peers after we committed state
        self.tcp_handle.signal_ready();
        info!("fast trusted relay UDP transport started");
        true
    }

    /// Shut down both runtimes.
    pub async fn shutdown(&self) {
        debug!("shutting down fast trusted relay...");
        self.stop_fast_relay().await;
        self.tcp_handle.stop();

        // Await TCP task completion
        if let Some(task) = self.tcp_task.lock().await.take() {
            let _ = task.await;
        }
        info!("fast trusted relay shut down");
    }

    pub async fn broadcast_block(&self, hash: Hash, block: Arc<FtrBlock>) -> Result<(), String> {
        debug!("broadcasting block from fast trusted relay...");
        if let Some(runtime) = self.udp_runtime.lock().await.as_ref() {
            runtime.submit_block_for_broadcast(hash, block)
        } else {
            // Relay is inactive; ignore the broadcast but return Ok to avoid
            // treating this as an error.
            Ok(())
        }
    }

    pub async fn recv_block(&self) -> (Hash, FtrBlock) {
        debug!("entering receive block loop from fast trusted relay...");
        loop {
            // Register for notification BEFORE checking condition to avoid race.
            // This ensures we don't miss notifications that arrive between
            // the condition check and the await.
            let notified = self.receive_block_waker.notified();

            // Clone receiver under the lock so it doesn't disappear mid-await.
            let receiver_opt = self.udp_runtime.lock().await.as_ref().map(|rt| rt.block_receive());
            if let Some(rx_arc) = receiver_opt {
                let mut rx = rx_arc.lock().await;
                debug!("Waiting to receive block from UDP runtime...");
                if let Some(msg) = rx.recv().await {
                    return msg.into_parts();
                }
                // Receiver closed unexpectedly (runtime dropped/crashed). Clear state.
                debug!("UDP block receiver closed unexpectedly; marking relay inactive");
                self.udp_runtime.lock().await.take();
                self.receive_block_waker.notify_waiters();
                // Fall through and wait for a restart
            }
            debug!("UDP runtime not active, waiting for it to become active...");
            notified.await;
        }
    }

    /// Enable or disable the UDP relay. This is a convenience method that calls
    /// `start_fast_relay()` or `stop_fast_relay()` based on the desired state.
    /// Returns the actual resulting state (true = active, false = inactive).
    pub async fn set_udp_enabled(&self, enable: bool) -> bool {
        if enable {
            self.start_fast_relay().await;
        } else {
            self.stop_fast_relay().await;
        }
        self.is_udp_active().await
    }

    /// Returns true if the UDP runtime is currently active.
    pub async fn is_udp_active(&self) -> bool {
        self.udp_runtime.lock().await.is_some()
    }

    /// Best-effort non-blocking check if UDP is active.
    /// Returns None if the lock couldn't be acquired immediately.
    pub fn try_is_udp_active(&self) -> Option<bool> {
        self.udp_runtime.try_lock().ok().map(|rt| rt.is_some())
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::time::{Duration, timeout};

    fn make_relay() -> FastTrustedRelay {
        let params = TransportParams::default();
        let frag = FragmentationConfig::new(4, 2, 1024);
        let listen = "127.0.0.1:0".parse().unwrap();
        FastTrustedRelay::new(params, frag, listen, vec![], vec![], vec![])
    }

    #[tokio::test]
    async fn concurrent_start_stop() {
        let relay = Arc::new(make_relay());
        let mut handles = Vec::new();
        for _ in 0..10 {
            let r = relay.clone();
            handles.push(tokio::spawn(async move {
                // These calls are serialized internally via the state machine
                r.start_fast_relay().await;
                r.stop_fast_relay().await;
            }));
        }
        for h in handles {
            let _ = h.await;
        }
        assert!(!relay.is_udp_active().await);
    }

    #[tokio::test]
    async fn lifecycle_state_transitions() {
        let relay = make_relay();
        assert!(!relay.is_udp_active().await);

        relay.start_fast_relay().await;
        assert!(relay.is_udp_active().await);

        relay.stop_fast_relay().await;
        assert!(!relay.is_udp_active().await);

        // Can restart
        relay.start_fast_relay().await;
        assert!(relay.is_udp_active().await);
    }

    #[tokio::test]
    async fn set_udp_enabled_toggle() {
        let relay = make_relay();

        // Enable
        let result = relay.set_udp_enabled(true).await;
        assert!(result);
        assert!(relay.is_udp_active().await);

        // Disable
        let result = relay.set_udp_enabled(false).await;
        assert!(!result);
        assert!(!relay.is_udp_active().await);

        // Re-enable
        let result = relay.set_udp_enabled(true).await;
        assert!(result);
        assert!(relay.is_udp_active().await);
    }

    #[tokio::test]
    async fn recv_block_handles_runtime_drop() {
        let relay = make_relay();
        relay.start_fast_relay().await;
        assert!(relay.is_udp_active().await);

        // Simulate runtime disappearing without calling stop_fast_relay
        relay.udp_runtime.lock().await.take();

        // Now inactive
        assert!(!relay.is_udp_active().await);

        // recv_block should wait for restart (times out in test)
        let relay2 = relay.clone();
        let handle = tokio::spawn(async move { tokio::time::timeout(Duration::from_millis(100), relay2.recv_block()).await });
        assert!(handle.await.unwrap().is_err());
    }

    #[tokio::test]
    async fn idempotent_start_stop() {
        let relay = make_relay();

        // Multiple stops when inactive are no-ops
        assert!(!relay.stop_fast_relay().await);
        assert!(!relay.stop_fast_relay().await);

        // Start succeeds
        assert!(relay.start_fast_relay().await);

        // Multiple starts when active are no-ops
        assert!(!relay.start_fast_relay().await);
        assert!(!relay.start_fast_relay().await);

        // Stop succeeds
        assert!(relay.stop_fast_relay().await);

        // Multiple stops are no-ops again
        assert!(!relay.stop_fast_relay().await);
    }

    // waker behaviour is lightly exercised by the concurrency test and by
    // observing that broadcast_block/recv_block work in integration; a full
    // injection test requires poking private channels and is deferred.

    #[test]
    fn sanity_check() {
        // make sure the harness picks up at least one test
        assert_eq!(2 + 2, 4);
    }
}
