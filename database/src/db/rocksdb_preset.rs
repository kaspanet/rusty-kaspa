//! RocksDB configuration presets for different use cases
//!
//! This module provides pre-configured RocksDB option sets optimized for different
//! deployment scenarios.

use rocksdb::Options;
use std::str::FromStr;

/// Available RocksDB configuration presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RocksDbPreset {
    /// Default configuration - balanced for general use on SSD/NVMe
    /// - 64MB write buffer
    /// - Standard compression
    /// - Optimized for fast storage
    #[default]
    Default,

    /// HDD configuration - optimized for hard disk drives
    /// - 256MB write buffer (4x default)
    /// - Aggressive compression (LZ4 + ZSTD)
    /// - BlobDB enabled for large values
    /// - Rate limiting to prevent I/O spikes
    /// - Optimized for sequential writes and reduced write amplification
    ///
    /// Recommended for archival nodes on HDD storage.
    Hdd,
}

impl FromStr for RocksDbPreset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "hdd" => Ok(Self::Hdd),
            _ => Err(format!("Unknown RocksDB preset: '{}'. Valid options: default, hdd", s)),
        }
    }
}

impl std::fmt::Display for RocksDbPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Default => write!(f, "default"),
            Self::Hdd => write!(f, "hdd"),
        }
    }
}

impl RocksDbPreset {
    /// Apply the preset configuration to RocksDB options
    ///
    /// # Arguments
    /// * `opts` - RocksDB options to configure
    /// * `parallelism` - Number of background threads
    /// * `mem_budget` - Memory budget (only used for Default preset, HDD uses fixed 256MB)
    pub fn apply_to_options(&self, opts: &mut Options, parallelism: usize, mem_budget: usize, cache_budget: Option<usize>) {
        match self {
            Self::Default => self.apply_default(opts, parallelism, mem_budget),
            Self::Hdd => self.apply_hdd(opts, parallelism, cache_budget),
        }
    }

    /// Apply default preset configuration
    fn apply_default(&self, opts: &mut Options, parallelism: usize, mem_budget: usize) {
        if parallelism > 1 {
            opts.increase_parallelism(parallelism as i32);
        }

        // Use the provided memory budget (typically 64MB)
        opts.optimize_level_style_compaction(mem_budget);
    }

