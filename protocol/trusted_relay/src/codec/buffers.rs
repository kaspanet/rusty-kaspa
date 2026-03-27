use bytes::Bytes;
use kaspa_hashes::Hash;

use crate::{codec::decoder::DecodeJob, model::fragments::Fragment, params::FragmentationConfig};

// Buffers designed for holding accumulated fragments, and for reassembling from such fragments.

// ============================================================================
// BUFFER METADATA
// ============================================================================

/// Computed metadata for a block being decoded.
/// Derived once from the first fragment's header and the `FragmentationConfig`.
#[derive(Clone, Copy)]
pub(crate) struct BufferMetadata {
    pub hash: Hash,
    pub total_generations: usize,
    pub k: usize,
    pub m: usize,
    pub payload_size: usize,
    pub last_gen_k: usize,
    pub last_gen_m: usize,
}

impl BufferMetadata {
    pub fn new(hash: Hash, total_fragments: usize, config: FragmentationConfig) -> Self {
        let gen_size = config.fragments_per_generation();
        let total_generations = total_fragments.div_ceil(gen_size);
        let (last_gen_k, last_gen_m) = config.calculate_last_k_and_m(total_fragments);
        Self {
            hash,
            total_generations,
            k: config.data_blocks,
            m: config.parity_blocks,
            payload_size: config.payload_size,
            last_gen_k,
            last_gen_m,
        }
    }

    /// Returns (k, m) for the given generation index.
    pub fn k_m_for_generation(&self, generation: usize) -> (usize, usize) {
        if generation == self.total_generations - 1 { (self.last_gen_k, self.last_gen_m) } else { (self.k, self.m) }
    }
}

// ============================================================================
// GENERATIONAL fragment BUFFER
// ============================================================================

/// Accumulates fragments for a single generation until decodable.
/// No locks needed — only the single-threaded Coordinator touches this.
///
/// Supports two decode trigger conditions (see `insert()` for details):
/// 1. All k data fragments arrived → fast-path (no RS recovery needed)
/// 2. k total fragments arrived (any mix) → may need RS recovery
#[derive(Clone)]
pub(crate) struct GenerationalfragmentBuffer {
    generation: usize,
    k: usize,
    m: usize,
    /// Indexed by position within generation (0..k+m).
    /// Data fragments occupy 0..k, parity fragments occupy k..k+m.
    data_fragments: Vec<Option<Fragment>>,
    data_fragment_count: usize,
    parity_fragments: Vec<Option<Fragment>>,
    parity_fragment_count: usize,
    /// Whether a decode job has been dispatched for this generation.
    /// Used to continue accepting data fragments after first dispatch
    /// (to potentially trigger a fast-path re-dispatch).
    dispatched: bool,
}

impl GenerationalfragmentBuffer {
    pub fn new(generation: usize, k: usize, m: usize) -> Self {
        Self {
            generation,
            k,
            m,
            data_fragments: vec![None; k],
            data_fragment_count: 0,
            parity_fragments: vec![None; m],
            parity_fragment_count: 0,
            dispatched: false,
        }
    }

