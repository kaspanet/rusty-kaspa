//! Minimal TransportRuntime scaffold: owned runtime + handle API
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tokio::sync::Mutex as TokioMutex;

use crossbeam_channel::bounded;
use fixedbitset::FixedBitSet;
use kaspa_consensus_core::{BlockHashMap, BlockHasher, Hash, HashMapCustomHasher};
use kaspa_core::{info, warn};
use ringmap::{RingMap, RingSet};
use tokio::sync::mpsc::unbounded_channel as tokio_unbounded_channel;

use crate::codec::buffers::BlockDecodeState;
use crate::fast_trusted_relay::DEFAULT_UDP_PORT;
use crate::model::ftr_block::FtrBlock;
use crate::params::{FragmentationConfig, TransportParams};
use crate::servers::auth::TokenAuthenticator;
use crate::servers::peer_directory::PeerDirectory;
use crate::servers::udp_transport::pipeline::broadcast::{self, BroadcastMessage, BroadcastReceiver, BroadcastSender};
use crate::servers::udp_transport::pipeline::reassembly::decoding::{self, DecodeJobMessage, DecodeResultMessage};
use crate::servers::udp_transport::pipeline::reassembly::reassembly::{
    BlockReassemblerBlockMessage, ReassemblerBlockReceiver, ReassemblerBlockSender, ReassemblerFragmentMessage,
};
use crate::servers::udp_transport::pipeline::relay::RelayMessage;
use crate::servers::udp_transport::pipeline::verification::VerificationMessage;
use crate::servers::udp_transport::pipeline::{collector, reassembly, verification};

struct TransportRuntimeHandles {
    broadcast_handles: Vec<JoinHandle<()>>,
    verifier_handles: Vec<JoinHandle<()>>,
    coordinator_handles: Vec<JoinHandle<()>>,
    decoder_handles: Vec<JoinHandle<()>>,
    collector_handles: Vec<JoinHandle<()>>,
    forwarder_handles: Vec<JoinHandle<()>>,
}

impl TransportRuntimeHandles {
    /// Signal all threads to shutdown without waiting.
    /// Threads will exit when they see shutdown signal or their channels close.
    fn signal_shutdown(&self) {
        // Handles will be joined by shutdown_blocking() or dropped naturally
        info!("UdpTransportRuntime: signaling worker threads to shutdown");
    }

