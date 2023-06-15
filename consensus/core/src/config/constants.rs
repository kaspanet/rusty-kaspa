pub mod consensus {
    //!
    //! A module for constants which directly impact consensus.
    //!

    use crate::KType;
    use kaspa_math::Uint256;

    //
    // ~~~~~~~~~~~~~~~~~~~~~~~~~ Network & Ghostdag ~~~~~~~~~~~~~~~~~~~~~~~~~
    //

    /// Estimated upper bound on network delay in seconds
    pub const NETWORK_DELAY_BOUND: u64 = 5;

    /// Estimation of the average network delay in seconds
    pub const NETWORK_DELAY_AVERAGE: u64 = 2;

    /// **Desired** upper bound on the probability of anticones larger than k
    pub const GHOSTDAG_TAIL_DELTA: f64 = 0.01;

    /// **Legacy** default K for 1 BPS
    pub const LEGACY_DEFAULT_GHOSTDAG_K: KType = 18;

    //
    // ~~~~~~~~~~~~~~~~~~ Timestamp deviation & Median time ~~~~~~~~~~~~~~~~~~
    //

    /// **Legacy** timestamp deviation tolerance (seconds)
    pub const LEGACY_TIMESTAMP_DEVIATION_TOLERANCE: u64 = 132;

    /// **New** timestamp deviation tolerance (seconds).
    /// KIP-0004: 605 (~10 minutes)
    pub const NEW_TIMESTAMP_DEVIATION_TOLERANCE: u64 = 605;

    /// The desired interval between samples of the median time window (seconds).
    /// KIP-0004: 10 seconds
    pub const PAST_MEDIAN_TIME_SAMPLE_INTERVAL: u64 = 10;

    /// Size of the **sampled** median time window (independent of BPS)
    pub const MEDIAN_TIME_SAMPLED_WINDOW_SIZE: u64 =
        ((2 * NEW_TIMESTAMP_DEVIATION_TOLERANCE - 1) + PAST_MEDIAN_TIME_SAMPLE_INTERVAL - 1) / PAST_MEDIAN_TIME_SAMPLE_INTERVAL;

    //
    // ~~~~~~~~~~~~~~~~~~~~~~~~~ Max difficulty target ~~~~~~~~~~~~~~~~~~~~~~~~~
    //

    /// Highest proof of work difficulty target a Kaspa block can have for all networks.
    /// This value is: 2^255 - 1.
    ///
    /// Computed value: `Uint256::from_u64(1).wrapping_shl(255) - 1.into()`
    pub const MAX_DIFFICULTY_TARGET: Uint256 =
        Uint256([18446744073709551615, 18446744073709551615, 18446744073709551615, 9223372036854775807]);

    /// Highest proof of work difficulty target as a floating number
    pub const MAX_DIFFICULTY_TARGET_AS_F64: f64 = 5.78960446186581e76;

    //
    // ~~~~~~~~~~~~~~~~~~~ Difficulty Adjustment Algorithm (DAA) ~~~~~~~~~~~~~~~~~~~
    //

    /// Minimal size of the difficulty window. Affects the DA algorithm only at the starting period of a new net
    pub const MIN_DIFFICULTY_WINDOW_LEN: usize = 2;

    /// **Legacy** difficulty adjustment window size corresponding to ~46 minutes with 1 BPS
    pub const LEGACY_DIFFICULTY_WINDOW_SIZE: usize = 2641;

    /// **New** difficulty window duration expressed in time units (seconds).
    /// KIP-0004: 30,000 (500 minutes)
    pub const NEW_DIFFICULTY_WINDOW_DURATION: u64 = 30_000;

    /// The desired interval between samples of the difficulty window (seconds).
    /// KIP-0004: 30 seconds
    pub const DIFFICULTY_WINDOW_SAMPLE_INTERVAL: u64 = 30;

    /// Size of the **sampled** difficulty window (independent of BPS)
    pub const DIFFICULTY_SAMPLED_WINDOW_SIZE: u64 = NEW_DIFFICULTY_WINDOW_DURATION / DIFFICULTY_WINDOW_SAMPLE_INTERVAL;

    //
    // ~~~~~~~~~~~~~~~~~~~ Finality & Pruning ~~~~~~~~~~~~~~~~~~~
    //

    /// **Legacy** finality depth (in block units)
    pub const LEGACY_FINALITY_DEPTH: u64 = 86_400;

    /// **New** finality duration expressed in time units (seconds).
    /// TODO: finalize this value (consider 6-24 hours)
    pub const NEW_FINALITY_DURATION: u64 = 43_200; // 12 hours

    /// The value of the pruning proof `M` parameter
    pub const PRUNING_PROOF_M: u64 = 1000;

    //
    // ~~~~~~~~~~~~~~~~~~~ Coinbase ~~~~~~~~~~~~~~~~~~~
    //

    /// **Legacy** value of the coinbase maturity parameter for 1 BPS networks
    pub const LEGACY_COINBASE_MATURITY: u64 = 100;
}

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

    #[derive(Clone, Debug)]
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

#[cfg(test)]
mod tests {
    use super::consensus::{MAX_DIFFICULTY_TARGET, MAX_DIFFICULTY_TARGET_AS_F64};
    use kaspa_math::Uint256;

    #[test]
    fn test_difficulty_max_consts() {
        assert_eq!(MAX_DIFFICULTY_TARGET, Uint256::from_u64(1).wrapping_shl(255) - 1.into());
        assert_eq!(MAX_DIFFICULTY_TARGET_AS_F64, MAX_DIFFICULTY_TARGET.as_f64());
    }
}
