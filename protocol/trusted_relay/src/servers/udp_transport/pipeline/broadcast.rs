use std::sync::Arc;

use kaspa_core::{debug, trace, warn};
use kaspa_hashes::Hash;

use crate::servers::auth::TokenAuthenticator;
use crate::{
    codec::encoder::FragmentGenerator,
    model::{fragments::FragmentHeader, ftr_block::FtrBlock},
    params::FragmentationConfig,
    servers::{
        auth::AuthToken,
        peer_directory::{PeerDirectory, PeerInfoList},
        udp_transport::pipeline::verification::{MarkBlockBroadcastedMessage, VerificationMessage},
    },
};
const WORKER_NAME: &str = "broadcast-worker";

pub type BroadcastSender = crossbeam_channel::Sender<BroadcastMessage>;
pub type BroadcastReceiver = crossbeam_channel::Receiver<BroadcastMessage>;

pub struct BroadcastMessage(Hash, Arc<FtrBlock>);

impl BroadcastMessage {
    pub fn new(hash: Hash, block: Arc<FtrBlock>) -> Self {
        Self(hash, block)
    }
}

/// Run the broadcast loop. Blocks until all senders are dropped.
fn run(
    broadcaster_idx: usize,
    receiver: crossbeam_channel::Receiver<BroadcastMessage>,
    peer_info_list: PeerInfoList,
    authenticator: Arc<TokenAuthenticator>,
    config: FragmentationConfig,
    verification_senders: Vec<crossbeam_channel::Sender<VerificationMessage>>,
) {
    debug!("{}-{} started", WORKER_NAME, broadcaster_idx);

    let mut framed = vec![0u8; FragmentHeader::SIZE + config.payload_size + AuthToken::TOKEN_SIZE];
    while let Ok(BroadcastMessage(hash, ftr_block)) = receiver.recv() {
        let peers = peer_info_list.load_full();
        let outbound_peers: Vec<_> = peers.iter().filter(|p| p.is_outbound_ready()).collect();
        if outbound_peers.is_empty() {
            debug!(
                "{}-{}: no outbound-ready peers, skipping block {} (total peers: {})",
                WORKER_NAME,
                broadcaster_idx,
                hash,
                peers.len()
            );
            continue;
        }

        trace!("{}-{}: encoding block {} for {} peer(s)", WORKER_NAME, broadcaster_idx, hash, outbound_peers.len());
        let fragment_gen = FragmentGenerator::new(config, hash, Arc::unwrap_or_clone(ftr_block));

        // Notify verification workers that this block is being broadcasted,
        // marking all its shards as seen to filter duplicates.
        let total_fragments = fragment_gen.total_fragments() as u16;
        for tx in verification_senders.iter() {
            let _ = tx.try_send(VerificationMessage::MarkBlockBroadcasted(MarkBlockBroadcastedMessage::new(hash, total_fragments)));
        }

        for fragment in fragment_gen {
            // Wire format: [0..32] MAC | [32..68] FragmentHeader | [68..] Payload
            framed[AuthToken::TOKEN_SIZE..AuthToken::TOKEN_SIZE + FragmentHeader::SIZE].copy_from_slice(fragment.header.as_bytes());
            framed
                [AuthToken::TOKEN_SIZE + FragmentHeader::SIZE..AuthToken::TOKEN_SIZE + FragmentHeader::SIZE + fragment.payload.len()]
                .copy_from_slice(fragment.payload.as_ref());
            // MAC covers everything after the token slot (header + payload)
            let mac = authenticator.mac(&framed[AuthToken::TOKEN_SIZE..]);
            framed[..AuthToken::TOKEN_SIZE].copy_from_slice(&mac);

            for peer in &outbound_peers {
                if let Some(socket) = peer.send_socket() {
                    if let Err(e) = socket.send(&framed) {
                        warn!("{}-{}: send to {}: {}", WORKER_NAME, broadcaster_idx, peer.udp_target(), e);
                    }
                } else {
                    warn!("{}-{}: no send socket for peer {}", WORKER_NAME, broadcaster_idx, peer.udp_target());
                }
            }
        }
    }
    // receive loop exits when channel is disconnected.
    debug!("{}-{} exited", WORKER_NAME, broadcaster_idx);
}

pub fn spawn_broadcaster_thread(
    broadcaster_idx: usize,
    directory: Arc<PeerDirectory>,
    receiver: BroadcastReceiver,
    authenticator: Arc<TokenAuthenticator>,
    config: FragmentationConfig,
    verification_senders: Vec<crossbeam_channel::Sender<VerificationMessage>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("{}-{}", WORKER_NAME, broadcaster_idx))
        .spawn(move || run(broadcaster_idx, receiver, directory.peer_info_list(), authenticator, config, verification_senders.clone()))
        .unwrap_or_else(|_| panic!("Failed to spawn {}-{} thread", WORKER_NAME, broadcaster_idx))
}