    /// Blocking shutdown - waits for all threads to complete.
    /// Should only be called from a blocking context (e.g., spawn_blocking).
    fn shutdown_blocking(&mut self) {
        for h in self.collector_handles.drain(..) {
            info!("Stopping collector thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        for h in self.broadcast_handles.drain(..) {
            info!("Stopping broadcast thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        for h in self.verifier_handles.drain(..) {
            info!("Stopping verifier thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        for h in self.forwarder_handles.drain(..) {
            info!("Stopping forwarder thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        for h in self.coordinator_handles.drain(..) {
            info!("Stopping coordinator thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        for h in self.decoder_handles.drain(..) {
            info!("Stopping decoder thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        }
        info!("UdpTransportRuntime: all worker threads stopped");
    }
}
struct TransportRuntimeInner {
    handles: Arc<Mutex<TransportRuntimeHandles>>,
    bound_addr: SocketAddr,
    /// Internal shutdown signal for collector threads. This is NOT exposed through the API.
    /// Set to true when shutdown() is called to signal collectors to exit.
    collector_shutdown: Arc<AtomicBool>,
}

impl TransportRuntimeInner {
    /// Signal shutdown without blocking. Safe to call from async context.
    fn signal_shutdown(&self) {
        self.collector_shutdown.store(true, Ordering::SeqCst);
        self.handles.lock().unwrap().signal_shutdown();
    }

    /// Blocking shutdown - signals and waits for all threads.
    /// Should only be called from a blocking context.
    fn shutdown_blocking(self) {
        self.collector_shutdown.store(true, Ordering::SeqCst);
        let mut handles = self.handles.lock().unwrap();
        handles.shutdown_blocking();
    }

    fn start(
        params: TransportParams,
        config: FragmentationConfig,
        directory: Arc<PeerDirectory>,
        authenticator: Arc<TokenAuthenticator>,
        broadcast_receiver: BroadcastReceiver,
        block_emit_sender: Arc<ReassemblerBlockSender>,
    ) -> Self {
        // Internal shutdown signal for collectors only
        let collector_shutdown = Arc::new(AtomicBool::new(false));

        let handles = Arc::new(Mutex::new(TransportRuntimeHandles {
            broadcast_handles: Vec::with_capacity(params.num_of_broadcasters),
            verifier_handles: Vec::with_capacity(params.num_of_verifiers),
            decoder_handles: Vec::with_capacity(params.num_of_decoders_per_coordinators),
            coordinator_handles: Vec::with_capacity(params.num_of_coordinators),
            collector_handles: Vec::with_capacity(params.num_of_collectors),
            forwarder_handles: Vec::with_capacity(params.num_of_forwarders),
        }));

        // Create a UDP datagram socket using socket2 for advanced options
        use socket2::{Domain, Protocol, Socket, Type};
        let udp_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).expect("Failed to create UDP socket");
        // Set large socket buffers to avoid packet loss under burst traffic
        // Uses .ok() since OS may cap to system limits (e.g. /proc/sys/net/core/rmem_max)
        udp_socket.set_recv_buffer_size(32 * 1024 * 1024).ok();
        udp_socket.set_send_buffer_size(32 * 1024 * 1024).ok();
        udp_socket.set_nonblocking(false).unwrap();
        udp_socket.set_reuse_address(true).unwrap();
        udp_socket.set_reuse_port(true).unwrap();
        udp_socket.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
        udp_socket.bind(&SocketAddr::new("0.0.0.0".parse().unwrap(), DEFAULT_UDP_PORT).into()).expect("Failed to bind UDP socket");
        let socket = UdpSocket::from(udp_socket);
        let bound_addr = socket.local_addr().expect("Failed to get local address from UDP socket");

        //2)  Pre-generate channels

        let socket = Arc::new(socket);
        //2.1) verification channels: Collector -> Verifier
        let mut verification_sender_channels = Vec::with_capacity(params.num_of_verifiers);
        let mut verification_receiver_channels = Vec::with_capacity(params.num_of_verifiers);
        for _ in 0..params.num_of_verifiers {
            let (tx, rx) = bounded::<VerificationMessage>(params.verification_channel_capacity());
            verification_sender_channels.push(tx);
            verification_receiver_channels.push(rx);
        }

        //2.2) coordinator channels: Verifier -> Coordinator
        let mut reassembly_sender_channels = Vec::with_capacity(params.num_of_coordinators);
        let mut reassembly_receiver_channels = Vec::with_capacity(params.num_of_coordinators);
        for _ in 0..params.num_of_coordinators {
            let (tx, rx) = bounded::<ReassemblerFragmentMessage>(params.coordinator_receive_channel_capacity());
            reassembly_sender_channels.push(tx);
            reassembly_receiver_channels.push(rx);
        }

        //2.3) Forwarder channels: Verifier -> Forwarder
        let (forwarder_sender, forwarder_receiver) = bounded::<RelayMessage>(params.forwarder_channel_capacity());

        // 2.4) DecodeWorker channels: Coordinator -> DecodeWorker
        let mut decode_job_sender_channels = Vec::with_capacity(params.num_of_coordinators);
        let mut decode_job_receiver_channels = Vec::with_capacity(params.num_of_coordinators);
        for _ in 0..(params.num_of_decoders_per_coordinators) {
            let (tx, rx) = bounded::<DecodeJobMessage>(params.decoder_channel_capacity());
            decode_job_sender_channels.push(tx);
            decode_job_receiver_channels.push(rx);
        }

        // 2.5) DecodeWorker result channels: DecodeWorker -> Coordinator
        let mut decode_result_sender_channels = Vec::with_capacity(params.num_of_coordinators);
        let mut decode_result_receiver_channels = Vec::with_capacity(params.num_of_coordinators);
        for _ in 0..(params.num_of_coordinators) {
            let (tx, rx) = bounded::<DecodeResultMessage>(params.decoder_channel_capacity());
            decode_result_sender_channels.push(tx);
            decode_result_receiver_channels.push(rx);
        }

        // 3) Generate caches
        let mut processed_block_cache = vec![
            RingSet::<Hash, BlockHasher>::with_capacity_and_hasher(
                params.coordinator_block_cache_capacity(),
                BlockHasher::default()
            );
            params.num_of_coordinators
        ];
        let mut partial_blocks =
            vec![BlockHashMap::<BlockDecodeState>::with_capacity(params.max_concurrent_blocks()); params.num_of_coordinators];
        let mut recent_shards_cache =
            vec![RingMap::<Hash, FixedBitSet>::with_capacity(params.block_cache_capacity()); params.num_of_verifiers];

        // spawn collectors
        for i in 0..params.num_of_coordinators {
            handles.lock().unwrap().collector_handles.push(collector::spawn_collector_thread(
                i,
                Arc::clone(&socket),
                verification_sender_channels.clone(),
                verification_receiver_channels.clone(),
                params.clone(),
                config.clone(),
                collector_shutdown.clone(),
            ));
        }

        // spawn verifiers
        for i in 0..params.num_of_verifiers {
            handles.lock().unwrap().verifier_handles.push(verification::spawn_verifier_thread(
                i,
                params.num_of_verifiers,
                directory.clone(),
                authenticator.clone(),
                verification_receiver_channels[i].clone(),
                reassembly_sender_channels.clone(),
                reassembly_receiver_channels.clone(),
                forwarder_sender.clone(),
                forwarder_receiver.clone(),
                config.clone(),
                params.clone(),
                recent_shards_cache.pop().unwrap(),
            ));
        }

        // spawn coordinators
        for i in 0..params.num_of_coordinators {
            for j in 0..params.num_of_decoders_per_coordinators {
                handles.lock().unwrap().decoder_handles.push(decoding::spawn_decode_worker(
                    i,
                    j,
                    config,
                    decode_job_receiver_channels[i].clone(),
                    decode_result_sender_channels[i].clone(),
                ));
            }

            handles.lock().unwrap().coordinator_handles.push(reassembly::reassembly::spawn_reassembler_thread(
                i,
                reassembly_receiver_channels[i].clone(),
                decode_job_sender_channels[i].clone(),
                decode_result_receiver_channels[i].clone(),
                block_emit_sender.clone(),
                processed_block_cache.pop().unwrap(),
                partial_blocks.pop().unwrap(),
                params.max_concurrent_blocks(),
                config.clone(),
            ));
        }

        // spawn broadcasters
        for i in 0..params.num_of_broadcasters {
            handles.lock().unwrap().broadcast_handles.push(broadcast::spawn_broadcaster_thread(
                i,
                Arc::clone(&socket),
                directory.clone(),
                broadcast_receiver.clone(),
                authenticator.clone(),
                config.clone(),
                verification_sender_channels.clone(),
            ));
        }

        TransportRuntimeInner { handles, bound_addr, collector_shutdown }
    }
}

/// Owned runtime that holds transport resources. Dropping this will shut down
/// the runtime; prefer calling `shutdown` for deterministic join.
pub struct TransportRuntime {
    params: TransportParams,
    config: FragmentationConfig,
    directory: Arc<PeerDirectory>,
    authenticator: Arc<TokenAuthenticator>,
    listen_addr: SocketAddr,
    block_emit_receiver: Arc<TokioMutex<ReassemblerBlockReceiver>>,
    block_emit_sender: Arc<ReassemblerBlockSender>,
    broadcast_sender: Option<BroadcastSender>,
    broadcast_receiver: BroadcastReceiver,
    inner: Option<TransportRuntimeInner>,
}

impl TransportRuntime {
    /// Create a new runtime owning the provided `PeerDirectory`.
    pub fn new(
        params: TransportParams,
        listen_addr: SocketAddr,
        config: FragmentationConfig,
        directory: Arc<PeerDirectory>,
        authenticator: Arc<TokenAuthenticator>,
    ) -> Self {
        let (block_emit_sender, block_emit_receiver) = tokio_unbounded_channel::<BlockReassemblerBlockMessage>();
        let (broadcast_sender, broadcast_receiver) = bounded::<BroadcastMessage>(params.broadcast_channel_capacity());
        Self {
            params,
            listen_addr,
            config,
            directory,
            authenticator: authenticator.clone(),
            block_emit_sender: Arc::new(block_emit_sender),
            broadcast_receiver,
            inner: None,
            block_emit_receiver: Arc::new(TokioMutex::new(block_emit_receiver)),
            broadcast_sender: Some(broadcast_sender),
        }
    }

    pub fn start(&mut self) -> bool {
        if self.inner.is_some() {
            warn!("TransportRuntime is already started, skipping start");
            return false;
        }
        self.inner = Some(TransportRuntimeInner::start(
            self.params.clone(),
            self.config.clone(),
            self.directory.clone(),
            self.authenticator.clone(),
            self.broadcast_receiver.clone(),
            self.block_emit_sender.clone(),
        ));
        true
    }

    pub fn submit_block_for_broadcast(&self, hash: Hash, block: Arc<FtrBlock>) -> Result<(), String> {
        if let Some(broadcast_sender) = &self.broadcast_sender {
            broadcast_sender.send(BroadcastMessage::new(hash, block)).map_err(|e| format!("Failed to send broadcast message: {}", e))
        } else {
            Err("TransportRuntime is not started".to_string())
        }
    }

    /// Receive the next decoded block from the runtime.
    ///
    /// This channel is a tokio channel, as to integrate with the wider tokio runtime.
    /// as such this is the only async method on the runtime.
    #[inline(always)]
    pub fn block_receive(&self) -> Arc<TokioMutex<ReassemblerBlockReceiver>> {
        self.block_emit_receiver.clone()
    }

    /// Returns the actual bound UDP address of the runtime.
    /// Only available after `start()` has been called.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.inner.as_ref().map(|inner| inner.bound_addr)
    }

    /// Returns true if the runtime is started and hasn't been shut down.
    pub fn is_active(&self) -> bool {
        self.inner.as_ref().map_or(false, |inner| !inner.collector_shutdown.load(Ordering::SeqCst))
    }

    /// Async-safe shutdown that uses spawn_blocking to wait for threads.
    /// This should be called during graceful shutdown to properly await thread completion.
    pub async fn shutdown_async(&mut self) {
        if let Some(inner) = self.inner.take() {
            self.broadcast_sender.take(); // Drop sender to unblock broadcaster
            // Use spawn_blocking to avoid blocking the tokio runtime
            let _ = tokio::task::spawn_blocking(move || {
                inner.shutdown_blocking();
            })
            .await;
        }
    }
}

impl Drop for TransportRuntime {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            self.broadcast_sender.take(); // Drop sender to unblock broadcaster
            // Only signal shutdown - don't block waiting for threads.
            // Threads will exit on their own when they see the shutdown signal
            // or when their channels are closed.
            inner.signal_shutdown();
            // Note: Thread handles are dropped here without joining.
            // For graceful shutdown, call shutdown_async() instead.
        }
    }
}
