use std::{
    collections::HashSet,
    net::SocketAddr,
    ops::Deref,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

use kaspa_core::{debug, info, task::service::AsyncService, warn};
use kaspa_hashes::Hash;
use kaspa_utils::networking::ContextualNetAddress;
use tokio::sync::Mutex as TokioMutex;

use crate::{
    model::ftr_block::FtrBlock,
    params::{FragmentationConfig, TransportParams},
    servers::{
        auth::TokenAuthenticator,
        peer_directory::{Allowlist, PeerDirectory},
        tcp_control::{PeerDirection, runtime::ControlRuntime},
        udp_transport::runtime::TransportRuntime,
    },
};

pub const DEFAULT_UDP_PORT: u16 = 16114;
pub const DEFAULT_TCP_PORT: u16 = 16113;

#[derive(Clone)]
pub struct FastTrustedRelay {
    udp_runtime: Option<Arc<TransportRuntime>>,
    tcp_runtime: Arc<TokioMutex<ControlRuntime>>,
    authenticator: Arc<TokenAuthenticator>,
    directory: Arc<PeerDirectory>,
    params: TransportParams,
    fragmentation_config: FragmentationConfig,
    listen_address: SocketAddr,
    udp_active: Arc<AtomicBool>,
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
        let is_udp_active = Arc::new(AtomicBool::new(false));
        let receive_block_waker = Arc::new(tokio::sync::Notify::new());
        let tcp_runtime = ControlRuntime::new(listen_address, directory.clone(), authenticator.clone(), is_udp_active.clone());
        Self {
            listen_address,
            tcp_runtime: Arc::new(TokioMutex::new(tcp_runtime)),
            udp_runtime: None,
            authenticator,
            directory,
            params,
            fragmentation_config,
            udp_port: DEFAULT_UDP_PORT,
            tcp_port: DEFAULT_TCP_PORT,
            udp_active: is_udp_active,
            receive_block_waker,
        }
    }

    pub async fn start_control_runtime(&mut self) {
        info!("Starting TCP control runtime...");
        let tcp_runtime = self.tcp_runtime.clone();
        let mut rt = tcp_runtime.lock().await;
        rt.run().await;
    }

    /// stop the UDP relay without consuming the struct so the caller can still
    /// use the relay instance afterwards.
    pub async fn stop_fast_relay(&mut self) {
        if !self.toggle_udp_active(false) {
            debug!("trying to stop fast trusted relay although it is not active");
            return;
        }

        // drops the runtime, which frees the resources
        self.udp_runtime.take();
        // signal to peers that the relay is not ready to receive blocks.
        self.tcp_runtime.lock().await.signal_not_ready().await;
        info!("fast trusted relay UDP transport stopped");
    }

    /// start or restart the UDP relay; takes `&mut self` to avoid moving the
    /// entire relay instance out of the caller.
    pub async fn start_fast_relay(&mut self) {
        if self.toggle_udp_active(true) {
            debug!("trying to start fast trusted relay although it is already active");
            return;
        }
        let mut udp_runtime = TransportRuntime::new(
            self.params,
            self.listen_address,
            self.fragmentation_config,
            self.directory.clone(),
            self.authenticator.clone(),
            self.udp_active.clone(),
        );
        udp_runtime.start();
        self.udp_runtime = Some(Arc::new(udp_runtime));
        // signal to peers that the relay is ready to receive blocks.
        self.tcp_runtime.lock().await.signal_ready().await;
        self.receive_block_waker.notify_waiters();
        info!("fast trusted relay UDP transport started");
    }

    /// shut down both runtimes; does not consume the relay in order to allow
    /// callers (including `Drop`) to borrow it.
    pub fn shutdown(&mut self) {
        debug!("shutting down fast trusted relay...");
        let mut self_clone = self.clone();
        tokio::spawn(async move {
            self_clone.stop_fast_relay().await;
            // only move the control runtime into the spawned task, keeping the
            // rest of `self` live.
            let tcp_runtime = self_clone.tcp_runtime.clone();
            let mut rt = tcp_runtime.lock().await;
            rt.stop().await;
        });
    }

    pub async fn broadcast_block(&self, hash: Hash, block: Arc<FtrBlock>) -> Result<(), String> {
        debug!("broadcasting block from fast trusted relay...");
        if let Some(udp_runtime) = &self.udp_runtime {
            udp_runtime.submit_block_for_broadcast(hash, block)
        } else {
            // Relay is inactive; ignore the broadcast but return Ok to avoid
            // treating this as an error.
            Ok(())
        }
    }

    pub async fn recv_block(&self) -> (Hash, FtrBlock) {
        debug!("entering receive block loop from fast trusted relay...");
        loop {
            if let Some(udp_runtime) = &self.udp_runtime {
                let block_receiver_arc = udp_runtime.block_receive();
                let mut block_receiver = block_receiver_arc.lock().await;
                debug!("Waiting to receive block from UDP runtime...");
                if let Some(msg) = block_receiver.recv().await {
                    return msg.into_parts();
                }
            }
            debug!("UDP runtime not active, waiting for it to become active...");
            // wait until the udp runtime becomes active again.
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    pub fn toggle_udp_active(&self, toggle: bool) -> bool {
        debug!("checking if UDP runtime is active: {}", self.udp_active.load(std::sync::atomic::Ordering::SeqCst));
        let old = self.udp_active.swap(toggle, std::sync::atomic::Ordering::SeqCst);
        old
    }

    pub fn is_udp_active(&self) -> bool {
        self.udp_active.load(std::sync::atomic::Ordering::SeqCst)
    }
}

// The relay is a cheap, cloneable handle; clones share the same
// underlying control/udp runtimes via `Arc`.  We used to shut the transport
// down on _every_ drop, which meant that creating a temporary clone (for
// example when registering the relay flow) would immediately trigger a shutdown
// task.  That task would then block waiting for a future `start_fast_relay` and
// eventually starve the tokio runtime – leaving kaspad apparently hung after
// IBD.
//
// Instead we only perform a shutdown when the _last_ strong reference disappears.
// At drop time the only thing we can observe synchronously is the current
// strong count; if it is one then this handle is the final owner and we can
// safely spawn the background task to tear everything down.  If there are others
// remaining we simply do nothing and let the real owner clean up later.
impl Drop for FastTrustedRelay {
    fn drop(&mut self) {
        // tcp_runtime is always present; udp_runtime is optional
        let tcp_refs = Arc::strong_count(&self.tcp_runtime);
        let udp_refs = self.udp_runtime.as_ref().map_or(0, |rt| Arc::strong_count(rt));

        // when both counts are 1 (i.e. only `self` holds the reference) we are
        // the last handle.
        if tcp_refs == 1 && udp_refs <= 1 {
            debug!("last FastTrustedRelay handle dropped, performing shutdown (tcp={}, udp={})", tcp_refs, udp_refs);
            self.shutdown();
        }
    }
}
