use kaspa_hashes::Hash;

const MAX_EXPECTED_BLOCK_SIZE: usize = 1024 * 1024; // 1 MiB
const BUFFER_SIZE_32MB: usize = 32 * 1024 * 1024;

/// FEC Fragmentation parameters: purely protocol-level, no worker counts.
#[derive(Debug, Clone, Copy)]
pub struct FragmentationConfig {
    /// Number of data blocks (k)
    pub data_blocks: usize,
    /// Number of parity blocks (m)
    pub parity_blocks: usize,
    /// UDP payload size in bytes (should be less than MTU, typically 1500 bytes for Ethernet)
    pub payload_size: usize,
}

impl FragmentationConfig {
    pub fn new(data_blocks: usize, parity_blocks: usize, payload_size: usize) -> Self {
        Self { data_blocks, parity_blocks, payload_size }
    }

    pub fn fragments_per_generation(&self) -> usize {
        self.data_blocks + self.parity_blocks
    }

    /// Minimum number of blocks needed to reconstruct data
    pub fn min_recovery_blocks(&self) -> usize {
        self.data_blocks
    }

    pub fn calculate_last_k_and_m(&self, total_fragments: usize) -> (usize, usize) {
        let gen_size = self.fragments_per_generation();
        let last_gen_fragments = total_fragments % gen_size;
        if last_gen_fragments == 0 {
            (self.data_blocks, self.parity_blocks)
        } else {
            // Split last_gen_fragments proportionally to maintain k:m ratio
            let last_k = (last_gen_fragments * self.data_blocks) / (self.data_blocks + self.parity_blocks);
            let last_m = last_gen_fragments - last_k; // remaining must be m
            (last_k, last_m)
        }
    }

    pub fn get_hash_bucket(&self, hash: Hash, bucket_size: usize) -> usize {
        // we use the 2nd index, because the 3rd is used for the BlockHashMap,
        // since we use that as well, and we want to avoid overlap.
        hash.to_le_u64()[2] as usize % bucket_size
    }
}

/// Transport runtime parameters (UDP buffer sizes, channel capacities, worker counts).
#[derive(Debug, Clone, Copy)]
pub struct TransportParams {
    /// Default UDP receive buffer size used by the collector.
    pub default_buffer_size: usize,

    /// Number of workers.
    pub num_of_collectors: usize,
    pub num_of_verifiers: usize,
    pub num_of_forwarders: usize,
    pub num_of_broadcasters: usize,
    pub num_of_coordinators: usize,
    pub num_of_decoders_per_coordinators: usize,

    /// Consensus Params.
    pub consensus_bps: usize,
    pub consensus_k: usize,
    pub consensus_mergeset_root: usize,

    /// Local Fragmentation params
    pub k: usize,
    pub m: usize,
    pub payload_size: usize,

    /// Peer counts
    pub num_of_incoming_peers: usize,
    pub num_of_outgoing_peers: usize,

    /// custom multiplier, for adjustments.
    pub multiplier: f64,
}

impl TransportParams {
    // TODO: correct these estimations based on real-world testing and metrics, and adjust the formulas as needed.
    // Best effort estimations on good sizes, taking into account:
    // Max expected block sizes, peer counts, consensus variables, Fragmentation configs, number of workers.

    pub fn block_cache_capacity(&self) -> usize {
        ((self.consensus_mergeset_root * 2) as f64 * self.multiplier) as usize
    }

    pub fn coordinator_block_cache_capacity(&self) -> usize {
        self.block_cache_capacity() / self.num_of_coordinators
    }

    pub fn verification_channel_capacity(&self) -> usize {
        self.receive_buffer_size() / self.num_of_verifiers
    }

    pub fn forwarder_channel_capacity(&self) -> usize {
        self.send_buffer_size()
    }

    pub fn broadcast_channel_capacity(&self) -> usize {
        self.send_buffer_size()
    }

    pub fn coordinator_receive_channel_capacity(&self) -> usize {
        ((self.consensus_k * self.fragments_per_block() / self.num_of_coordinators) as f64 * self.multiplier) as usize
    }

    pub fn coordinator_send_channel_capacity(&self) -> usize {
        (self.consensus_k as f64 * self.multiplier) as usize
    }

    pub fn decoder_channel_capacity(&self) -> usize {
        ((self.consensus_k * self.fragments_per_block() / self.num_of_decoders_per_coordinators) as f64 * self.multiplier) as usize
    }

    pub fn send_buffer_size(&self) -> usize {
        BUFFER_SIZE_32MB
    }

    pub fn receive_buffer_size(&self) -> usize {
        BUFFER_SIZE_32MB
    }

    pub fn fragments_per_block(&self) -> usize {
        (MAX_EXPECTED_BLOCK_SIZE / (self.payload_size + 1)) + (MAX_EXPECTED_BLOCK_SIZE / (self.payload_size + 1)) * (self.m / self.k)
    }

    pub fn generations_per_block(&self) -> usize {
        self.fragments_per_block() / (self.k + self.m)
    }

    pub fn max_concurrent_blocks(&self) -> usize {
        self.consensus_k * 10
    }
}

impl Default for TransportParams {
    fn default() -> Self {
        Self {
            default_buffer_size: 2048,
            num_of_collectors: 1,
            num_of_verifiers: 1,
            num_of_forwarders: 1,
            num_of_broadcasters: 1,
            num_of_coordinators: 1,
            num_of_decoders_per_coordinators: 2,
            multiplier: 1.0,
            consensus_bps: 10,
            consensus_mergeset_root: 1200,
            consensus_k: 256,
            k: 16,
            m: 4,
            payload_size: 1200,
            num_of_incoming_peers: 4,
            num_of_outgoing_peers: 4,
        }
    }
}

/// Top-level container for trusted-relay runtime parameters.
#[derive(Debug, Clone, Copy)]
pub struct TrustedRelayParams {
    pub fragmentation: FragmentationConfig,
    pub transport: TransportParams,
}

impl TrustedRelayParams {
    pub fn new(fragmentation: FragmentationConfig, transport: TransportParams) -> Self {
        Self { fragmentation, transport }
    }

    /// Convenience constructor with sane defaults for transport/decoding.
    pub fn default_with_fragmentation(data_blocks: usize, parity_blocks: usize, payload_size: usize) -> Self {
        Self {
            fragmentation: FragmentationConfig::new(data_blocks, parity_blocks, payload_size),
            transport: TransportParams::default(),
        }
    }
}

impl Default for TrustedRelayParams {
    fn default() -> Self {
        Self::default_with_fragmentation(16, 4, 1200)
    }
}
