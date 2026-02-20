use std::mem::MaybeUninit;
use std::sync::Arc;

use crossbeam_channel::RecvError;
use kaspa_hashes::Hash;
use log::{info, trace, warn};
use std::net::UdpSocket;

use crate::{codec::encoder::FragmentGenerator, model::{fragments::FragmentHeader, ftr_block::FtrBlock}, params::FragmentationConfig, servers::{auth::AuthToken, peer_directory::{PeerDirectory, PeerInfoList}, udp_transport::pipeline::verification::{MarkBlockBroadcastedMessage, VerificationMessage}}};
use crate::servers::auth::TokenAuthenticator;
const WORKER_NAME : &str = "broadcast-worker";

pub type BroadcastSender = crossbeam_channel::Sender<BroadcastMessage>;
pub type BroadcastReceiver = crossbeam_channel::Receiver<BroadcastMessage>;

pub struct BroadcastMessage(Hash, FtrBlock);

impl BroadcastMessage {
    #[inline(always)]
    pub fn new(hash: Hash, block: FtrBlock) -> Self {
        Self(hash, block)
    }

    #[inline(always)]
    pub fn hash(self) -> Hash {
        self.0
    }

    #[inline(always)]
    pub fn block(self) -> FtrBlock {
        self.1
    }
}

/// Run the broadcast loop. Blocks until all senders are dropped.
fn run(
        broadcaster_idx: usize,
        receiver: crossbeam_channel::Receiver<BroadcastMessage>,
        socket: Arc<UdpSocket>,
        peer_info_list: PeerInfoList,
        authenticator: Arc<TokenAuthenticator>,
        config: FragmentationConfig,
        verification_senders: Vec<crossbeam_channel::Sender<VerificationMessage>>
    ) {
        info!("{}-{} started", WORKER_NAME, broadcaster_idx);

        let mut framed = vec![0u8; FragmentHeader::SIZE + config.payload_size + AuthToken::TOKEN_SIZE];
        while let Ok(BroadcastMessage { 0: hash, 1: ftr_block }) = receiver.recv() {
            let peers = peer_info_list.load_full();
            let outbound_peers: Vec<_> = peers.iter().filter(|p| p.is_outbound_ready()).collect();
                    if outbound_peers.is_empty() {
                        trace!("{}-{}: no outbound-ready peers, skipping block {}", WORKER_NAME, broadcaster_idx, hash);
                        continue;
                    }

                    trace!("{}-{}: encoding block {} for {} peer(s)", WORKER_NAME, broadcaster_idx, hash, outbound_peers.len());
                    let fragment_gen =FragmentGenerator::new(config, hash, ftr_block);

                    // Notify verification workers that this block is being broadcasted,
                    // marking all its shards as seen to filter duplicates.
                    let total_fragments = fragment_gen.total_fragments() as u16;
                    for tx in verification_senders.iter() {
                        let _ = tx.try_send(VerificationMessage::MarkBlockBroadcasted(
                            MarkBlockBroadcastedMessage::new(hash, total_fragments)
                        ));
                    }

                    for fragment in fragment_gen {
                        framed[..FragmentHeader::SIZE].copy_from_slice(fragment.header.as_bytes());
                        framed[FragmentHeader::SIZE..FragmentHeader::SIZE + fragment.payload.len()].copy_from_slice(fragment.payload.as_ref());
                        let mac = authenticator.mac(&framed[AuthToken::TOKEN_SIZE..]);
                        framed[..AuthToken::TOKEN_SIZE].copy_from_slice(&mac);

                        for peer in &outbound_peers {
                            if let Err(e) = socket.send_to(&framed, peer.udp_target()) {
                                warn!("{}-{}: send to {}: {}", WORKER_NAME, broadcaster_idx, peer.udp_target(), e);
                            }
                        }
                    }
                };
            // receive loop exits when channel is disconnected.
            info!("{}-{} exited", WORKER_NAME, broadcaster_idx);
        }

pub fn spawn_broadcaster_thread(
    broadcaster_idx: usize,
    socket: Arc<UdpSocket>,
    directory: Arc<PeerDirectory>,
    receiver: BroadcastReceiver,
    authenticator: Arc<TokenAuthenticator>,
    config: FragmentationConfig,
    verification_senders: Vec<crossbeam_channel::Sender<VerificationMessage>>,
) -> std::thread::JoinHandle<()> {
    let handle = std::thread::Builder::new()
        .name(format!("{}-{}", WORKER_NAME, broadcaster_idx))
        .spawn(move || run(broadcaster_idx, receiver, socket, directory.peer_info_list(), authenticator, config, verification_senders.clone()))
        .expect(&format!("Failed to spawn {}-{} thread", WORKER_NAME, broadcaster_idx));
    handle
}
