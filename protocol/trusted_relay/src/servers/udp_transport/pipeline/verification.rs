use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bytes::Bytes;
use crossbeam_channel::Receiver as CrossbeamReceiver;
use crossbeam_channel::Sender as CrossbeamSender;
use fixedbitset::FixedBitSet;
use kaspa_core::{info, trace, warn};
use kaspa_hashes::Hash;
use ringmap::RingMap;

use std::net::SocketAddr;

use crate::model::fragments::{Fragment, FragmentHeader};
use crate::params::{FragmentationConfig, TransportParams};
use crate::servers::auth::{AuthToken, TokenAuthenticator};
use crate::servers::peer_directory::{Allowlist, PeerDirectory};
use crate::servers::udp_transport::pipeline::reassembly::reassembly::{ReassemblerBlockReceiver, ReassemblerFragmentMessage, ReassemblerFragmentReceiver, ReassemblerFragmentSender};
use crate::servers::udp_transport::pipeline::relay::{self, RelayMessage, RelayReceiver, RelaySender};

const WORKER_NAME: &str = "verification-worker";

pub type VerificationSender = CrossbeamSender<VerificationMessage>;
pub type VerificationReceiver = CrossbeamReceiver<VerificationMessage>;

pub struct PacketReceivedMessage(Bytes, SocketAddr);

impl PacketReceivedMessage {
    #[inline(always)]
    pub fn new(raw_packet: Bytes, src: SocketAddr) -> Self {
        Self(raw_packet, src)
    }

    #[inline(always)]
    pub fn raw_packet(mut self) -> Bytes {
        self.0
    }

    #[inline(always)]
    pub fn src(mut self) -> SocketAddr {
        self.1
    }
}

pub struct MarkBlockBroadcastedMessage(Hash, u16);

impl MarkBlockBroadcastedMessage {
    #[inline(always)]
    pub fn new(hash: Hash, total_fragments: u16) -> Self {
        Self(hash, total_fragments)
    }

    #[inline(always)]
    pub fn hash(mut self) -> Hash {
        self.0
    }

    #[inline(always)]
    pub fn total_fragments(mut self) -> u16 {
        self.1
    }
}

/// A raw datagram received by the collector, not yet authenticated.
/// Messages sent to the verifier worker.
pub enum VerificationMessage {
    /// An incoming UDP packet to verify and deduplicate.
    PacketReceived(PacketReceivedMessage),
    /// Broadcast control: mark all fragments of a broadcasted block as seen.
    /// This updates the ringmap to set the full bitset to true,
    /// preventing future packets for this block from being duplicated.
    MarkBlockBroadcasted(MarkBlockBroadcastedMessage),
}

