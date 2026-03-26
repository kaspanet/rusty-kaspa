use std::net::SocketAddr;
use std::sync::Arc;

use bytes::Bytes;
use kaspa_core::trace;
use log::{info, warn};

use crate::servers::peer_directory::{PeerDirectory, PeerInfoList};

/// This is a worker that simply relays fragments to all ready outbound peers.
/// It receives fragments from the verifier, which takes care of validation and is only responsible for routing.

const WORKER_NAME: &str = "relay-worker";

pub type RelaySender = crossbeam_channel::Sender<RelayMessage>;
pub type RelayReceiver = crossbeam_channel::Receiver<RelayMessage>;

pub struct RelayMessage(Bytes, SocketAddr);

impl RelayMessage {
    #[inline(always)]
    pub fn new(raw_packet: Bytes, src: SocketAddr) -> Self {
        Self(raw_packet, src)
    }

    #[inline(always)]
    pub fn raw_packet(self) -> Bytes {
        self.0
    }

    #[inline(always)]
    pub fn src(self) -> SocketAddr {
        self.1
    }
}

/// Run the forwarding loop. Blocks until all senders are dropped.
///
/// Uses a drain-batch pattern: after the initial blocking `recv()`,
/// we `try_recv()` to drain all queued jobs, take a single directory
/// snapshot for the whole batch, and fan-out all packets using that
/// snapshot.  This amortises the `ArcSwap::load_full()` cost over
/// many packets during bursts.
fn run(worker_idx: usize, receiver: RelayReceiver, peer_info_list: PeerInfoList) {
    info!("{}-{} started", WORKER_NAME, worker_idx);

    while let Ok(RelayMessage { 0: raw_packet, 1: src }) = receiver.recv() {
        trace!("{}-{}: received packet (len={}) from {}", WORKER_NAME, worker_idx, raw_packet.len(), src);

        // One snapshot for the entire batch.
        let peers = peer_info_list.load_full();

        for peer in peers.iter() {
            if !peer.is_outbound_ready() || peer.udp_target() == src {
                trace!("{}-{}: skipping peer {} for packet from {}", WORKER_NAME, worker_idx, peer.address(), src);
                continue;
            }

            if let Some(socket) = peer.send_socket() {
                if let Err(e) = socket.send(&raw_packet) {
                    warn!("{}-{}: failed to send to {}: {}", WORKER_NAME, worker_idx, peer.udp_target(), e);
                }
            } else {
                warn!("{}-{}: no send socket for peer {}", WORKER_NAME, worker_idx, peer.udp_target());
            }
        }
    }
    info!("{}-{} exited", WORKER_NAME, worker_idx);
}

pub fn spawn_relay_thread(
    worker_idx: usize,
    receiver: RelayReceiver,
    peer_directory: Arc<PeerDirectory>,
) -> std::thread::JoinHandle<()> {
    let handle = std::thread::Builder::new()
        .name(format!("{}-{}", WORKER_NAME, worker_idx))
        .spawn(move || run(worker_idx, receiver, peer_directory.peer_info_list()))
        .expect(&format!("Failed to spawn {}-{} thread", WORKER_NAME, worker_idx));
    handle
}
