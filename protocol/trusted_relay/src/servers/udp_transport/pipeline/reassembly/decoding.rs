use crossbeam_channel::{Receiver, Sender};
use kaspa_core::{info, warn};
use reed_solomon_simd::ReedSolomonDecoder;

use crate::servers::udp_transport::pipeline::reassembly::reassembly::WORKER_NAME as REASSEMBLER_WORKER_NAME;
use crate::{
    codec::decoder::{DecodeJob, DecodeResult, decode_generation},
    params::FragmentationConfig,
};
const WORKER_NAME: &str = "decode-worker";

pub type DecodingResultSender = Sender<DecodeResultMessage>;
pub type DecodingResultReceiver = Receiver<DecodeResultMessage>;

pub type DecodingJobSender = Sender<DecodeJobMessage>;
pub type DecodingJobReceiver = Receiver<DecodeJobMessage>;
pub struct DecodeJobMessage(DecodeJob);

impl DecodeJobMessage {
    #[inline(always)]
    pub fn new(job: DecodeJob) -> Self {
        Self(job)
    }

    #[inline(always)]
    pub fn job(self) -> DecodeJob {
        self.0
    }
}

pub struct DecodeResultMessage(DecodeResult);

impl DecodeResultMessage {
    #[inline(always)]
    pub fn new(result: DecodeResult) -> Self {
        Self(result)
    }

    #[inline(always)]
    pub fn result(self) -> DecodeResult {
        self.0
    }
}

// ============================================================================
// DECODE WORKER
// ============================================================================

/// Runs the decode worker loop. Each worker thread calls this function.
/// Workers are stateless — they receive jobs, decode, and return results.
/// Each worker pre-allocates a ReedSolomonDecoder for the common (k, m) case
/// and reuses it across jobs to avoid repeated allocation.
fn run(
    reassembler_idx: usize,
    decoder_idx: usize,
    config: FragmentationConfig,
    job_rx: Receiver<DecodeJobMessage>,
    result_tx: Sender<DecodeResultMessage>,
) {
    info!("{}-{}-{}-{} started", REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx);

    // Pre-allocate decoder for the common k/m (full generations).
    // Last generations with different k/m will create a temporary decoder.
    let mut common_decoder = match ReedSolomonDecoder::new(config.data_blocks, config.parity_blocks, config.payload_size) {
        Ok(d) => d,
        Err(e) => {
            warn!(
                "{}-{}-{}-{}: Failed to create ReedSolomonDecoder: {}",
                REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx, e
            );
            return;
        }
    };

    while let Ok(DecodeJobMessage(mut job)) = job_rx.recv() {
        // Defensive: protect the worker thread from panics inside `decode_generation`.
        // If decoding panics (e.g. due to malformed input or library error), log and continue
        // so other jobs are not lost and the worker thread stays alive.
        let maybe_data =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decode_generation(&mut common_decoder, config, &mut job)));

        let data = match maybe_data {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => {
                warn!(
                    "{}-{}-{}-{}: decoding failed for block {} generation {} — {e}",
                    REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx, job.hash, job.generation
                );
                continue;
            }
            Err(_) => {
                warn!(
                    "{}-{}-{}-{}: decoding panicked for block {} generation {} — skipping",
                    REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx, job.hash, job.generation
                );
                continue;
            }
        };

        if result_tx.send(DecodeResultMessage::new(DecodeResult { hash: job.hash, generation: job.generation, data })).is_err() {
            // Result channel closed — coordinator is gone, exit worker
            break;
        }
    }
}