/// Verification worker loop — runs on a dedicated OS thread.
///
/// Owns a private `RingMap` for deduplication (no cross-worker synchronization).
/// Performs MAC verification, deduplication, and forwarding of authenticated fragments.
fn run(
    worker_idx: usize,
    num_of_workers: usize,

    mut recent_fragments: RingMap<Hash, FixedBitSet>,
    authenticator: Arc<TokenAuthenticator>,
    // channels
    receiver: CrossbeamReceiver<VerificationMessage>,
    reassembly_senders: Vec<ReassemblerFragmentSender>,
    relay_sender: RelaySender,

    // we hold receivers so we can drain them when the channel is full.
    // this is to prioritize making space for newer fragment / forwarding jobs.
    // the receivers on the other side are blocking, so they cannot realize and act upon full channel capacity.
    reassembly_receivers: Vec<ReassemblerFragmentReceiver>,
    relay_receiver: RelayReceiver, // for graceful shutdown when forwarder disconnects

    config: FragmentationConfig,
    allowlist: Allowlist,
) {
    info!("{}-{} started", WORKER_NAME, worker_idx);
    let mut count = 0u64;
    while let Ok(message) = receiver.recv() {
        match message {
            VerificationMessage::PacketReceived(PacketReceivedMessage(packet, src)) => {
                count += 1;
                trace!("{}-{}: {} received packet (len={}) from {}", WORKER_NAME, worker_idx, count, packet.len(), src);
                // Quickly drop malformed packets.
                if packet.len() != AuthToken::TOKEN_SIZE + FragmentHeader::SIZE + config.payload_size {
                    trace!("{}-{}: dropping malformed packet (len={}) from {}", WORKER_NAME, worker_idx, packet.len(), src);
                    continue;
                }

                if !is_from_allowlist(&src, &allowlist) {
                    // note: allowlist is a naive, best-effort, defense, ip headers in udp packets can be spoofed.
                    // but it does require an attacker to know the ips of trusted relay peers,
                    // or have a way to sniff the traffic. Actual authentication is via the hmac.
                    trace!("{}-{}: dropping packet from {} (not allowlisted)", WORKER_NAME, worker_idx, src);
                    continue;
                }

                let (fragment_index, last_fragment_index) = get_fragment_indices(&packet);

                if !is_fragment_index_valid(fragment_index, last_fragment_index) {
                    trace!(
                        " {}-{}: invalid fragment indices (idx={}, total={}) from {}",
                        WORKER_NAME, worker_idx, fragment_index, last_fragment_index, src
                    );
                    continue;
                }

                let fragment_hash = get_fragment_hash(&packet);
                let (compressed_fragment_index, compressed_capacity) =
                    compress_fragment_indices(fragment_index, last_fragment_index, num_of_workers);
                let (is_unique, hash_entry_present) =
                    is_unique_fragment(fragment_hash, compressed_fragment_index as u16, compressed_capacity as u16, &mut recent_fragments);

                // TODO: Consider making the checking order here configurable.
                // Note: currently we optimize for the expected case, i.e. that is multiple peers sending duplicate packets,
                // as such we filter via is_unique before hmac verification, this prioritizes deduplication (which is cheaper) before authentication (which is significantly more expensive).
                // That being said, under malicious conditions, i.e. ddos attack via packet floods, doing hmac before deduplication (and anything else for that matter) should be preferred,
                // as this reduces the per packet processing overhead in such conditions.
                if !is_unique {
                    trace!("{}-{}: duplicate fragment {}:{} from {}", WORKER_NAME, worker_idx, fragment_hash, fragment_index, src);
                    continue;
                } else if !is_authenticated(&packet, &authenticator) {
                    warn!(
                        "{}-{}: MAC verification failed for fragment {}:{} from {}",
                        WORKER_NAME, worker_idx, fragment_hash, fragment_index, src
                    );
                    continue;
                }

                update_recent_fragments(fragment_hash, hash_entry_present, compressed_fragment_index, compressed_capacity, &mut recent_fragments);

                // Forward to the Coordinator.
                let fragment = build_fragment(packet.clone(), fragment_hash, fragment_index, last_fragment_index);
                if let Err(e) = reassembly_senders[config.get_hash_bucket(fragment_hash, reassembly_senders.len())]
                    .try_send(ReassemblerFragmentMessage::new(fragment))
                {
                    match e {
                        crossbeam_channel::TrySendError::Full(_) => {
                            warn!(
                                "{}-{}: reassembly channel full, dropping {}:{} from {}, and draining reassembly channel {} to make room",
                                WORKER_NAME,
                                worker_idx,
                                fragment_hash,
                                fragment_index,
                                src,
                                config.get_hash_bucket(fragment_hash, reassembly_receivers.len())
                            );
                            // drain cannel to make space (best-effort)
                            reassembly_receivers[config.get_hash_bucket(fragment_hash, reassembly_receivers.len())].try_iter();
                        }
                        crossbeam_channel::TrySendError::Disconnected(_) => {
                            info!("{}-{}: fragment channel disconnected, shutting down", WORKER_NAME, worker_idx);
                            return;
                        }
                    }
                }

                // Forward verified fragment to fragmentForwarder (best-effort, non-blocking).
                if let Err(e) = relay_sender.try_send(RelayMessage::new(packet, src))
                {
                    match e {
                        crossbeam_channel::TrySendError::Full(_) => {
                            warn!(
                                "{}-{}: forwarder channel full, dropping {}:{} from {}, and draining forward channel to make room - consider increasing channel capacities",
                                WORKER_NAME,
                                worker_idx,
                                fragment_hash,
                                fragment_index,
                                src,
                            );
                            // drain channel to make space
                            relay_receiver.try_iter();
                        }
                        crossbeam_channel::TrySendError::Disconnected(_) => {
                            info!("{}-{}: forwarder channel disconnected, shutting down", WORKER_NAME, worker_idx);
                            return;
                        }
                    }
                }
            }
            VerificationMessage::MarkBlockBroadcasted(MarkBlockBroadcastedMessage(hash, total_fragments)) => {
                trace!("{}-{}: received MarkBlockBroadcasted for block {} with total_fragments={}", WORKER_NAME, worker_idx, hash, total_fragments);
                let count = total_fragments.saturating_add(1) as usize;
                let compressed_capacity = count.div_ceil(num_of_workers).max(1);

                if recent_fragments.len() == recent_fragments.capacity() {
                    let _ = recent_fragments.pop_front();
                }

                // Create a full bitset (all bits set) to mark every fragment as seen.
                let mut full_bitset = FixedBitSet::with_capacity(compressed_capacity);
                full_bitset.toggle_range(..);
                recent_fragments.push_back(hash, full_bitset);

                trace!(
                    "{}-{}: marked block {} as broadcasted (capacity={})",
                    WORKER_NAME, worker_idx, hash, compressed_capacity
                );
            }
        }
    }

    info!("{}-{}: UDP verification worker exited", WORKER_NAME, worker_idx);
}

