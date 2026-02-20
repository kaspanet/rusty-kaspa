use std::sync::Arc;

use crossbeam_channel::{Receiver, Sender, select_biased};
use kaspa_consensus_core::{BlockHashMap, BlockHasher};
use kaspa_core::{debug, info, trace, warn};
use kaspa_hashes::Hash;
use ringmap::RingSet;
use tokio::sync::mpsc::{UnboundedReceiver as TokioReceiver, UnboundedSender as TokioSender};

use crate::{codec::{buffers::BlockDecodeState, decoder::DecodeResult}, model::{fragments::Fragment, ftr_block::FtrBlock}, params::FragmentationConfig, servers::udp_transport::pipeline::reassembly::decoding::{DecodeJobMessage, DecodingJobSender, DecodingResultReceiver}};


pub const WORKER_NAME: &str = "block-reassembler-worker";

pub struct ReassemblerFragmentMessage(Fragment);

impl ReassemblerFragmentMessage {
    #[inline(always)]
    pub fn new(fragment: Fragment) -> Self {
        Self(fragment)
    }

    #[inline(always)]
    pub fn fragment(self) -> Fragment {
        self.0
    }
}

pub struct BlockReassemblerBlockMessage(Hash, FtrBlock);

impl BlockReassemblerBlockMessage {
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

    #[inline(always)]
    pub fn into_parts(self) -> (Hash, FtrBlock) {
        (self.0, self.1)
    }
}

pub type ReassemblerFragmentReceiver = Receiver<ReassemblerFragmentMessage>;
pub type ReassemblerFragmentSender = Sender<ReassemblerFragmentMessage>;

pub type ReassemblerBlockSender = TokioSender<BlockReassemblerBlockMessage>;
pub type ReassemblerBlockReceiver = TokioReceiver<BlockReassemblerBlockMessage>;
// ============================================================================
// DECODER CONFIG
// ============================================================================

pub fn run(
    reassembler_idx: usize,
    fragment_receiver: ReassemblerFragmentReceiver,
    decoder_job_sender: DecodingJobSender,
    decoder_result_receiver: DecodingResultReceiver,
    block_sender: Arc<ReassemblerBlockSender>,
    mut processed_block_cache: RingSet<Hash, BlockHasher>,
    mut partial_blocks: BlockHashMap<BlockDecodeState>,
    max_congruent_blocks: usize,
    config: FragmentationConfig
) {

    loop {
        select_biased!(
            recv(decoder_result_receiver) -> msg => match msg {
                Ok(msg) => handle_decode_result(msg.result(), &block_sender, &mut partial_blocks, &mut processed_block_cache),
                Err(e) => panic!("Result channel disconnected: {}", e)
            },
            recv(fragment_receiver) -> msg => match msg {
                Ok(msg) => handle_fragment(msg.fragment(), &decoder_job_sender, max_congruent_blocks, &mut partial_blocks, &mut processed_block_cache, &config),
                Err(_) => {
                    // exit
                    break;
                },
            },
        )
    }

    info!("Coordinator event loop exited: all channels closed");
}

    // ========================================================================
    // Internal handlers
    // ========================================================================

fn handle_fragment(fragment: Fragment, decoder_job_sender: &DecodingJobSender, max_congruent_blocks: usize, partial_blocks: &mut BlockHashMap<BlockDecodeState>, processed_block_cache: &mut RingSet<Hash, BlockHasher>, config: &FragmentationConfig) {
    if processed_block_cache.contains(&fragment.header.block_hash()) {
        trace!("Received fragment for block {} which has already been decoded", fragment.header.block_hash());
    };

    let hash = fragment.header.block_hash();
    let gen_size = config.fragments_per_generation();
    let generation = fragment.header.fragment_generation(config.data_blocks as u16, config.parity_blocks as u16) as usize;
    let index_within_gen = fragment.header.index_within_generation(gen_size as u16) as usize;

    // Enforce max_concurrent_blocks: if we're at the limit and this is a
    // new block, evict the oldest entry to prevent unbounded growth.
    if !partial_blocks.contains_key(&hash) && partial_blocks.len() >= max_congruent_blocks {
        // Evict the oldest block (first inserted) to make room.
        if let Some((&oldest_hash, _)) = partial_blocks.iter().next() {
            warn!(
                "Evicting stale block {} to enforce max_concurrent_blocks ({})",
                oldest_hash, max_congruent_blocks
            );
            partial_blocks.remove(&oldest_hash);
        }
    }

    // Get-or-create block state
    let state =
        partial_blocks.entry(hash).or_insert_with(|| BlockDecodeState::new(hash, fragment.header.total_fragments() as usize, *config));

    // Insert fragment; returns true if decodable (and not previously decodable) after this insert
    if state.encoded.insert_fragment(generation, index_within_gen, fragment) {
        let job = state.encoded.extract_job_from_generation(generation);
        if let Err(e) = decoder_job_sender.try_send(DecodeJobMessage::new(job)) {
            warn!("Error dispatching decode job for block {}: {}", state.metadata.hash, e);
        }
    }
}

