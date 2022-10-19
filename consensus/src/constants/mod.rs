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

    /// The default standard cache size for per-block stores
    pub const CACHE_SIZE: u64 = 100_000;

    /// The default cache size for large data items (e.g., block DAA window)
    pub const LARGE_DATA_CACHE_SIZE: u64 = 2_000;

    /// The default cache size for UTXO set entries
    pub const UTXO_CACHE_SIZE: u64 = 10_000;
}

pub mod store_names {
    pub const VIRTUAL_UTXO_SET: &[u8] = b"virtual-utxo-set";
    pub const PRUNING_UTXO_SET: &[u8] = b"pruning-utxo-set";
    pub const BODY_TIPS: &[u8] = b"body-tips";
    pub const HEADERS_SELECTED_TIP: &[u8] = b"headers-selected-tip";
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