    /// Insert a fragment by its index within this generation.
    ///
    /// Returns `true` if this insertion crossed a decode threshold for the first time.
    ///
    /// ## Why Two Conditions?
    ///
    /// With UDP, packet reordering is common. If parity fragments arrive before some data
    /// fragments (reaching k total), we'd trigger RS recovery even though waiting slightly
    /// longer might give us all k data fragments (enabling the much cheaper fast-path).
    ///
    /// To optimize for this, we support two decode triggers:
    ///
    /// 1. **All k data fragments arrived** → Triggers fast-path decode (just concatenation,
    ///    no Reed-Solomon recovery needed). This is the ideal case.
    ///
    /// 2. **k total fragments arrived** (any mix of data + parity) → Triggers decode that
    ///    may require RS recovery if some data fragments are missing.
    ///
    /// If condition #2 fires first (parity pushed us to k total before we had all data),
    /// we continue accepting data fragments. If condition #1 fires later (we now have all
    /// k data), we dispatch again — the fast-path decode may win the race and the
    /// redundant slow-path result is simply discarded (DecodedBuffer::store is idempotent).
    ///
    /// Once we have all k data fragments AND have dispatched, we stop accepting fragments
    /// entirely since we've achieved the optimal decode condition.
    pub fn insert(&mut self, index_within_gen: usize, fragment: Fragment) -> bool {
        if index_within_gen >= self.data_fragments.len() + self.parity_fragments.len() {
            log::warn!(
                "fragment index {} out of bounds for generation {} (capacity {})",
                index_within_gen,
                self.generation,
                self.data_fragments.len() + self.parity_fragments.len()
            );
            return false;
        }

        // Early exit: stop accepting fragments once we've dispatched AND have all data.
        // Before that point, we keep accepting data fragments to potentially trigger
        // a fast-path re-dispatch even after an initial parity-triggered dispatch.
        if self.dispatched && self.data_fragment_count >= self.k {
            return false;
        }

        let total_before = self.data_fragment_count + self.parity_fragment_count;
        let data_before = self.data_fragment_count;

        if index_within_gen < self.k {
            // Data fragment
            if self.data_fragments[index_within_gen].is_some() {
                return false; // Duplicate
            }
            self.data_fragment_count += 1;
            self.data_fragments[index_within_gen] = Some(fragment);
        } else {
            // Parity fragment — only accept if we haven't dispatched yet
            // (no point collecting more parity after first dispatch)
            if self.dispatched {
                return false;
            }
            let parity_index = index_within_gen - self.k;
            if self.parity_fragments[parity_index].is_some() {
                return false; // Duplicate
            }
            self.parity_fragment_count += 1;
            self.parity_fragments[parity_index] = Some(fragment);
        }

        let total_after = self.data_fragment_count + self.parity_fragment_count;
        let data_after = self.data_fragment_count;

        // Trigger decode on EITHER threshold being crossed for the first time:
        // 1. All k data fragments (fast-path possible)
        // 2. k total fragments (may need RS recovery)
        let crossed_data_threshold = data_before < self.k && data_after >= self.k;
        let crossed_total_threshold = total_before < self.k && total_after >= self.k;

        crossed_data_threshold || crossed_total_threshold
    }

    /// Mark this generation as having been dispatched for decoding.
    pub fn mark_dispatched(&mut self) {
        self.dispatched = true;
    }

    /// Extract data and parity fragment payloads for the decode worker.
    /// Data slots: indices 0..k (Some if present, None if missing).
    /// Parity slots: indices 0..m (Some if present, None if missing).
    ///
    /// Takes ownership of the fragment payloads (moves them out), freeing the
    /// memory held by the buffer and avoiding the double-retention that the
    /// previous clone-based implementation caused.
    pub fn take_for_decode(&mut self) -> (Vec<Option<Bytes>>, Vec<Option<Bytes>>) {
        let data = self.data_fragments.iter_mut().map(|opt| opt.take().map(|fragment| fragment.payload)).collect();
        let parity = self.parity_fragments.iter_mut().map(|opt| opt.take().map(|fragment| fragment.payload)).collect();
        (data, parity)
    }

    /// Clone fragment payloads for decode without consuming them.
    /// Used for slow-path dispatch when we may want a subsequent fast-path dispatch.
    pub fn clone_for_decode(&self) -> (Vec<Option<Bytes>>, Vec<Option<Bytes>>) {
        let data = self.data_fragments.iter().map(|opt| opt.as_ref().map(|fragment| fragment.payload.clone())).collect();
        let parity = self.parity_fragments.iter().map(|opt| opt.as_ref().map(|fragment| fragment.payload.clone())).collect();
        (data, parity)
    }

    /// Returns true if all k data fragments are present (fast-path possible).
    pub fn has_all_data(&self) -> bool {
        self.data_fragment_count >= self.k
    }
}

// ============================================================================
// ENCODED BUFFER
// ============================================================================

/// Holds all generational fragment buffers for a single block.
/// No locks — single Coordinator thread owns this.
#[derive(Clone)]
pub(crate) struct EncodedBuffer {
    hash: Hash,
    generations: Vec<GenerationalfragmentBuffer>,
    /// Number of generations that have been dispatched for decoding.
    dispatched_count: usize,
    _payload_size: usize,
}