pub fn spawn_decode_worker(
    reassembler_idx: usize,
    decoder_idx: usize,
    config: FragmentationConfig,
    job_rx: Receiver<DecodeJobMessage>,
    result_tx: Sender<DecodeResultMessage>,
) -> std::thread::JoinHandle<()> {
    let handle = std::thread::Builder::new()
        .name(format!("{}-{}-{}-{}", REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx))
        .spawn(move || run(reassembler_idx, decoder_idx, config, job_rx, result_tx))
        .expect(
            format!("Failed to spawn {}-{}-{}-{} thread", REASSEMBLER_WORKER_NAME, reassembler_idx, WORKER_NAME, decoder_idx).as_str(),
        );
    handle
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use kaspa_hashes::Hash;

    fn make_payload(value: u8, size: usize) -> Vec<u8> {
        vec![value; size]
    }

    fn make_job(hash: Hash, k: usize, m: usize, payload_size: usize) -> DecodeJob {
        DecodeJob {
            hash,
            generation: 0,
            k,
            m,
            data_fragments: vec![None; k],
            num_of_data_fragments: 0,
            parity_fragments: vec![None; m],
        }
    }

    // ========================================================================
    // Fast Path Tests (all data shards present, no RS decoding needed)
    // ========================================================================

    #[test]
    fn test_decode_generation_fast_path_all_data_present() {
        let k = 4;
        let m = 2;
        let payload_size = 100;
        let hash = Hash::default();

        let mut job = make_job(hash, k, m, payload_size);
        let config = FragmentationConfig::new(k, m, payload_size);

        // Fill all k data fragments
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 1, payload_size)));
            job.num_of_data_fragments += 1;
        }

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        // Result should be concatenation of all data shards
        assert_eq!(result.len(), k * payload_size);
        for i in 0..k {
            let start = i * payload_size;
            let end = start + payload_size;
            assert_eq!(&result[start..end], &make_payload((i as u8) + 1, payload_size)[..]);
        }
    }

    #[test]
    fn test_decode_generation_fast_path_empty_data() {
        let k = 3;
        let m = 1;
        let payload_size = 50;
        let hash = Hash::default();

        let mut job = make_job(hash, k, m, payload_size);
        let config = FragmentationConfig::new(k, m, payload_size);

        // Fill all k data fragments with empty bytes
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::new());
            job.num_of_data_fragments += 1;
        }

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        assert_eq!(result.len(), 0);
    }

    // ========================================================================
    // Slow Path Tests (RS recovery with missing data shards)
    // ========================================================================

    #[test]
    fn test_decode_generation_slow_path_missing_one_data_shard() {
        let k = 4;
        let m = 2;
        let payload_size = 100;
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);

        // Create a set of fragments first to understand their content
        let mut job = make_job(hash, k, m, payload_size);
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 1, payload_size)));
            job.num_of_data_fragments += 1;
        }

        // Encode to get parity fragments
        let mut encoder = reed_solomon_simd::ReedSolomonEncoder::new(k, m, payload_size).unwrap();
        for s in job.data_fragments.iter().flatten() {
            encoder.add_original_shard(s.as_ref()).unwrap();
        }
        let encode_result = encoder.encode().unwrap();
        for (i, recovery_data) in encode_result.recovery_iter().enumerate() {
            job.parity_fragments[i] = Some(Bytes::copy_from_slice(recovery_data));
        }

        // Now remove one data fragment (index 1)
        let removed_fragment = job.data_fragments[1].take();
        assert!(removed_fragment.is_some());
        job.num_of_data_fragments -= 1; // reflect removal

        // Decode should recover the missing fragment
        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        // Should have recovered all k shards
        assert_eq!(result.len(), k * payload_size);
        // Check that shard 1 was properly reconstructed
        let start = payload_size;
        let end = start + payload_size;
        assert_eq!(&result[start..end], &make_payload(2, payload_size)[..]);
    }

    #[test]
    fn test_decode_generation_slow_path_only_parity_fragments() {
        let k = 3;
        let m = k;
        let payload_size = 50;
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);

        // Create original data fragments
        let mut job = make_job(hash, k, m, payload_size);
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 10, payload_size)));
            job.num_of_data_fragments += 1;
        }

        // Encode to get parity
        let mut encoder = reed_solomon_simd::ReedSolomonEncoder::new(k, m, payload_size).unwrap();
        for s in job.data_fragments.iter().flatten() {
            encoder.add_original_shard(s.as_ref()).unwrap();
        }
        let encode_result = encoder.encode().unwrap();
        for (i, recovery_data) in encode_result.recovery_iter().enumerate() {
            job.parity_fragments[i] = Some(Bytes::copy_from_slice(recovery_data));
        }

        // Remove ALL data fragments
        for i in 0..k {
            job.data_fragments[i] = None;
        }
        job.num_of_data_fragments = 0;
        // Decode should still work: k parity fragments can recover k data fragments
        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        // Should have recovered all k fragments
        assert_eq!(result.len(), k * payload_size);
        // Verify each recovered fragment
        for i in 0..k {
            let start = i * payload_size;
            let end = start + payload_size;
            assert_eq!(
                &result[start..end],
                &make_payload((i as u8) + 10, payload_size)[..],
                "Fragment {} was not correctly recovered",
                i
            );
        }
    }

    // ========================================================================
    // Last Generation Tests (different k/m)
    // ========================================================================

    #[test]
    fn test_decode_generation_last_gen_different_k_m() {
        let common_k = 6;
        let common_m = 3;
        let last_k = 4;
        let last_m = 2;
        let payload_size = 100;
        let hash = Hash::default();

        let config = FragmentationConfig::new(last_k, last_m, payload_size);
        let mut job = make_job(hash, last_k, last_m, payload_size);

        // Fill all k data fragments for last generation
        for i in 0..last_k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 1, payload_size)));
            job.num_of_data_fragments += 1;
        }

        // Encode to get parity fragments for last gen
        let mut encoder = reed_solomon_simd::ReedSolomonEncoder::new(last_k, last_m, payload_size).unwrap();
        for s in job.data_fragments.iter().flatten() {
            encoder.add_original_shard(s.as_ref()).unwrap();
        }
        let encode_result = encoder.encode().unwrap();
        for (i, recovery_data) in encode_result.recovery_iter().enumerate() {
            job.parity_fragments[i] = Some(Bytes::copy_from_slice(recovery_data));
        }

        // Remove one data fragment
        job.data_fragments[2] = None;
        job.num_of_data_fragments -= 1;
        // Decode with different k/m (common_decoder is for common_k/m)
        let mut common_decoder = ReedSolomonDecoder::new(common_k, common_m, payload_size).unwrap();
        let result = decode_generation(&mut common_decoder, config, &mut job).unwrap();

        // Should use temporary decoder for last_gen and recover successfully
        assert_eq!(result.len(), last_k * payload_size);
        let start = 2 * payload_size;
        let end = start + payload_size;
        assert_eq!(&result[start..end], &make_payload(3, payload_size)[..]);
    }

    // ========================================================================
    // Edge Cases
    // ========================================================================

    #[test]
    fn test_decode_generation_k1_m1() {
        // Minimal case: k=1, m=1
        let k = 1;
        let m = 1;
        let payload_size = 64;
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);
        let mut job = make_job(hash, k, m, payload_size);
        job.data_fragments[0] = Some(Bytes::from(make_payload(42, payload_size)));

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        assert_eq!(result.len(), payload_size);
        assert_eq!(result, make_payload(42, payload_size));
    }

    #[test]
    fn test_decode_generation_large_payload() {
        let k = 3;
        let m = 1;
        let payload_size = 1_000_000; // 1MB
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);
        let mut job = make_job(hash, k, m, payload_size);
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 1, payload_size)));
        }

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        assert_eq!(result.len(), k * payload_size);
    }

    #[test]
    fn test_decode_with_missing_unrecoverable_shard_returns_error() {
        let k = 3;
        let m = 1;
        let payload_size = 100;
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);
        let mut job = make_job(hash, k, m, payload_size);

        // Only provide 2 data fragments (less than k), no parity
        job.data_fragments[0] = Some(Bytes::from(make_payload(1, payload_size)));
        job.data_fragments[1] = Some(Bytes::from(make_payload(2, payload_size)));
        // Fragment 2 is missing, and no parity to recover it

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job);
        assert!(result.is_err(), "Should return error when not enough fragments are available");
    }

    #[test]
    fn test_decode_with_multiple_missing_but_recoverable() {
        let k = 4;
        let m = 3;
        let payload_size = 100;
        let hash = Hash::default();

        let config = FragmentationConfig::new(k, m, payload_size);
        let mut job = make_job(hash, k, m, payload_size);

        // Fill original data fragments
        for i in 0..k {
            job.data_fragments[i] = Some(Bytes::from(make_payload((i as u8) + 1, payload_size)));
        }

        // Encode to get parity
        let mut encoder = reed_solomon_simd::ReedSolomonEncoder::new(k, m, payload_size).unwrap();
        for s in job.data_fragments.iter().flatten() {
            encoder.add_original_shard(s.as_ref()).unwrap();
        }
        let encode_result = encoder.encode().unwrap();
        for (i, recovery_data) in encode_result.recovery_iter().enumerate() {
            job.parity_fragments[i] = Some(Bytes::copy_from_slice(recovery_data));
        }

        // Remove 2 data fragments (less than m parity)
        job.data_fragments[1] = None;
        job.data_fragments[3] = None;

        let decoder = &mut ReedSolomonDecoder::new(k, m, payload_size).unwrap();
        let result = decode_generation(decoder, config, &mut job).unwrap();

        // Should recover both missing fragments
        assert_eq!(result.len(), k * payload_size);
        assert_eq!(&result[payload_size..2 * payload_size], &make_payload(2, payload_size)[..]);
        assert_eq!(&result[3 * payload_size..4 * payload_size], &make_payload(4, payload_size)[..]);
    }
}