pub fn spawn_verifier_thread(
    worker_idx: usize,
    num_of_workers: usize,
    directory: Arc<PeerDirectory>,
    authenticator: Arc<TokenAuthenticator>,
    receiver: VerificationReceiver,
    reassembly_senders: Vec<ReassemblerFragmentSender>,
    reassembly_receivers: Vec<ReassemblerFragmentReceiver>,
    forwarder_senders: RelaySender,
    forwarder_receivers: RelayReceiver,
    config: FragmentationConfig,
    transport: TransportParams,
    recent_fragments: RingMap<Hash, FixedBitSet>
) -> std::thread::JoinHandle<()> {

    let handle = std::thread::Builder::new()
        .name(format!("{}-{}", WORKER_NAME, worker_idx))
        .spawn(move || {
            run(
                worker_idx,
                num_of_workers,
                recent_fragments,
                authenticator,
                receiver,
                reassembly_senders,
                forwarder_senders,
                reassembly_receivers,
                forwarder_receivers,
                config,
                directory.allowlist(),
            );
        })
        .expect(&format!("Failed to spawn {}-{} thread", WORKER_NAME, worker_idx));
    handle
}

#[inline(always)]
fn get_fragment_indices(packet: &[u8]) -> (u16, u16) {
    // Caller must ensure `packet.len() >= AuthToken::TOKEN_SIZE + FragmentHeader::SIZE`.
    // Use direct indexed reads (faster and no panics because caller checks length).
    let base = AuthToken::TOKEN_SIZE;
    let fragment_index =
        u16::from_le_bytes([packet[base + FragmentHeader::FRAGMENT_INDEX_OFFSET], packet[base + FragmentHeader::FRAGMENT_INDEX_OFFSET + 1]]);
    let total_fragments =
        u16::from_le_bytes([packet[base + FragmentHeader::TOTAL_FRAGMENTS_OFFSET], packet[base + FragmentHeader::TOTAL_FRAGMENTS_OFFSET + 1]]);
    (fragment_index, total_fragments)
}

#[inline(always)]
fn get_fragment_hash(packet: &[u8]) -> Hash {
    Hash::from_slice(&packet[AuthToken::TOKEN_SIZE..AuthToken::TOKEN_SIZE + FragmentHeader::SIZE - 4]) // exclude fragment_index and total_fragments
}

