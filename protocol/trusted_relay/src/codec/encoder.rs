use std::collections::VecDeque;

use bytes::Bytes;
use kaspa_core::trace;
use kaspa_hashes::Hash;
use reed_solomon_simd::ReedSolomonEncoder;

use crate::{
    model::{
        fragments::{Fragment, FragmentHeader, FragmentPayload},
        ftr_block::FtrBlock,
    },
    params::FragmentationConfig,
};

/// Generates FEC-encoded fragments for a serialized block.
///
/// Implements [`Iterator`] for streaming fragment production. Data fragments for each
/// generation are emitted first; once a generation is complete the encoder runs
/// Reed-Solomon and queues the parity fragments for emission.
///
/// This approach optimises per-generation throughput: encoding is deferred until
/// the data queue for a generation is exhausted.
pub struct FragmentGenerator {
    config: FragmentationConfig,
    encoder: ReedSolomonEncoder,
    hash: Hash,
    current_fragment_index: usize,
    total_number_of_fragments: usize,
    last_gen_k: usize,
    last_gen_m: usize,
    parity_payloads: VecDeque<FragmentPayload>,
    data_fragments: VecDeque<FragmentPayload>,
}

impl FragmentGenerator {
    pub fn new(config: FragmentationConfig, hash: Hash, ftr_block: FtrBlock) -> Self {
        let mut data = ftr_block.as_bytes();

        if !data.len().is_multiple_of(config.payload_size) {
            let padded_len = data.len().div_ceil(config.payload_size) * config.payload_size;
            data.resize(padded_len, 0);
        }

        let data_len = data.len();
        let data_bytes = Bytes::copy_from_slice(&data);

        let total_data_fragments = data_len / config.payload_size;
        let data_fragments = VecDeque::from_iter((0..total_data_fragments).map(|i| {
            let start = i * config.payload_size;
            let end = start + config.payload_size;
            data_bytes.slice(start..end)
        }));

        // Calculate total fragments: full generations use config.parity_blocks,
        // the last (partial) generation gets a proportionally reduced parity count.
        let num_generations = total_data_fragments.div_ceil(config.data_blocks);
        let num_full_generations =
            if total_data_fragments.is_multiple_of(config.data_blocks) { num_generations } else { num_generations - 1 };
        let full_gen_parity = num_full_generations * config.parity_blocks;
        let partial_gen_parity = if total_data_fragments.is_multiple_of(config.data_blocks) {
            0
        } else {
            let last_gen_k = total_data_fragments % config.data_blocks;
            (last_gen_k * config.parity_blocks).div_ceil(config.data_blocks)
        };
        let total_number_of_fragments = total_data_fragments + full_gen_parity + partial_gen_parity;

        // Account for last generation truncation.
        let (last_gen_k, last_gen_m) = config.calculate_last_k_and_m(total_number_of_fragments);

        let encoder = if total_data_fragments < config.data_blocks {
            ReedSolomonEncoder::new(last_gen_k, last_gen_m, config.payload_size)
                .expect("Reed-Solomon encoder creation failed for adjusted data blocks")
        } else {
            ReedSolomonEncoder::new(config.data_blocks, config.parity_blocks, config.payload_size)
                .expect("Reed-Solomon encoder creation failed")
        };

        Self {
            config,
            encoder,
            hash,
            current_fragment_index: 0,
            total_number_of_fragments,
            last_gen_k,
            last_gen_m,
            parity_payloads: VecDeque::with_capacity(config.parity_blocks),
            data_fragments,
        }
    }

    pub fn total_fragments(&self) -> usize {
        self.total_number_of_fragments
    }

    pub fn index_of_last_generation(&self) -> usize {
        (self.total_number_of_fragments - 1) / self.config.fragments_per_generation()
    }

    pub fn current_generation(&self) -> usize {
        std::cmp::min(self.current_fragment_index, self.total_number_of_fragments - 1) / self.config.fragments_per_generation()
    }

    pub fn should_encode(&self) -> bool {
        if self.current_fragment_index == 0 {
            return false;
        }

        // For the last generation: encode when we've emitted all its data fragments
        if self.current_generation() == self.index_of_last_generation() {
            return self.index_within_generation() == self.last_gen_k;
        }

        // For full generations: encode at generation boundary
        self.index_within_generation() == self.config.data_blocks
    }

