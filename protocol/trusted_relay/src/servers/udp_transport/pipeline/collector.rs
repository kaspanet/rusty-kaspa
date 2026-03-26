use bytes::Bytes;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use kaspa_core::{debug, info, trace, warn};

use crate::model::fragments::FragmentHeader;
use crate::params::{FragmentationConfig, TransportParams};
use crate::servers::auth::AuthToken;
use crate::servers::udp_transport::pipeline::verification::{
    PacketReceivedMessage, VerificationMessage, VerificationReceiver, VerificationSender,
};

const COLLECTOR_WORKER_NAME: &str = "collector-worker";

fn run(
    collector_idx: usize,
    socket: Arc<std::net::UdpSocket>,
    verification_receivers: Vec<VerificationReceiver>,
    verification_senders: Vec<VerificationSender>,
    buf_size: usize,
    config: FragmentationConfig,
    shutting_down: Arc<AtomicBool>,
) {
    info!("{}-{} started, listening on UDP socket", COLLECTOR_WORKER_NAME, collector_idx);
    let mut buf = vec![0u8; buf_size];
    let min_expected_fragment_size = AuthToken::TOKEN_SIZE + FragmentHeader::SIZE + config.payload_size;
    let fragment_index_offset = AuthToken::TOKEN_SIZE + FragmentHeader::FRAGMENT_INDEX_OFFSET;
    let mut first_packet_logged = false;
    while !shutting_down.load(Ordering::Relaxed) {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                // Log first packet to confirm UDP reception is working
                if !first_packet_logged {
                    debug!(
                        "Collector {}: received FIRST UDP packet ({} bytes) from {} - relay is receiving data",
                        collector_idx, len, src
                    );
                    first_packet_logged = true;
                }
                // We must ensure we can read the fragment index bytes before routing.
                // This is the only check we do in the collector.
                if len < min_expected_fragment_size {
                    warn!("Collector {}: received undersized UDP datagram ({} bytes) from {}, dropping", collector_idx, len, src);
                    continue;
                }
                // Safety: sock_ref.recv_from guarantees `buf[..len]` is initialized.
                let worker_idx = u16::from_le_bytes([buf[fragment_index_offset], buf[fragment_index_offset + 1]]) as usize
                    % verification_senders.len();

                trace!(
                    "{}[{}]: recv {} bytes from {}, routing to worker {}",
                    COLLECTOR_WORKER_NAME, collector_idx, len, src, worker_idx
                );
                let packet = Bytes::copy_from_slice(&buf[..len]);
                let msg = VerificationMessage::PacketReceived(PacketReceivedMessage::new(packet, src));
                if let Err(e) = verification_senders[worker_idx].try_send(msg) {
                    match e {
                        crossbeam_channel::TrySendError::Full(_) => {
                            warn!("Collector {}: worker {} channel full, dropping packet from {}", collector_idx, worker_idx, src);
                            verification_receivers[worker_idx].try_iter();
                        }
                        crossbeam_channel::TrySendError::Disconnected(_) => {
                            info!("Collector {}: worker {} disconnected, shutting down", collector_idx, worker_idx);
                            return;
                        }
                    }
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut {
                    // Normal timeout - the while condition will check shutdown flag on next iteration
                    continue;
                }
                // Other socket errors - exit the loop
                info!("Collector {}: socket error: {}, shutting down", collector_idx, e);
                break;
            }
        }
    }
    info!("{}-{} exited (shutdown signaled)", COLLECTOR_WORKER_NAME, collector_idx);
}

pub fn spawn_collector_thread(
    collector_idx: usize,
    socket: Arc<std::net::UdpSocket>,
    verification_senders: Vec<VerificationSender>,
    verification_receivers: Vec<VerificationReceiver>,
    transport_params: TransportParams,
    config: FragmentationConfig,
    shutting_down: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    let handle = std::thread::Builder::new()
        .name(format!("{}-{}", COLLECTOR_WORKER_NAME, collector_idx))
        .spawn(move || {
            run(
                collector_idx,
                socket,
                verification_receivers,
                verification_senders,
                transport_params.default_buffer_size,
                config,
                shutting_down.clone(),
            );
        })
        .expect(&format!("Failed to spawn {}-{} thread", COLLECTOR_WORKER_NAME, collector_idx));
    handle
}
