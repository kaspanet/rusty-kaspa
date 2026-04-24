use bytes::Bytes;
use kaspa_hashes::Hash;
use reed_solomon_simd::ReedSolomonDecoder;

use crate::params::FragmentationConfig;

/// A unit of work sent from Coordinator → Worker via channel.
pub(crate) struct DecodeJob {
    pub hash: Hash,
    pub generation: usize,
    pub k: usize,
    pub m: usize,
    /// Data fragment payloads indexed 0..k (None if missing).
    pub data_fragments: Vec<Option<Bytes>>,
    pub num_of_data_fragments: usize,
    /// Parity fragment payloads indexed 0..m (None if missing).
    pub parity_fragments: Vec<Option<Bytes>>,
}

/// Result sent from Worker → Coordinator via channel.
pub(crate) struct DecodeResult {
    pub hash: Hash,
    pub generation: usize,
    /// The decoded data for this generation (k * payload_size bytes).
    pub data: Vec<u8>,
}

/// Decode a single generation's shards into the original data.
pub(crate) fn decode_generation(
    common_decoder: &mut ReedSolomonDecoder,
    config: FragmentationConfig,
    job: &mut DecodeJob,
) -> Result<Vec<u8>, reed_solomon_simd::Error> {
    let payload_size = config.payload_size;

    // Fast path: if all k data shards are present, just concatenate — no RS decoding needed
    if job.num_of_data_fragments == job.k {
        return Ok(job.data_fragments.iter().flatten().flat_map(|s| s.as_ref().iter().copied()).collect());
    }

    // Slow path: need RS recovery
    if job.k == config.data_blocks && job.m == config.parity_blocks {
        // Reset the pre-allocated decoder (clears added shards, reuses memory)
        common_decoder.reset(job.k, job.m, payload_size)?;
        decode_generation_with_parity(common_decoder, job)
    } else {
        // Rare case: last generation with different k/m, create temporary decoder
        let mut temp_decoder = ReedSolomonDecoder::new(job.k, job.m, payload_size)?;
        decode_generation_with_parity(&mut temp_decoder, job)
    }
}

/// Feed shards into the decoder and recover missing data.
fn decode_generation_with_parity(decoder: &mut ReedSolomonDecoder, job: &mut DecodeJob) -> Result<Vec<u8>, reed_solomon_simd::Error> {
    for (i, opt) in job.data_fragments.iter().enumerate() {
        if let Some(bytes) = opt {
            decoder.add_original_shard(i, bytes.as_ref())?;
        }
    }

    // Add present parity (recovery) fragments
    for (i, opt) in job.parity_fragments.iter().enumerate() {
        if let Some(bytes) = opt {
            decoder.add_recovery_shard(i, bytes.as_ref())?;
        }
    }

    // Decode
    let result = decoder.decode()?;

    // Collect restored shards — must copy before DecoderResult is dropped
    // since DecoderResult borrows from the decoder
    for (i, bytes) in result.restored_original_iter().map(|(i, data)| (i, data.to_vec())) {
        // insert into job data shards accordingly
        job.data_fragments[i] = Some(Bytes::from(bytes));
    }
    drop(result);

    Ok(job.data_fragments.iter().flatten().flatten().cloned().collect())
}