impl EncodedBuffer {
    pub fn new(metadata: &BufferMetadata) -> Self {
        let mut generations = Vec::with_capacity(metadata.total_generations);
        for g in 0..metadata.total_generations {
            let (k, m) = metadata.k_m_for_generation(g);
            generations.push(GenerationalfragmentBuffer::new(g, k, m));
        }
        Self { hash: metadata.hash, generations, dispatched_count: 0, _payload_size: metadata.payload_size }
    }

    /// Insert a fragment into the correct generational buffer.
    /// Returns `Some(generation_index)` if that generation just became decodable.
    pub fn insert_fragment(&mut self, generation: usize, index_within_gen: usize, fragment: Fragment) -> bool {
        if generation >= self.generations.len() {
            log::warn!("Generation {} out of bounds (total {})", generation, self.generations.len());
            return false;
        }

        self.generations[generation].insert(index_within_gen, fragment)
    }

    /// Extract fragment data for a generation that's ready to decode.
    /// Returns (data_slots, parity_slots) for the decode worker.
    ///
    /// If all k data fragments are present (fast-path), takes ownership of fragments.
    /// Otherwise (slow-path), clones fragments to allow a potential subsequent fast-path dispatch.
    pub fn extract_job_from_generation(&mut self, generation: usize) -> DecodeJob {
        self.dispatched_count += 1;
        let generational_buffer = &mut self.generations[generation];
        let k = generational_buffer.k;
        let m = generational_buffer.m;
        let num_of_data_fragments = generational_buffer.data_fragment_count;

        // If we have all data fragments, this is the optimal (final) dispatch — take ownership.
        // Otherwise, clone to keep fragments around for potential fast-path re-dispatch.
        let (data_fragments, parity_fragments) = if generational_buffer.has_all_data() {
            generational_buffer.take_for_decode()
        } else {
            generational_buffer.clone_for_decode()
        };

        generational_buffer.mark_dispatched();
        DecodeJob { hash: self.hash, generation, k, m, data_fragments, num_of_data_fragments, parity_fragments }
    }

    #[cfg(test)]
    pub fn all_dispatched(&self) -> bool {
        self.dispatched_count >= self.generations.len()
    }
}

// ============================================================================
// DECODED BUFFER
// ============================================================================

/// Collects decoded generation payloads and reassembles them in order.
/// No locks — single Coordinator thread owns this.
#[derive(Clone)]
pub(crate) struct DecodedBuffer {
    generations: Vec<Option<Vec<u8>>>,
    completed_count: usize,
    total_generations: usize,
}

impl DecodedBuffer {
    pub fn new(total_generations: usize) -> Self {
        Self { generations: vec![None; total_generations], completed_count: 0, total_generations }
    }

    /// Store a decoded generation's data. Returns `true` if all generations are now decoded.
    pub fn store(&mut self, generation: usize, data: Vec<u8>) -> bool {
        if self.generations[generation].is_none() {
            self.generations[generation] = Some(data);
            self.completed_count += 1;
        }
        self.is_complete()
    }

    pub fn is_complete(&self) -> bool {
        self.completed_count >= self.total_generations
    }

    /// Reassemble all decoded generations into final block data.
    /// Returns an error if not all generations are decoded.
    pub fn reassemble(self) -> Vec<u8> {
        if self.completed_count != self.total_generations {
            panic!("Cannot reassemble: only {}/{} generations decoded", self.completed_count, self.total_generations);
        }
        self.generations.into_iter().flatten().flatten().collect()
    }
}

// ============================================================================
// BLOCK DECODE STATE — ties everything together for one block
// ============================================================================

/// Complete decode state for a single block.
/// Owned entirely by the Coordinator — no Arc, no Mutex.
#[derive(Clone)]
pub(crate) struct BlockDecodeState {
    #[allow(dead_code)]
    pub metadata: BufferMetadata,
    pub encoded: EncodedBuffer,
    pub decoded: DecodedBuffer,
}

