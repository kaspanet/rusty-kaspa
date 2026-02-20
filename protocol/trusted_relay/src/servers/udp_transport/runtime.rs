//! Minimal TransportRuntime scaffold: owned runtime + handle API
use std::net::{SocketAddr, UdpSocket};
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use crossbeam_channel::{bounded};
use fixedbitset::FixedBitSet;
use kaspa_consensus_core::{BlockHashMap, BlockHasher, Hash, HashMapCustomHasher};
use kaspa_core::{info, warn};
use ringmap::{RingMap, RingSet};
use tokio::sync::mpsc::{unbounded_channel as tokio_unbounded_channel};

use crate::codec::buffers::BlockDecodeState;
use crate::model::ftr_block::FtrBlock;
use crate::params::{FragmentationConfig, TransportParams};
use crate::servers::auth::TokenAuthenticator;
use crate::servers::peer_directory::PeerDirectory;
use crate::servers::udp_transport::pipeline::broadcast::{self, BroadcastMessage, BroadcastReceiver, BroadcastSender};
use crate::servers::udp_transport::pipeline::{collector, reassembly, verification};
use crate::servers::udp_transport::pipeline::reassembly::decoding::{self, DecodeJobMessage, DecodeResultMessage};
use crate::servers::udp_transport::pipeline::reassembly::reassembly::{BlockReassemblerBlockMessage, ReassemblerBlockReceiver, ReassemblerBlockSender, ReassemblerFragmentMessage};
use crate::servers::udp_transport::pipeline::relay::RelayMessage;
use crate::servers::udp_transport::pipeline::verification::VerificationMessage;

struct TransportRuntimeHandles {
    broadcast_handles: Vec<JoinHandle<()>>,
    verifier_handles: Vec<JoinHandle<()>>,
    coordinator_handles: Vec<JoinHandle<()>>,
    decoder_handles: Vec<JoinHandle<()>>,
    collector_handles: Vec<JoinHandle<()>>,
    forwarder_handles: Vec<JoinHandle<()>>,
}