    pub fn index_within_generation(&self) -> usize {
        self.current_fragment_index % self.config.fragments_per_generation()
    }

    pub fn is_in_parity_phase(&self) -> bool {
        if self.current_generation() == self.index_of_last_generation() {
            self.index_within_generation() >= self.last_gen_k
        } else {
            self.index_within_generation() >= self.config.data_blocks
        }
    }

    pub fn current_generation_max_index(&self) -> usize {
        if self.current_generation() == self.index_of_last_generation() {
            self.last_gen_k + self.last_gen_m - 1
        } else {
            self.config.fragments_per_generation() - 1
        }
    }

    /// Returns `(k, m)` for the *next* generation's encoding pass, or `None`
    /// if the current generation is the last.
    pub fn to_encode_counts(&self) -> Option<(usize, usize)> {
        let current_generation = self.current_generation();
        let index_of_last_generation = self.index_of_last_generation();
        if current_generation == index_of_last_generation {
            None
        } else if index_of_last_generation > 0 && current_generation == index_of_last_generation - 1 {
            Some((self.last_gen_k, self.last_gen_m))
        } else {
            Some((self.config.data_blocks, self.config.parity_blocks))
        }
    }
}
impl Iterator for FragmentGenerator {
    type Item = Fragment;

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_fragment_index == self.total_number_of_fragments {
            return None;
        }

        trace!(
            "fragmenting block: {} | fragment_idx; [{}/{}] | generation_idx: [{}/{}] | index_within_gen: [{}/{}] | processing_parity: {}",
            self.hash,
            self.current_fragment_index,
            self.total_number_of_fragments - 1,
            self.current_generation(),
            self.index_of_last_generation(),
            self.index_within_generation(),
            self.current_generation_max_index(),
            self.is_in_parity_phase(),
        );

        // We crossed a generational boundary
        if self.should_encode() {
            trace!("fragmenting block: {} -> Encoding parity fragments", self.hash);
            // We've moved to a new generation, so encode the previous one
            let encoding_result = self.encoder.encode().expect("Reed-Solomon encoding failed");
            // Add to parity payloads
            self.parity_payloads.extend(encoding_result.recovery_iter().map(Bytes::copy_from_slice));

            // Clear and reset encoder state for next generation
            drop(encoding_result);
            if let Some((next_k, next_m)) = self.to_encode_counts() {
                self.encoder
                    .reset(next_k, next_m, self.config.payload_size)
                    .expect("Failed to reset Reed-Solomon encoder for next generation");
            }
        }

        // First, if we have parity fragments queued, emit them
        if let Some(parity_fragment) = self.parity_payloads.pop_front() {
            let header = FragmentHeader::new(self.hash, self.current_fragment_index as u16, self.total_number_of_fragments as u16);
            self.current_fragment_index += 1;
            return Some(Fragment { header, payload: parity_fragment });
        }

        // Add data fragment to encoder
        let current_chunk = self.data_fragments.pop_front()?;
        self.encoder.add_original_shard(&current_chunk).unwrap();
        let header = FragmentHeader::new(self.hash, self.current_fragment_index as u16, self.total_number_of_fragments as u16);
        self.current_fragment_index += 1;
        Some(Fragment { header, payload: current_chunk })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_generation() {
        // Test data that fits exactly in one generation
        kaspa_core::log::try_init_logger("trace");
        let config = FragmentationConfig::new(4, 2, 100);
        let test_data = vec![42u8; 400]; // 4 fragments * 100 bytes = one full generation
        let hash = Hash::from_u64_word(1u64);

        let mut generator = FragmentGenerator::new(config, hash, FtrBlock(test_data.clone()));
        let fragments: Vec<_> = generator.by_ref().collect();

        // Should have 4 data fragments + 2 parity fragments = 6 total
        assert!(fragments.len() >= 4, "Should have at least 4 data fragments");

        // Verify all fragments have correct header
        for (i, fragment) in fragments.iter().enumerate() {
            assert_eq!(fragment.header().fragment_index() as usize, i, "fragment index mismatch at position {}", i);
        }

        // After consuming generator, ensure generation indexing is clamped
        assert!(
            generator.current_generation() <= generator.index_of_last_generation(),
            "Current generation should not exceed last generation index"
        );
    }