fn handle_decode_result(result: DecodeResult, block_sender: &ReassemblerBlockSender, partial_blocks: &mut BlockHashMap<BlockDecodeState>, processed_block_cache: &mut RingSet<Hash, BlockHasher>) {
    let state = match partial_blocks.get_mut(&result.hash) {
        Some(s) => s,
        None => {
            debug!("Received decode result for block {} which has no state — likely already cleaned up", result.hash);
            return;
        }
    };
    let all_done = state.decoded.store(result.generation, result.data);

    if all_done {
        // Remove state and reassemble
        if processed_block_cache.len() >= processed_block_cache.capacity() {
            processed_block_cache.pop_front();
        }
        processed_block_cache.push_back(result.hash);
        if let Some(state) = partial_blocks.remove(&result.hash) {
            let block_data =  state.decoded.reassemble();
            block_sender.send(BlockReassemblerBlockMessage::new(result.hash, FtrBlock::from(block_data))).unwrap_or_else(|e| warn!("Failed to send reassembled block {}: {}", result.hash, e));
        };
    }
}

pub fn spawn_reassembler_thread(
    reassembler_idx: usize,
    fragment_receiver: ReassemblerFragmentReceiver,
    decoder_job_sender: DecodingJobSender,
    decoder_result_receiver: DecodingResultReceiver,
    block_sender: Arc<ReassemblerBlockSender>,
    processed_block_cache: RingSet<Hash, BlockHasher>,
    partial_blocks: BlockHashMap<BlockDecodeState>,
    max_congruent_blocks: usize,
    config: FragmentationConfig
) -> std::thread::JoinHandle<()> {
    let handle = std::thread::Builder::new()
        .name(format!("{}-{}", WORKER_NAME, reassembler_idx))
        .spawn(move || run(reassembler_idx, fragment_receiver, decoder_job_sender, decoder_result_receiver, block_sender, processed_block_cache, partial_blocks, max_congruent_blocks, config))
        .expect(format!("Failed to spawn {}-{} thread", WORKER_NAME, reassembler_idx).as_str());
    handle
}
/*
#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_hashes::Hash;

    /// End-to-end test: encode serialized block data, drop one data fragment per generation,
    /// feed remaining fragments through a real Coordinator, and verify the data
    /// is reassembled and converted to FtrBlock.
    #[test]
    fn lossy_decode_through_coordinator() {
        use crate::fragmenting::encoder::fragmentGenerator;

        let config = fragmentingConfig::new(16, 4, 1200);
        let gen_size = config.fragments_per_generation() as u16;
        let decoder_config = DecoderConfig::default();

        let (coordinator, fragment_tx, block_rx) = Coordinator::new(config, decoder_config).unwrap();
        let handle = std::thread::spawn(move || coordinator.run());

        // Create properly serialized FtrBlock (hash + header_len + txs_len + header + txs)
        let header_data = bincode::serialize(&0u32).unwrap(); // Minimal header for testing
        let txs_data = bincode::serialize(&vec![0u8; 400 * 1024]).unwrap(); // Some tx data
        let hash = Hash::from_bytes([0xEE; 32]);
        let ftr_block = FtrBlock::new(hash, header_data.len() as u32, txs_data.len() as u32, header_data, txs_data);

        let fragments: Vec<_> = fragmentGenerator::new(config, hash, ftr_block).collect();

        // Drop the first data fragment (index_within_gen == 0) of every generation
        let mut sent = 0usize;
        let mut dropped = 0usize;
        for fragment in &fragments {
            if fragment.header().index_within_generation(gen_size) == 0 {
                dropped += 1;
                continue;
            }
            fragment_tx.send(fragment.clone()).unwrap();
            sent += 1;
        }

        match block_rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok((h, ftr_block)) => {
                // Verify we received the correct hash and can access FtrBlock fields
                assert_eq!(h, hash, "Hash mismatch");
                assert_eq!(ftr_block.hash(), hash, "FtrBlock hash mismatch");
                // Verify the FtrBlock was created correctly
                assert!(ftr_block.header_len() > 0, "Header length should be positive");
            }
            Err(e) => {
                panic!("Lossy decode failed: {:?}. Sent {} fragments, dropped {}.", e, sent, dropped);
            }
        }

        drop(fragment_tx);
        handle.join().unwrap();
    }

    #[test]
    fn processed_blocks_never_exceeds_capacity_on_many_decode_results() {
        let config = fragmentingConfig::new(4, 2, 1200);
        let mut decoder_config = DecoderConfig::default();
        decoder_config.processed_block_cache_capacity = 4;

        let (mut coordinator, _fragment_tx, _block_rx) = Coordinator::new(config.clone(), decoder_config.clone()).unwrap();

        // Simulate many decode results for distinct hashes and assert the
        // processed_blocks RingSet never grows beyond the configured capacity.
        let inserts = 16usize;
        for i in 0..inserts {
            let mut bytes = [0u8; 32];
            bytes[0] = (i as u8).wrapping_add(1);
            let hash = Hash::from_bytes(bytes);

            // Create minimal block state so handle_decode_result finds state.
            let total_fragments = config.fragments_per_generation() as usize;
            coordinator.blocks.entry(hash).or_insert_with(|| BlockDecodeState::new(hash, total_fragments, config.clone()));

            // Supply a decode result for generation 0 with a payload sized to k * payload_size
            let data_len = config.data_blocks * config.payload_size;
            coordinator.handle_decode_result(DecodeResult { hash, generation: 0, data: vec![0u8; data_len] });

            assert!(
                coordinator.processed_blocks.len() <= decoder_config.processed_block_cache_capacity,
                "processed_blocks exceeded capacity at iteration {}",
                i
            );
        }
    }
}
*/