impl TransportRuntimeHandles {
    fn shutdown(&mut self) {
        for h in self.broadcast_handles.drain(..) {
            info!("Stopping broadcast thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        for h in self.verifier_handles.drain(..) {
            info!("Stopping verifier thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        for h in self.coordinator_handles.drain(..) {
            info!("Stopping coordinator thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        for h in self.collector_handles.drain(..) {
            info!("Stopping collector thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        for h in self.decoder_handles.drain(..) {
            info!("Stopping decoder thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        for h in self.forwarder_handles.drain(..) {
            info!("Stopping forwarder thread: {}", h.thread().name().unwrap_or("unknown"));
            let _ = h.join();
        };
        info!("UdpTransportRuntime: all worker threads stopped");
    }
}

impl Drop for TransportRuntimeHandles {
    fn drop(&mut self) {
        self.shutdown();
    }
}
struct TransportRuntimeInner {
    handles: Arc<Mutex<TransportRuntimeHandles>>,
    shut_down: Arc<AtomicBool>,
}

impl TransportRuntimeInner {
    /// Spawn a broadcast worker and record its handle.
    fn shutdown(&self) {
        let mut handles = self.handles.lock().unwrap();
        self.shut_down.store(true, std::sync::atomic::Ordering::SeqCst);
        handles.shutdown();
    }

    fn start(
        listen_address: SocketAddr,
        params: TransportParams,
        config: FragmentationConfig,
        directory: Arc<PeerDirectory>,
        authenticator: Arc<TokenAuthenticator>,
        shutdown: Arc<AtomicBool>,
        broadcast_receiver: BroadcastReceiver,
        block_emit_sender: Arc<ReassemblerBlockSender>,
    ) -> Self {
        let handles = Arc::new(Mutex::new(TransportRuntimeHandles {
            broadcast_handles: Vec::with_capacity(params.num_of_broadcasters),
            verifier_handles: Vec::with_capacity(params.num_of_verifiers),
            decoder_handles: Vec::with_capacity(params.num_of_decoders_per_coordinators),
            coordinator_handles: Vec::with_capacity(params.num_of_coordinators),
            collector_handles: Vec::with_capacity(params.num_of_collectors),
            forwarder_handles: Vec::with_capacity(params.num_of_forwarders),
        }));

        //1) create and bind udp socket.
        let socket = Arc::new(UdpSocket::bind(listen_address).expect("Failed to bind UDP socket"));
        socket.set_nonblocking(false).expect("Failed to set UDP socket to non-blocking mode");

        //2)  Pre-generate channels

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
        let mut partial_blocks = vec![BlockHashMap::<BlockDecodeState>::with_capacity(params.max_concurrent_blocks()); params.num_of_coordinators];
        let mut recent_shards_cache =
            vec![RingMap::<Hash, FixedBitSet>::with_capacity(params.block_cache_capacity()); params.num_of_verifiers];

        // spawn collectors
        for i in 0..params.num_of_coordinators {
            handles.lock().unwrap().collector_handles.push(collector::spawn_collector_thread(
                i,
                socket.clone(),
                verification_sender_channels.clone(),
                verification_receiver_channels.clone(),
                params.clone(),
                config.clone(),
                shutdown.clone(),
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
                socket.clone(),
                directory.clone(),
                broadcast_receiver.clone(),
                authenticator.clone(),
                config.clone(),
                verification_sender_channels.clone(),
            ));
        }

        TransportRuntimeInner { handles, shut_down: shutdown }
    }
}

/// Owned runtime that holds transport resources. Dropping this will drop
/// the held resources; prefer calling `shutdown` for deterministic join.
pub struct TransportRuntime {
    params: TransportParams,
    config: FragmentationConfig,
    directory: Arc<PeerDirectory>,
    authenticator: Arc<TokenAuthenticator>,
    shutdown: Arc<AtomicBool>,
    listen_addr: SocketAddr,
    block_emit_receiver: ReassemblerBlockReceiver,
    block_emit_sender: Arc<ReassemblerBlockSender>,
    broadcast_sender: BroadcastSender,
    broadcast_receiver: BroadcastReceiver,
    inner: Option<Arc<TransportRuntimeInner>>,
}

impl TransportRuntime {
    /// Create a new runtime owning the provided `PeerDirectory`.
    pub fn new(
        params: TransportParams,
        listen_addr: SocketAddr,
        config: FragmentationConfig,
        directory: Arc<PeerDirectory>,
        authenticator: Arc<TokenAuthenticator>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        let (block_emit_sender, block_emit_receiver) = tokio_unbounded_channel::<BlockReassemblerBlockMessage>();
        let (broadcast_sender, broadcast_receiver) = bounded::<BroadcastMessage>(params.broadcast_channel_capacity());
        Self { params, listen_addr, config, directory, authenticator: authenticator.clone(), shutdown: shutdown.clone(), block_emit_sender: Arc::new(block_emit_sender),  broadcast_receiver, inner: None, block_emit_receiver, broadcast_sender }
    }

    pub fn start(&mut self) {
        if self.inner.is_some() {
            warn!("TransportRuntime is already started, skipping start");
            return;
        }
        self.inner = Some(Arc::new(TransportRuntimeInner::start(

            self.listen_addr,
            self.params.clone(),
            self.config.clone(),
            self.directory.clone(),
            self.authenticator.clone(),
            self.shutdown.clone(),
            self.broadcast_receiver.clone(),
            self.block_emit_sender.clone(),
        )));
    }
    /// Shutdown the runtime: close channels and join worker threads.
    /// This is a best-effort operation; it will attempt to join any spawned
    /// workers recorded in the runtime.
    pub fn shutdown(mut self) {
        if let Some(inner) = self.inner.clone() {
            inner.shutdown();
        }
        self.inner = None;
    }

    pub fn submit_block_for_broadcast(&self, hash: Hash, block: FtrBlock) -> Result<(), String> {
        if self.inner.is_some() {
            self.broadcast_sender.send(BroadcastMessage::new(hash, block)).map_err(|e| format!("Failed to send broadcast message: {}", e))
        } else {
            Err("TransportRuntime is not started".to_string())
        }
    }

    /// Receive the next decoded block from the runtime.
    ///
    /// This channel is a tokio channel, as to integrate with the wider tokio runtime.
    /// as such this is the only async method on the runtime.
pub async fn block_receive(mut self) -> (Hash, FtrBlock) {
        let res = self.block_emit_receiver.recv().await.expect("Failed to receive block from coordinator");
        res.into_parts()
    }
}

impl Drop for TransportRuntime {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            if Arc::strong_count(inner) == 1 {
                inner.shutdown();
            }
        }
    }
}
