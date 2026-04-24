use std::sync::atomic::{AtomicBool, Ordering};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};

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
    /// Unspawned TCP control runtime. Stored until spawn_tcp_runtime() is called
    /// within an async context where Tokio runtime is available.
    tcp_runtime: Arc<TokioMutex<Option<ControlRuntime>>>,
    /// Handle for controlling the TCP control runtime (populated after spawn).
    tcp_handle: Arc<TokioMutex<Option<ControlRuntimeHandle>>>,
    /// Task handle for awaiting TCP runtime shutdown.
    tcp_task: Arc<TokioMutex<Option<tokio::task::JoinHandle<()>>>>,
    authenticator: Arc<TokenAuthenticator>,
    directory: Arc<PeerDirectory>,
    params: TransportParams,
    fragmentation_config: FragmentationConfig,
    listen_address: SocketAddr,
    /// Notifier to wake up `recv_block` waiters when state changes.
    receive_block_waker: Arc<tokio::sync::Notify>,
    /// Flag to prevent restarts during shutdown.
    shutting_down: Arc<AtomicBool>,
    _udp_port: u16,
    _tcp_port: u16,
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
        // Build allowlist as HashMap<IpAddr, PeerDirection> directly to avoid duplicates.
        // Priority: Both > Outbound > Inbound (peers in both lists get Both direction)
        let mut allowlist: HashMap<std::net::IpAddr, PeerDirection> = HashMap::new();

        // First add all outgoing peers
        for peer in &outgoing_peers {
            let ip = SocketAddr::from(*peer).ip();
            allowlist.insert(ip, PeerDirection::Outbound);
        }

        // Then process incoming peers - upgrade to Both if already exists as Outbound
        for peer in &incoming_peers {
            let ip = SocketAddr::from(*peer).ip();
            allowlist
                .entry(ip)
                .and_modify(|dir| {
                    if *dir == PeerDirection::Outbound {
                        *dir = PeerDirection::Both;
                    }
                })
                .or_insert(PeerDirection::Inbound);
        }

        info!("Fast trusted relay allowlist: {:?}", allowlist);

        let directory = Arc::new(PeerDirectory::new(allowlist));
        let authenticator = Arc::new(TokenAuthenticator::new(secret));
        let receive_block_waker = Arc::new(tokio::sync::Notify::new());
        // Create the TCP runtime but don't spawn it yet - that requires an active Tokio runtime
        let tcp_runtime = ControlRuntime::new(listen_address, directory.clone(), authenticator.clone());
        Self {
            listen_address,
            tcp_runtime: Arc::new(TokioMutex::new(Some(tcp_runtime))),
            tcp_handle: Arc::new(TokioMutex::new(None)),
            tcp_task: Arc::new(TokioMutex::new(None)),
            udp_runtime: Arc::new(TokioMutex::new(None)),
            authenticator,
            directory,
            params,
            fragmentation_config,
            // TODO: make this configurable via kaspad args.
            _udp_port: DEFAULT_UDP_PORT,
            _tcp_port: DEFAULT_TCP_PORT,
            receive_block_waker,
            shutting_down: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Spawn the TCP control runtime. Must be called from within an async context
    /// where a Tokio runtime is active. No-op if already spawned.
    pub async fn spawn_tcp_runtime(&self) {
        let mut runtime_guard = self.tcp_runtime.lock().await;
        if let Some(runtime) = runtime_guard.take() {
            let (handle, task) = runtime.spawn();
            *self.tcp_handle.lock().await = Some(handle);
            *self.tcp_task.lock().await = Some(task);
            info!("TCP control runtime spawned");
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
            if let Some(handle) = self.tcp_handle.lock().await.as_ref() {
                handle.signal_not_ready();
            }

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
        // Prevent restarts during shutdown
        if self.shutting_down.load(Ordering::SeqCst) {
            debug!("Cannot start UDP runtime - shutdown in progress");
            return false;
        }

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
        if let Some(handle) = self.tcp_handle.lock().await.as_ref() {
            handle.signal_ready();
        }
        info!("fast trusted relay UDP transport started");
        true
    }

    /// Shut down both runtimes.
    pub async fn shutdown(&self) {
        debug!("shutting down fast trusted relay...");
        // Prevent any new start_fast_relay calls
        self.shutting_down.store(true, Ordering::SeqCst);

        self.stop_fast_relay().await;

        // Stop TCP runtime if it was spawned
        if let Some(handle) = self.tcp_handle.lock().await.as_ref() {
            handle.stop();
        }

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

                // Use select! to race recv() against state change notifications.
                // This ensures we re-check the runtime if it's replaced while waiting,
                // avoiding the stale receiver problem where we wait on an old channel
                // while the new runtime publishes to a different one.
                tokio::select! {
                    biased;
                    _ = notified => {
                        // Runtime state changed (stop/restart), re-check and get fresh receiver
                        debug!("recv_block: notified of state change, re-acquiring receiver");
                    }
                    result = rx.recv() => {
                        if let Some(msg) = result {
                            return msg.into_parts();
                        }
                        // Receiver closed unexpectedly (runtime dropped/crashed). Clear state.
                        debug!("UDP block receiver closed unexpectedly; marking relay inactive");
                        self.udp_runtime.lock().await.take();
                        self.receive_block_waker.notify_waiters();
                        // Loop will re-register notified and wait for restart
                    }
                }
            } else {
                // No runtime active, wait for notification that one has started
                debug!("UDP runtime not active, waiting for it to become active...");
                notified.await;
            }
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
