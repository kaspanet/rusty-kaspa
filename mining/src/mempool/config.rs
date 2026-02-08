use kaspa_consensus_core::mass::{BlockMassLimits, MassCofactors};

pub(crate) const DEFAULT_MAXIMUM_TRANSACTION_COUNT: usize = 1_000_000;
pub(crate) const DEFAULT_MEMPOOL_SIZE_LIMIT: usize = 1_000_000_000;
pub(crate) const DEFAULT_MAXIMUM_BUILD_BLOCK_TEMPLATE_ATTEMPTS: u64 = 5;

pub(crate) const DEFAULT_TRANSACTION_EXPIRE_INTERVAL_SECONDS: u64 = 24 * 60 * 60;
pub(crate) const DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 60;
pub(crate) const DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_INTERVAL_SECONDS: u64 = 120;
pub(crate) const DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 10;
pub(crate) const DEFAULT_ORPHAN_EXPIRE_INTERVAL_SECONDS: u64 = 60;
pub(crate) const DEFAULT_ORPHAN_EXPIRE_SCAN_INTERVAL_SECONDS: u64 = 10;

pub(crate) const DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_MASS: u64 = 1_000_000; // TODO(covpp-mainnet)
pub(crate) const DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_COUNT: u64 = 500;

/// DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE specifies the minimum transaction fee for a transaction to be accepted to
/// the mempool and relayed. It is specified in sompi per 1kg (or 1000 grams) of transaction mass.
pub(crate) const DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE: u64 = 1000;

#[derive(Clone, Debug)]
pub struct Config {
    pub maximum_transaction_count: usize,
    pub mempool_size_limit: usize,
    pub maximum_build_block_template_attempts: u64,
    pub transaction_expire_interval_daa_score: u64,
    pub transaction_expire_scan_interval_daa_score: u64,
    pub transaction_expire_scan_interval_milliseconds: u64,
    pub accepted_transaction_expire_interval_daa_score: u64,
    pub accepted_transaction_expire_scan_interval_daa_score: u64,
    pub accepted_transaction_expire_scan_interval_milliseconds: u64,
    pub orphan_expire_interval_daa_score: u64,
    pub orphan_expire_scan_interval_daa_score: u64,
    pub maximum_orphan_transaction_mass: u64, // TODO normalized max mass
    pub maximum_orphan_transaction_count: u64,
    pub accept_non_standard: bool,
    pub mass_cofactors: MassCofactors,
    pub minimum_relay_transaction_fee: u64,
    pub network_blocks_per_second: u64,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        maximum_transaction_count: usize,
        mempool_size_limit: usize,
        maximum_build_block_template_attempts: u64,
        transaction_expire_interval_daa_score: u64,
        transaction_expire_scan_interval_daa_score: u64,
        transaction_expire_scan_interval_milliseconds: u64,
        accepted_transaction_expire_interval_daa_score: u64,
        accepted_transaction_expire_scan_interval_daa_score: u64,
        accepted_transaction_expire_scan_interval_milliseconds: u64,
        orphan_expire_interval_daa_score: u64,
        orphan_expire_scan_interval_daa_score: u64,
        maximum_orphan_transaction_mass: u64, // TODO normalized max mass
        maximum_orphan_transaction_count: u64,
        accept_non_standard: bool,
        mass_cofactors: MassCofactors,
        minimum_relay_transaction_fee: u64,
        network_blocks_per_second: u64,
    ) -> Self {
        Self {
            maximum_transaction_count,
            mempool_size_limit,
            maximum_build_block_template_attempts,
            transaction_expire_interval_daa_score,
            transaction_expire_scan_interval_daa_score,
            transaction_expire_scan_interval_milliseconds,
            accepted_transaction_expire_interval_daa_score,
            accepted_transaction_expire_scan_interval_daa_score,
            accepted_transaction_expire_scan_interval_milliseconds,
            orphan_expire_interval_daa_score,
            orphan_expire_scan_interval_daa_score,
            maximum_orphan_transaction_mass,
            maximum_orphan_transaction_count,
            accept_non_standard,
            mass_cofactors,
            minimum_relay_transaction_fee,
            network_blocks_per_second,
        }
    }

    /// Build a default config.
    /// The arguments should be obtained from the current consensus [`kaspa_consensus_core::config::params::Params`] instance.
    pub fn build_default(
        target_milliseconds_per_block: u64,
        relay_non_std_transactions: bool,
        block_mass_limits: BlockMassLimits,
    ) -> Self {
        let mass_cofactors = block_mass_limits.cofactors();
        Self {
            maximum_transaction_count: DEFAULT_MAXIMUM_TRANSACTION_COUNT,
            mempool_size_limit: DEFAULT_MEMPOOL_SIZE_LIMIT,
            maximum_build_block_template_attempts: DEFAULT_MAXIMUM_BUILD_BLOCK_TEMPLATE_ATTEMPTS,
            transaction_expire_interval_daa_score: DEFAULT_TRANSACTION_EXPIRE_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            transaction_expire_scan_interval_daa_score: DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            transaction_expire_scan_interval_milliseconds: DEFAULT_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000,
            accepted_transaction_expire_interval_daa_score: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            accepted_transaction_expire_scan_interval_daa_score: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000
                / target_milliseconds_per_block,
            accepted_transaction_expire_scan_interval_milliseconds: DEFAULT_ACCEPTED_TRANSACTION_EXPIRE_SCAN_INTERVAL_SECONDS * 1000,
            orphan_expire_interval_daa_score: DEFAULT_ORPHAN_EXPIRE_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            orphan_expire_scan_interval_daa_score: DEFAULT_ORPHAN_EXPIRE_SCAN_INTERVAL_SECONDS * 1000 / target_milliseconds_per_block,
            maximum_orphan_transaction_mass: DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_MASS, // TODO normalized max mass
            maximum_orphan_transaction_count: DEFAULT_MAXIMUM_ORPHAN_TRANSACTION_COUNT,
            accept_non_standard: relay_non_std_transactions,
            mass_cofactors,
            minimum_relay_transaction_fee: DEFAULT_MINIMUM_RELAY_TRANSACTION_FEE,
            network_blocks_per_second: 1000 / target_milliseconds_per_block,
        }
    }

    pub fn apply_ram_scale(mut self, ram_scale: f64) -> Self {
        // Allow only scaling down
        self.maximum_transaction_count = (self.maximum_transaction_count as f64 * ram_scale.min(1.0)) as usize;
        self.mempool_size_limit = (self.mempool_size_limit as f64 * ram_scale.min(1.0)) as usize;
        self
    }

    /// Returns the minimum standard fee/mass ratio currently required by the mempool
    pub(crate) fn minimum_feerate(&self) -> f64 {
        // The parameter minimum_relay_transaction_fee is in sompi/kg units so divide by 1000 to get sompi/gram
        self.minimum_relay_transaction_fee as f64 / 1000.0
    }
}