    /// Apply HDD preset configuration (HDD-optimized settings)
    fn apply_hdd(&self, opts: &mut Options, parallelism: usize, cache_budget: Option<usize>) {
        if parallelism > 1 {
            opts.increase_parallelism(parallelism as i32);
        }

        // Memory and write buffer settings (256MB for better batching on HDD)
        let write_buffer_size = 256 * 1024 * 1024; // 256MB

        // Optimize for level-style compaction with archive-appropriate memory
        // This sets up LSM tree parameters
        opts.optimize_level_style_compaction(write_buffer_size);

        // Re-set write_buffer_size after optimize_level_style_compaction()
        // because optimize_level_style_compaction() internally overrides it to size/4
        opts.set_write_buffer_size(write_buffer_size);

        // LSM Tree Structure - Optimized for large (4TB+) archives
        // 256 MB SST files reduce file count dramatically (500K → 16K files for 4TB)
        opts.set_target_file_size_base(256 * 1024 * 1024); // 256 MB SST files
        opts.set_target_file_size_multiplier(1); // Same size across all levels
        opts.set_max_bytes_for_level_base(1024 * 1024 * 1024); // 1 GB L1 base
        opts.set_level_compaction_dynamic_level_bytes(true); // Minimize space amplification

        // Compaction settings
        // Trigger compaction when L0 has just 1 file (minimize write amplification)
        opts.set_level_zero_file_num_compaction_trigger(1);

        // Prioritize compacting older/smaller files first
        use rocksdb::CompactionPri;
        opts.set_compaction_pri(CompactionPri::OldestSmallestSeqFirst);

        // Read-ahead for compactions (4MB - good for sequential HDD reads)
        opts.set_compaction_readahead_size(4 * 1024 * 1024);

        // Compression strategy: LZ4 for all levels, ZSTD for bottommost
        use rocksdb::DBCompressionType;

        // Set default compression to LZ4 (fast)
        opts.set_compression_type(DBCompressionType::Lz4);

        // Enable bottommost level compression with maximum ZSTD level
        opts.set_bottommost_compression_type(DBCompressionType::Zstd);

        // ZSTD compression options for bottommost level
        // Larger dictionaries (64 KB) improve compression on large archives
        opts.set_compression_options(
            -1,        // window_bits (let ZSTD choose optimal)
            22,        // level (maximum compression)
            0,         // strategy (default)
            64 * 1024, // dict_bytes (64 KB dictionary)
        );

        // Train ZSTD dictionaries on 8 MB of sample data (~125x dictionary size)
        opts.set_zstd_max_train_bytes(8 * 1024 * 1024);

        // Block-based table options for better caching
        use rocksdb::{BlockBasedOptions, Cache};
        let mut block_opts = BlockBasedOptions::default();

        // Partitioned Bloom filters (18 bits per key for better false-positive rate)
        block_opts.set_bloom_filter(18.0, false); // 18 bits per key
        block_opts.set_partition_filters(true); // Partition for large databases
        block_opts.set_format_version(5); // Latest format with optimizations
        block_opts.set_index_type(rocksdb::BlockBasedIndexType::TwoLevelIndexSearch);

        // Cache index and filter blocks in block cache for faster queries
        block_opts.set_cache_index_and_filter_blocks(true);

        // Block cache: Default 256MB (safe for low-RAM systems)
        // Can be scaled via ram-scale or overridden via --rocksdb-cache-size
        let default_cache_size = 256 * 1024 * 1024; // 256MB
        let cache_size = cache_budget.unwrap_or(default_cache_size);
        let cache = Cache::new_lru_cache(cache_size);
        block_opts.set_block_cache(&cache);

        // Set block size (256KB - better for sequential HDD reads)
        block_opts.set_block_size(256 * 1024);

        opts.set_block_based_table_factory(&block_opts);

        // Rate limiting: prevent I/O spikes on HDD
        // 12 MB/s rate limit for background writes
        opts.set_ratelimiter(12 * 1024 * 1024, 100_000, 10);

        // Enable BlobDB for large values (reduces write amplification)
        opts.set_enable_blob_files(true);
        opts.set_min_blob_size(512); // Only values >512 bytes go to blob files
        opts.set_blob_file_size(256 * 1024 * 1024); // 256MB blob files
        opts.set_blob_compression_type(DBCompressionType::Zstd); // Compress blobs
        opts.set_enable_blob_gc(true); // Enable garbage collection
        opts.set_blob_gc_age_cutoff(0.9); // GC blobs when 90% old
        opts.set_blob_gc_force_threshold(0.1); // Force GC at 10% garbage
        opts.set_blob_compaction_readahead_size(8 * 1024 * 1024); // 8 MB blob readahead
    }

    /// Get a human-readable description of the preset
    pub fn description(&self) -> &'static str {
        match self {
            Self::Default => "Default preset - balanced for SSD/NVMe (64MB write buffer, standard compression)",
            Self::Hdd => {
                "HDD preset - optimized for hard disk drives (256MB write buffer, BlobDB, aggressive compression, rate limiting)"
            }
        }
    }

    /// Get the recommended use case for this preset
    pub fn use_case(&self) -> &'static str {
        match self {
            Self::Default => "General purpose nodes on SSD/NVMe storage",
            Self::Hdd => "Archival nodes on HDD storage (--archival flag recommended)",
        }
    }

    /// Get memory requirements for this preset
    pub fn memory_requirements(&self) -> &'static str {
        match self {
            Self::Default => "~4GB minimum, scales with --ram-scale",
            Self::Hdd => "~4GB minimum (256MB write buffer + 256MB cache + overhead), 8GB+ recommended for public RPC",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preset_from_str() {
        assert_eq!(RocksDbPreset::from_str("default").unwrap(), RocksDbPreset::Default);
        assert_eq!(RocksDbPreset::from_str("Default").unwrap(), RocksDbPreset::Default);
        assert_eq!(RocksDbPreset::from_str("hdd").unwrap(), RocksDbPreset::Hdd);
        assert_eq!(RocksDbPreset::from_str("HDD").unwrap(), RocksDbPreset::Hdd);
        assert!(RocksDbPreset::from_str("unknown").is_err());
    }
}