impl BlockDecodeState {
    pub fn new(hash: Hash, total_fragments: usize, config: FragmentationConfig) -> Self {
        let metadata = BufferMetadata::new(hash, total_fragments, config);
        let encoded = EncodedBuffer::new(&metadata);
        let decoded = DecodedBuffer::new(metadata.total_generations);
        Self { metadata, encoded, decoded }
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::panic;

    fn test_config() -> FragmentationConfig {
        FragmentationConfig::new(6, 3, 1024)
    }

    fn dummy_fragment(hash: Hash, index: u16, data: &[u8]) -> Fragment {
        let header = crate::model::fragments::FragmentHeader::new(hash, index, 18);
        Fragment { header, payload: Bytes::copy_from_slice(data) }
    }

    // ========================================================================
    // BufferMetadata Tests
    // ========================================================================

    #[test]
    fn test_metadata_single_generation_exact() {
        let config = test_config(); // k=6, m=3, gen_size=9
        let metadata = BufferMetadata::new(Hash::default(), 9, config);

        assert_eq!(metadata.total_generations, 1);
        assert_eq!(metadata.k, 6);
        assert_eq!(metadata.m, 3);
        assert_eq!(metadata.last_gen_k, 6);
        assert_eq!(metadata.last_gen_m, 3);
    }

    #[test]
    fn test_metadata_multiple_generations_partial_last() {
        let config = test_config(); // k=6, m=3, gen_size=9
        let metadata = BufferMetadata::new(Hash::default(), 15, config);

        assert_eq!(metadata.total_generations, 2);
        assert_eq!(metadata.k, 6);
        assert_eq!(metadata.m, 3);
        // Last gen has 6 fragments: 4 data, 2 parity (proportional split)
        assert_eq!(metadata.last_gen_k, 4);
        assert_eq!(metadata.last_gen_m, 2);
    }

    #[test]
    fn test_metadata_k_m_for_generation() {
        let config = test_config();
        let metadata = BufferMetadata::new(Hash::default(), 15, config);

        // Full generation
        let (k, m) = metadata.k_m_for_generation(0);
        assert_eq!(k, 6);
        assert_eq!(m, 3);

        // Last generation
        let (k, m) = metadata.k_m_for_generation(1);
        assert_eq!(k, 4);
        assert_eq!(m, 2);
    }

    // ========================================================================
    // GenerationalfragmentBuffer Tests
    // ========================================================================

    #[test]
    fn test_generational_buffer_insert_until_decodable() {
        let hash = Hash::default();
        let mut buf = GenerationalfragmentBuffer::new(0, 6, 3);

        // Insert 5 fragments (not yet decodable)
        for i in 0..5 {
            let fragment = dummy_fragment(hash, i as u16, b"data");
            let decodable = buf.insert(i, fragment);
            assert!(!decodable, "Buffer should not be decodable with {} fragments", i + 1);
        }

        // Insert 6th fragment (now decodable)
        let fragment = dummy_fragment(hash, 5, b"data");
        let decodable = buf.insert(5, fragment);
        assert!(decodable, "Buffer should be decodable with 6 fragments");

        // Adding more fragments doesn't change decodability flag (already true)
        let fragment = dummy_fragment(hash, 6, b"parity");
        let decodable = buf.insert(6, fragment);
        assert!(!decodable, "Decodability flag only true on first k fragments");
    }

    #[test]
    fn test_generational_buffer_duplicate_reject() {
        let hash = Hash::default();
        let mut buf = GenerationalfragmentBuffer::new(0, 6, 3);

        let fragment1 = dummy_fragment(hash, 0, b"data1");
        let inserted = buf.insert(0, fragment1);
        // First insert does not make the generation decodable (k=6), so insert returns false
        // but the fragment should have been recorded (count == 1).
        assert!(!inserted, "First insert should not yet be decodable");
        assert_eq!(buf.data_fragment_count, 1, "Data fragment count should be 1");

        let fragment2 = dummy_fragment(hash, 0, b"data2");
        let inserted = buf.insert(0, fragment2);
        assert!(!inserted, "Duplicate insert should return false");
        assert_eq!(buf.data_fragment_count, 1, "Count should not increase");
    }

    #[test]
    fn test_generational_buffer_out_of_bounds() {
        let hash = Hash::default();
        let mut buf = GenerationalfragmentBuffer::new(0, 6, 3); // Capacity 9

        let fragment = dummy_fragment(hash, 10, b"data");
        let inserted = buf.insert(10, fragment);
        assert!(!inserted, "Out-of-bounds insert should fail");
        assert_eq!(buf.data_fragment_count + buf.parity_fragment_count, 0, "Count should remain zero");
    }

    #[test]
    fn test_generational_buffer_take_for_decode() {
        let hash = Hash::default();
        let mut buf = GenerationalfragmentBuffer::new(0, 6, 3);

        // Insert some data fragments
        for i in 0..4 {
            let fragment = dummy_fragment(hash, i as u16, format!("data{}", i).as_bytes());
            buf.insert(i, fragment);
        }

        // Insert some parity fragments
        for i in 0..2 {
            let fragment = dummy_fragment(hash, (6 + i) as u16, format!("parity{}", i).as_bytes());
            buf.insert(6 + i, fragment);
        }

        let (data_slots, parity_slots) = buf.take_for_decode();

        // Should have 6 data slots (4 present, 2 missing)
        assert_eq!(data_slots.len(), 6);
        assert_eq!(data_slots.iter().filter(|s| s.is_some()).count(), 4);
        assert_eq!(data_slots.iter().filter(|s| s.is_none()).count(), 2);

        // Should have 3 parity slots (2 present, 1 missing)
        assert_eq!(parity_slots.len(), 3);
        assert_eq!(parity_slots.iter().filter(|s| s.is_some()).count(), 2);
        assert_eq!(parity_slots.iter().filter(|s| s.is_none()).count(), 1);
    }

    // ========================================================================
    // EncodedBuffer Tests
    // ========================================================================

    #[test]
    fn test_encoded_buffer_single_generation() {
        let config = test_config();
        let metadata = BufferMetadata::new(Hash::default(), 9, config);
        let mut encoded = EncodedBuffer::new(&metadata);

        let hash = Hash::default();

        // Insert 6 fragments (k) to make generation 0 decodable
        for i in 0..6 {
            let fragment = dummy_fragment(hash, i as u16, b"data");
            let ready = encoded.insert_fragment(0, i, fragment);
            if i == 5 {
                assert!(ready, "Generation 0 should become decodable");
            } else {
                assert!(!ready, "Generation 0 should not be ready yet");
            }
        }

        assert!(!encoded.all_dispatched());
    }

    #[test]
    fn test_encoded_buffer_multiple_generations() {
        let config = test_config();
        let metadata = BufferMetadata::new(Hash::default(), 18, config); // 2 full generations
        let mut encoded = EncodedBuffer::new(&metadata);

        let hash = Hash::default();

        // Fill generation 0 (k=6 fragments needed)
        for i in 0..6 {
            let fragment = dummy_fragment(hash, i as u16, b"gen0");
            let ready = encoded.insert_fragment(0, i, fragment);
            if i == 5 {
                assert!(ready, "Generation 0 should become decodable");
                // Simulate dispatching the generation
                let job = encoded.extract_job_from_generation(0);
                assert_eq!(job.data_fragments.len(), 6);
                assert_eq!(job.parity_fragments.len(), 3);
            }
        }

        // Fill generation 1 (k=6 fragments needed)
        for i in 0..6 {
            let fragment = dummy_fragment(hash, (9 + i) as u16, b"gen1");
            let ready = encoded.insert_fragment(1, i, fragment);
            if i == 5 {
                assert!(ready, "Generation 1 should become decodable");
                // Simulate dispatching the generation
                let job = encoded.extract_job_from_generation(1);
                assert_eq!(job.data_fragments.len(), 6);
                assert_eq!(job.parity_fragments.len(), 3);
            }
        }

        assert!(encoded.all_dispatched());
    }

    #[test]
    fn test_encoded_buffer_take_generation() {
        let config = test_config();
        let metadata = BufferMetadata::new(Hash::default(), 9, config);
        let mut encoded = EncodedBuffer::new(&metadata);

        let hash = Hash::default();

        // Insert k fragments
        for i in 0..6 {
            let fragment = dummy_fragment(hash, i as u16, b"data");
            encoded.insert_fragment(0, i, fragment);
        }

        // Take generation 0
        let job = encoded.extract_job_from_generation(0);
        assert_eq!(job.data_fragments.len(), 6);
        assert_eq!(job.parity_fragments.len(), 3);
        assert_eq!(encoded.dispatched_count, 1);
    }

    // ========================================================================
    // DecodedBuffer Tests
    // ========================================================================

    #[test]
    fn test_decoded_buffer_single_generation() {
        let mut decoded = DecodedBuffer::new(1);

        assert!(!decoded.is_complete());

        let data = vec![1, 2, 3, 4, 5];
        let all_done = decoded.store(0, data.clone());

        assert!(all_done, "Buffer should be complete");
        assert!(decoded.is_complete());
    }

    #[test]
    fn test_decoded_buffer_multiple_generations_partial() {
        let mut decoded = DecodedBuffer::new(3);

        assert!(!decoded.is_complete());

        // Store generation 0
        let all_done = decoded.store(0, vec![1, 2, 3]);
        assert!(!all_done);
        assert_eq!(decoded.completed_count, 1);

        // Store generation 2 (out of order)
        let all_done = decoded.store(2, vec![7, 8, 9]);
        assert!(!all_done);
        assert_eq!(decoded.completed_count, 2);

        // Store generation 1
        let all_done = decoded.store(1, vec![4, 5, 6]);
        assert!(all_done, "All generations should be complete");
    }

    #[test]
    fn test_decoded_buffer_duplicate_store() {
        let mut decoded = DecodedBuffer::new(2);

        decoded.store(0, vec![1, 2, 3]);
        assert_eq!(decoded.completed_count, 1);

        // Store same generation again
        decoded.store(0, vec![9, 9, 9]);
        assert_eq!(decoded.completed_count, 1, "Count should not increase on duplicate");
    }

    #[test]
    fn test_decoded_buffer_reassemble() {
        let mut decoded = DecodedBuffer::new(3);

        decoded.store(0, vec![1, 2]);
        decoded.store(1, vec![3, 4]);
        decoded.store(2, vec![5, 6]);

        let result = decoded.reassemble();
        assert_eq!(result, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_decoded_buffer_reassemble_incomplete_returns_error() {
        let mut decoded = DecodedBuffer::new(3);
        decoded.store(0, vec![1, 2]);
        decoded.store(1, vec![3, 4]);
        // Missing generation 2 – should panic when reassembling
        assert!(panic::catch_unwind(|| decoded.reassemble()).is_err());
    }

    // ========================================================================
    // BlockDecodeState Integration Tests
    // ========================================================================

    #[test]
    fn test_block_decode_state_single_generation() {
        let config = test_config();
        let state = BlockDecodeState::new(Hash::default(), 9, config);

        assert_eq!(state.metadata.total_generations, 1);
        assert_eq!(state.metadata.k, 6);
        assert_eq!(state.metadata.m, 3);
    }

    #[test]
    fn test_block_decode_state_partial_last_generation() {
        let config = test_config();
        let state = BlockDecodeState::new(Hash::default(), 15, config);

        assert_eq!(state.metadata.total_generations, 2);
        assert_eq!(state.metadata.last_gen_k, 4);
        assert_eq!(state.metadata.last_gen_m, 2);
    }

    #[test]
    fn test_block_decode_state_workflow() {
        let config = test_config();
        let mut state = BlockDecodeState::new(Hash::default(), 18, config);

        let hash = Hash::default();

        // Simulate inserting fragments for generation 0
        for i in 0..6 {
            let fragment = dummy_fragment(hash, i as u16, b"gen0");
            let ready = state.encoded.insert_fragment(0, i, fragment);
            if i == 5 {
                assert!(ready);

                // When generation is ready, we'd normally dispatch it
                let job = state.encoded.extract_job_from_generation(0);
                assert_eq!(job.data_fragments.len(), 6);
                assert_eq!(job.parity_fragments.len(), 3);
            }
        }

        // Simulate storing decoded data for generation 0
        let all_done = state.decoded.store(0, vec![1, 2, 3]);
        assert!(!all_done, "Block not complete with only 1 of 2 generations");

        // Insert fragments for generation 1
        for i in 0..6 {
            let fragment = dummy_fragment(hash, (9 + i) as u16, b"gen1");
            let ready = state.encoded.insert_fragment(1, i, fragment);
            if i == 5 {
                assert!(ready);
                let job = state.encoded.extract_job_from_generation(1);
                assert_eq!(job.data_fragments.len(), 6);
                assert_eq!(job.parity_fragments.len(), 3);
            }
        }

        // Store generation 1
        let all_done = state.decoded.store(1, vec![4, 5, 6]);
        assert!(all_done, "Block should be complete");
    }
}
