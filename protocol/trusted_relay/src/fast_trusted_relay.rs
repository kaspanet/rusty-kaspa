use std::{
    collections::HashSet, net::SocketAddr, ops::Deref, sync::{Arc, atomic::AtomicBool}, time::Duration
};

use kaspa_core::{info, warn};
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
    async fn new(
        params: TransportParams,
        fragmentation_config: FragmentationConfig,
        listen_address: SocketAddr,
        udp_port: u16,
        tcp_port: u16,
        secret: Vec<u8>,
        peers: Vec<(ContextualNetAddress, PeerDirection)>,
    ) -> Self {
        let directory = Arc::new(PeerDirectory::new(peers.iter().cloned().map(|(addr, direction)| (addr.into(), direction)).collect()));
        let authenticator = Arc::new(TokenAuthenticator::new(secret));
        let is_udp_active = Arc::new(AtomicBool::new(false));
        let receive_block_waker = Arc::new(tokio::sync::Notify::new());
        let tcp_runtime =
            ControlRuntime::new( listen_address, directory.clone(), authenticator.clone(), is_udp_active.clone()).await;
        Self { listen_address, tcp_runtime: Arc::new(TokioMutex::new(tcp_runtime)), udp_runtime: None, authenticator, directory, params, fragmentation_config, udp_port, tcp_port, udp_active: is_udp_active, receive_block_waker }
    }

    pub async fn start_control_runtime(&mut self) {
        let tcp_runtime = self.tcp_runtime.clone();
        tokio::spawn(async move {
            let mut rt = tcp_runtime.lock().await;
            rt.run().await;
        });
    }

    /// stop the UDP relay without consuming the struct so the caller can still
    /// use the relay instance afterwards.
    pub async fn stop_fast_relay(&mut self) {
        if !self.is_udp_active() {
            warn!("trying to stop fast trusted relay although it is not active");
            return;
        } else {
            self.udp_active.store(false, std::sync::atomic::Ordering::SeqCst);
            // drops the runtime, which frees the resources
            self.udp_runtime.take();
            // signal to peers that the relay is not ready to receive blocks.
            self.tcp_runtime.lock().await.signal_not_ready().await;
        };
    }

    /// start or restart the UDP relay; takes `&mut self` to avoid moving the
    /// entire relay instance out of the caller.
    pub async fn start_fast_relay(&mut self) {
        if self.is_udp_active() {
            warn!("trying to start fast trusted relay although it is already active");
            return;
        }
        self.udp_active.store(true, std::sync::atomic::Ordering::SeqCst);
        let mut udp_runtime = TransportRuntime::new(
            self.params.clone(),
            self.listen_address.into(),
            self.fragmentation_config.clone(),
            self.directory.clone(),
            self.authenticator.clone(),
            self.udp_active.clone(),
        );
        udp_runtime.start();
        self.udp_runtime = Some(Arc::new(udp_runtime));
        // signal to peers that the relay is ready to receive blocks.
        self.tcp_runtime.lock().await.signal_ready().await;
        self.receive_block_waker.notify_one();
        info!("fast trusted relay UDP transport started");
    }

    /// shut down both runtimes; does not consume the relay in order to allow
    /// callers (including `Drop`) to borrow it.
    pub fn shutdown(&mut self) {
        self.stop_fast_relay();
        // only move the control runtime into the spawned task, keeping the
        // rest of `self` live.
        let tcp_runtime = self.tcp_runtime.clone();
        tokio::spawn(async move {
            let mut rt = tcp_runtime.lock().await;
            rt.stop().await;
        });
    }

    pub async fn broadcast_block(&self, hash: Hash, block: FtrBlock) -> Result<(), String> {
        if let Some(udp_runtime) = &self.udp_runtime {
            udp_runtime.submit_block_for_broadcast(hash, block)
        } else {
            // Relay is inactive; ignore the broadcast but return Ok to avoid
            // treating this as an error.
            Ok(())
        }
    }

    pub async fn recv_block(&self) -> (Hash, FtrBlock) {
        loop {
            if let Some(udp_runtime) = &self.udp_runtime {
                let block_receiver_arc = udp_runtime.block_receive();
                let mut block_receiver = block_receiver_arc.lock().await;
                if let Some(msg) = block_receiver.recv().await {
                    return msg.into_parts();
                }
            }
            // wait until the udp runtime becomes active again.
            self.receive_block_waker.notified().await;
        }
    }

    pub fn is_udp_active(&self) -> bool {
        self.udp_runtime.as_ref().is_some_and(|udp_runtime| udp_runtime.is_active())
    }
}

impl Drop for FastTrustedRelay {
    fn drop(&mut self) {
        // Ensure all tasks are stopped when the relay is dropped.
        // `shutdown` now borrows `&mut self` and spawns the necessary async
        // work internally.
        self.shutdown();
    }
}