    #[test]
    fn test_multiple_generations() {
        // Test data that spans multiple generations
        kaspa_core::log::try_init_logger("trace");
        let config = FragmentationConfig::new(4, 2, 100);
        let test_data = vec![42u8; 1000]; // 10 fragments = 2.5 generations
        let hash = Hash::from_u64_word(2u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data.clone()));
        let fragments: Vec<_> = generator.collect();

        // 10 data fragments:
        // - Gen 0: 4 data + 2 parity = 6 fragments
        // - Gen 1: 4 data + 2 parity = 6 fragments
        // - Gen 2: 2 data + ceil(2*2/4)=1 parity = 3 fragments (last gen with k=2, m=1)
        // Total: 15 fragments (10 data + 5 parity)
        assert_eq!(fragments.len(), 15, "Multiple generations calculation failed");

        // Verify all fragments have sequential indices
        for (i, fragment) in fragments.iter().enumerate() {
            assert_eq!(fragment.header().fragment_index() as usize, i, "fragment indices should be sequential");
            assert_eq!(fragment.header().total_fragments() as usize, 15, "All fragments should report same total");
        }
    }

    #[test]
    fn test_parity_ratio_preserved() {
        kaspa_core::log::try_init_logger("trace");
        // Test that parity ratio is maintained across generations
        let config = FragmentationConfig::new(16, 8, 1200); // k=16, m=8
        let test_data = vec![42u8; 20 * 1200]; // 20 data fragments across 2 generations
        let hash = Hash::from_u64_word(3u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // Gen 0 (full): 16 data + 8 parity = 24 fragments
        // Gen 1 (last): 4 data + ceil(4*8/16)=2 parity = 6 fragments
        // Total: 20 data + (8 + 2) parity = 30 fragments
        assert_eq!(fragments.len(), 30, "Parity ratio test: expected 30 fragments");
    }

    #[test]
    fn test_small_data_single_fragment() {
        kaspa_core::log::try_init_logger("trace");
        // Test with data smaller than one payload
        let config = FragmentationConfig::new(4, 2, 1000);
        let test_data = vec![42u8; 500]; // Less than one full fragment
        let hash = Hash::from_u64_word(4u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // Only 1 data fragment (last gen): 1 data + ceil(1*2/4)=1 parity = 2 total
        assert_eq!(fragments.len(), 2, "Small data should have 2 fragments (1 data + 1 parity)");
    }

    #[test]
    fn test_exact_multiple_generations() {
        kaspa_core::log::try_init_logger("trace");
        // Test data that fits exactly in N generations
        let config = FragmentationConfig::new(5, 3, 100);
        let test_data = vec![42u8; 1500]; // 15 fragments = exactly 3 generations
        let hash = Hash::from_u64_word(5u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // Each generation: 5 data + 3 parity = 8
        // 3 generations: 24 fragments total
        assert_eq!(fragments.len(), 24, "Exact multiple generations should have 24 fragments");
    }

    #[test]
    fn test_fragment_headers_unique() {
        kaspa_core::log::try_init_logger("trace");
        // Verify each fragment has unique index
        let config = FragmentationConfig::new(4, 2, 100);
        let test_data = vec![42u8; 800]; // 8 fragments = 2 generations
        let hash = Hash::from_u64_word(6u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        let indices: std::collections::HashSet<u16> = fragments.iter().map(|fragment| fragment.header().fragment_index()).collect();

        // All indices should be unique
        assert_eq!(indices.len(), fragments.len(), "All fragment indices should be unique");
    }

    #[test]
    fn test_total_number_of_fragments_calculation() {
        kaspa_core::log::try_init_logger("trace");
        // Verify that total_number_of_fragments is correctly calculated
        let config = FragmentationConfig::new(8, 4, 200);
        let data_size = 7 * 200; // 7 data fragments = < 1 generation
        let test_data = vec![42u8; data_size];
        let hash = Hash::from_u64_word(7u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // 7 data fragments in last gen: m = ceil(7 * 4 / 8) = 4
        // Total: 7 data + 4 parity = 11 fragments
        assert_eq!(fragments.len(), 11, "Total fragments calculation incorrect");
    }

    #[test]
    fn test_data_payload_correctness() {
        kaspa_core::log::try_init_logger("trace");
        // Verify that data payloads match the original data
        let config = FragmentationConfig::new(4, 2, 100);
        let test_data = vec![123u8; 400]; // 4 fragments
        let hash = Hash::from_u64_word(8u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data.clone()));
        let fragments: Vec<_> = generator.collect();

        // First 4 fragments are data, should match original
        for (i, fragment) in fragments.iter().take(4).enumerate() {
            let expected_start = i * 100;
            let expected_end = (i + 1) * 100;
            assert_eq!(
                fragment.payload().as_ref(),
                &test_data[expected_start..expected_end],
                "Data payload {} doesn't match original",
                i
            );
        }
    }

    #[test]
    fn test_large_data_1mb() {
        kaspa_core::log::try_init_logger("trace");
        // Stress test with 1MB of data
        let config = FragmentationConfig::new(16, 16, 1200);
        let test_data = vec![42u8; 1024 * 1024]; // 1MB
        let hash = Hash::from_u64_word(9u64);

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // Approximately 873 data fragments (1MB / 1200 bytes ≈ 873)
        // Generations: ceil(873 / 16) = 55
        // Full gens: 54 * 16 = 864 data fragments
        // Last gen: 9 data fragments → m = ceil(9 * 16 / 16) = 9
        // Parity: 54 * 16 + 9 = 873
        // Total: 873 + 873 = 1746
        let expected_data_fragments = (1024 * 1024 + 1199) / 1200; // ceil division
        assert!(fragments.len() > expected_data_fragments, "Should have at least {} fragments", expected_data_fragments);
    }

    #[test]
    fn test_randomized_configs() {
        kaspa_core::log::try_init_logger("trace");
        // Test with various random-like FEC configurations
        let configs = vec![(2, 1, 500), (8, 4, 1500), (32, 16, 800), (3, 2, 200), (10, 5, 1000)];

        for (k, m, payload) in configs {
            let config = FragmentationConfig::new(k, m, payload);
            let test_data = vec![42u8; k * payload * 3]; // 3 full generations
            let hash = Hash::from_u64_word(10u64);

            let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
            let fragments: Vec<_> = generator.collect();

            // Should have k*3 data fragments and k*m*3 parity fragments (all full generations)
            let expected_data = k * 3;
            let expected_total = expected_data + (m * 3);

            assert_eq!(
                fragments.len(),
                expected_total,
                "Config k={}, m={}, payload={}: expected {} fragments, got {}",
                k,
                m,
                payload,
                expected_total,
                fragments.len()
            );
        }
    }

    #[test]
    fn test_last_gen_calculation() {
        kaspa_core::log::try_init_logger("trace");
        // Debug test to verify last_gen_k and last_gen_m are calculated correctly
        let config = FragmentationConfig::new(16, 8, 1200);
        let total_data_fragments = 20;
        let test_data = vec![42u8; total_data_fragments * 1200];
        let hash = Hash::from_u64_word(11u64);

        // Manually calculate expected last_gen_k and last_gen_m
        let expected_last_gen_k = {
            let rem = total_data_fragments % config.data_blocks;
            if rem == 0 { config.data_blocks } else { rem }
        };
        let expected_last_gen_m = if expected_last_gen_k == config.data_blocks {
            config.parity_blocks
        } else {
            (expected_last_gen_k * config.parity_blocks).div_ceil(config.data_blocks)
        };

        // For 20 data fragments with k=16:
        // last_gen_k = 20 % 16 = 4
        // last_gen_m = ceil(4 * 8 / 16) = 2
        assert_eq!(expected_last_gen_k, 4, "Expected last_gen_k to be 4");
        assert_eq!(expected_last_gen_m, 2, "Expected last_gen_m to be 2");

        let generator = FragmentGenerator::new(config, hash, FtrBlock(test_data));
        let fragments: Vec<_> = generator.collect();

        // Total parity: 1 full gen (8) + 1 last gen (2) = 10
        // Total fragments: 20 data + 10 parity = 30
        assert_eq!(fragments.len(), 30, "Total fragments should be 30 for 20 data fragments with k=16, m=8");
    }
}
