pub mod perf {
    //!
    //! A module for performance critical constants which depend on consensus parameters.
    //! The constants in this module should all be revisited if mainnet consensus parameters change.
    //!

    /// The default target depth for reachability reindexes.
    pub const DEFAULT_REINDEX_DEPTH: u64 = 100;

    /// The default slack interval used by the reachability
    /// algorithm to encounter for blocks out of the selected chain.
    pub const DEFAULT_REINDEX_SLACK: u64 = 1 << 12;

    #[derive(Clone)]
    pub struct PerfParams {
        //
        // Cache sizes
        //
        /// Preferred cache size for header-related data
        pub header_data_cache_size: u64,

        /// Preferred cache size for block-body-related data which
        /// is typically orders-of magnitude larger than header data
        /// (Note this cannot be set to high due to severe memory consumption)
        pub block_data_cache_size: u64,

        /// Preferred cache size for UTXO-related data
        pub utxo_set_cache_size: u64,

        /// Preferred cache size for block-window-related data
        pub block_window_cache_size: u64,

        //
        // Thread-pools
        //
        /// Defaults to 0 which indicates using system default
        /// which is typically the number of logical CPU cores
        pub block_processors_num_threads: usize,

        /// Defaults to 0 which indicates using system default
        /// which is typically the number of logical CPU cores
        pub virtual_processor_num_threads: usize,
    }

    pub const PERF_PARAMS: PerfParams = PerfParams {
        header_data_cache_size: 10_000,
        block_data_cache_size: 200,
        utxo_set_cache_size: 10_000,
        block_window_cache_size: 2000,
        block_processors_num_threads: 0,
        virtual_processor_num_threads: 0,
    };
}

pub mod store_names {
    pub const VIRTUAL_UTXO_SET: &[u8] = b"virtual-utxo-set";
    pub const PRUNING_UTXO_SET: &[u8] = b"pruning-utxo-set";
}

pub const BLOCK_VERSION: u16 = 1;
pub const TX_VERSION: u16 = 0;
pub const LOCK_TIME_THRESHOLD: u64 = 500_000_000_000;
pub const SOMPI_PER_KASPA: u64 = 100_000_000;
pub const MAX_SOMPI: u64 = 29_000_000_000 * SOMPI_PER_KASPA;

// SEQUENCE_LOCK_TIME_MASK is a mask that extracts the relative lock time
// when masked against the transaction input sequence number.
pub const SEQUENCE_LOCK_TIME_MASK: u64 = 0x00000000ffffffff;

// SEQUENCE_LOCK_TIME_DISABLED is a flag that if set on a transaction
// input's sequence number, the sequence number will not be interpreted
// as a relative lock time.
pub const SEQUENCE_LOCK_TIME_DISABLED: u64 = 1 << 63;