#[inline(always)]
fn compress_fragment_indices(fragment_index: u16, last_fragment_index: u16, worker_count: usize) -> (usize, usize) {
    let compressed_capacity = (last_fragment_index as usize).div_ceil(worker_count).max(1);
    let compressed_idx = fragment_index as usize / worker_count;
    (compressed_idx, compressed_capacity)
}

#[inline(always)]
fn update_recent_fragments(
    fragment_hash: Hash,
    hash_entry_present: bool,
    compressed_idx: usize,
    compressed_capacity: usize,
    recent_fragments: &mut RingMap<Hash, FixedBitSet>,
) {
    if hash_entry_present {
        // Try safe mutable access to existing entry — don't panic if absent (defensive).
        if let Some(entry) = recent_fragments.get_mut(&fragment_hash) {
            entry.set(compressed_idx, true);
            return;
        }
        // Fallthrough: entry disappeared (evicted) — create a new entry below.
    } else if recent_fragments.len() == recent_fragments.capacity() {
        // evict
        let _ = recent_fragments.pop_front();
    }
    let mut new_bitset = FixedBitSet::with_capacity(compressed_capacity);
    new_bitset.set(compressed_idx, true);
    recent_fragments.push_back(fragment_hash, new_bitset);
}

#[inline(always)]
fn build_fragment(packet: Bytes, fragment_hash: Hash, fragment_index: u16, total_fragments: u16) -> Fragment {
    let payload = Bytes::copy_from_slice(&packet[AuthToken::TOKEN_SIZE + FragmentHeader::SIZE..]);
    Fragment { header: FragmentHeader::new(fragment_hash, fragment_index, total_fragments), payload }
}

#[inline(always)]
fn is_fragment_index_valid(fragment_index: u16, total_fragments: u16) -> bool {
    fragment_index <= total_fragments
}

#[inline(always)]
fn is_authenticated(packet: &[u8], authenticator: &TokenAuthenticator) -> bool {
    if packet.len() < AuthToken::TOKEN_SIZE {
        return false;
    }
    authenticator.verify_mac(&packet[AuthToken::TOKEN_SIZE..], &packet[..AuthToken::TOKEN_SIZE])
}

#[inline(always)]
fn is_from_allowlist(src: &SocketAddr, allowlist: &Allowlist) -> bool {
    allowlist.load().contains_key(&src.ip())
}

#[inline(always)]
fn is_relay_ready(is_ready: &Arc<AtomicBool>) -> bool {
    is_ready.load(Ordering::Relaxed)
}

#[inline(always)]
fn is_unique_fragment(
    fragment_hash: Hash,
    compressed_idx: u16,
    compressed_capacity: u16,
    recent_fragments: &mut RingMap<Hash, FixedBitSet>,
) -> (bool, bool) {
    // Return tuple: (is_unique, entry_present)
    if let Some(existing) = recent_fragments.get(&fragment_hash) {
        // existing entry: validate shape and check duplicate (read-only)
        if existing.len() != compressed_capacity as usize {
            return (false, true);
        }
        if existing.contains(compressed_idx as usize) {
            return (false, true);
        }
        (true, true)
    } else {
        (true, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;

    #[test]
    fn test_update_recent_fragments_handles_missing_entry_gracefully() {
        let mut recent_fragments = RingMap::<Hash, FixedBitSet>::with_capacity(2);
        let fragment_hash = Hash::from_slice(&[42u8; 32]);

        // Caller indicates an existing entry, but the map does not contain it yet —
        // update_recent_fragments should create the entry instead of panicking.
        update_recent_fragments(fragment_hash, true, 0, 1, &mut recent_fragments);

        assert_eq!(recent_fragments.len(), 1);
        let entry = recent_fragments.get(&fragment_hash).expect("entry should exist after update");
        assert!(entry.contains(0));
    }
}
