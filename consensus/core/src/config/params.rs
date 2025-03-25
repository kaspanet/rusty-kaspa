pub use super::{
    bps::{Bps, TenBps},
    constants::consensus::*,
    genesis::{GenesisBlock, DEVNET_GENESIS, GENESIS, SIMNET_GENESIS, TESTNET11_GENESIS, TESTNET_GENESIS},
};
use crate::{
    constants::STORAGE_MASS_PARAMETER,
    network::{NetworkId, NetworkType},
    BlockLevel, KType,
};
use kaspa_addresses::Prefix;
use kaspa_math::Uint256;
use std::cmp::min;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ForkActivation(u64);

impl ForkActivation {
    const NEVER: u64 = u64::MAX;
    const ALWAYS: u64 = 0;

    pub const fn new(daa_score: u64) -> Self {
        Self(daa_score)
    }

    pub const fn never() -> Self {
        Self(Self::NEVER)
    }

    pub const fn always() -> Self {
        Self(Self::ALWAYS)
    }

    /// Returns the actual DAA score triggering the activation. Should be used only
    /// for cases where the explicit value is required for computations (e.g., coinbase subsidy).
    /// Otherwise, **activation checks should always go through `self.is_active(..)`**
    pub fn daa_score(self) -> u64 {
        self.0
    }

    pub fn is_active(self, current_daa_score: u64) -> bool {
        current_daa_score >= self.0
    }

    /// Checks if the fork was "recently" activated, i.e., in the time frame of the provided range.
    /// This function returns false for forks that were always active, since they were never activated.
    pub fn is_within_range_from_activation(self, current_daa_score: u64, range: u64) -> bool {
        self != Self::always() && self.is_active(current_daa_score) && current_daa_score < self.0 + range
    }

    /// Checks if the fork is expected to be activated "soon", i.e., in the time frame of the provided range.
    /// Returns the distance from activation if so, or `None` otherwise.  
    pub fn is_within_range_before_activation(self, current_daa_score: u64, range: u64) -> Option<u64> {
        if !self.is_active(current_daa_score) && current_daa_score + range > self.0 {
            Some(self.0 - current_daa_score)
        } else {
            None
        }
    }
}

/// A consensus parameter which depends on forking activation
#[derive(Clone, Copy, Debug)]
pub struct ForkedParam<T: Copy> {
    pre: T,
    post: T,
    activation: ForkActivation,
}

impl<T: Copy> ForkedParam<T> {
    const fn new(pre: T, post: T, activation: ForkActivation) -> Self {
        Self { pre, post, activation }
    }

    pub const fn new_const(val: T) -> Self {
        Self { pre: val, post: val, activation: ForkActivation::never() }
    }

    pub fn activation(&self) -> ForkActivation {
        self.activation
    }

    pub fn get(&self, daa_score: u64) -> T {
        if self.activation.is_active(daa_score) {
            self.post
        } else {
            self.pre
        }
    }

    /// Returns the value before activation (=pre unless activation = always)
    pub fn before(&self) -> T {
        match self.activation.0 {
            ForkActivation::ALWAYS => self.post,
            _ => self.pre,
        }
    }

    /// Returns the permanent long-term value after activation (=post unless the activation is never scheduled)
    pub fn after(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            _ => self.post,
        }
    }

    /// Maps the ForkedParam<T> to a new ForkedParam<U> by applying a map function on both pre and post
    pub fn map<U: Copy, F: Fn(T) -> U>(&self, f: F) -> ForkedParam<U> {
        ForkedParam::new(f(self.pre), f(self.post), self.activation)
    }
}

impl<T: Copy + Ord> ForkedParam<T> {
    /// Returns the min of `pre` and `post` values. Useful for non-consensus initializations
    /// which require knowledge of the value bounds.
    ///
    /// Note that if activation is not scheduled (set to never) then pre is always returned,
    /// and if activation is set to always (since inception), post will be returned.
    pub fn lower_bound(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            ForkActivation::ALWAYS => self.post,
            _ => self.pre.min(self.post),
        }
    }

    /// Returns the max of `pre` and `post` values. Useful for non-consensus initializations
    /// which require knowledge of the value bounds.
    ///
    /// Note that if activation is not scheduled (set to never) then pre is always returned,
    /// and if activation is set to always (since inception), post will be returned.
    pub fn upper_bound(&self) -> T {
        match self.activation.0 {
            ForkActivation::NEVER => self.pre,
            ForkActivation::ALWAYS => self.post,
            _ => self.pre.max(self.post),
        }
    }
}

/// Fork params for the Crescendo hardfork
#[derive(Clone, Debug)]
pub struct CrescendoParams {
    pub past_median_time_sampled_window_size: u64,
    pub sampled_difficulty_window_size: u64,

    /// Target time per block (in milliseconds)
    pub target_time_per_block: u64,
    pub ghostdag_k: KType,

    pub past_median_time_sample_rate: u64,
    pub difficulty_sample_rate: u64,

    pub max_block_parents: u8,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
    pub finality_depth: u64,
    pub pruning_depth: u64,

    pub max_tx_inputs: usize,
    pub max_tx_outputs: usize,
    pub max_signature_script_len: usize,
    pub max_script_public_key_len: usize,

    pub coinbase_maturity: u64,
}

pub const CRESCENDO: CrescendoParams = CrescendoParams {
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE,

    //
    // ~~~~~~~~~~~~~~~~~~ BPS dependent constants ~~~~~~~~~~~~~~~~~~
    //
    target_time_per_block: TenBps::target_time_per_block(),
    ghostdag_k: TenBps::ghostdag_k(),
    past_median_time_sample_rate: TenBps::past_median_time_sample_rate(),
    difficulty_sample_rate: TenBps::difficulty_adjustment_sample_rate(),
    max_block_parents: TenBps::max_block_parents(),
    mergeset_size_limit: TenBps::mergeset_size_limit(),
    merge_depth: TenBps::merge_depth_bound(),
    finality_depth: TenBps::finality_depth(),
    pruning_depth: TenBps::pruning_depth(),

    coinbase_maturity: TenBps::coinbase_maturity(),

    // Limit the cost of calculating compute/transient/storage masses
    max_tx_inputs: 1000,
    max_tx_outputs: 1000,
    // Transient mass enforces a limit of 125Kb, however script engine max scripts size is 10Kb so there's no point in surpassing that.
    max_signature_script_len: 10_000,
    // Compute mass enforces a limit of ~45.5Kb, however script engine max scripts size is 10Kb so there's no point in surpassing that.
    // Note that storage mass will kick in and gradually penalize also for lower lengths (generalized KIP-0009, plurality will be high).
    max_script_public_key_len: 10_000,
};

/// Consensus parameters. Contains settings and configurations which are consensus-sensitive.
/// Changing one of these on a network node would exclude and prevent it from reaching consensus
/// with the other unmodified nodes.
#[derive(Clone, Debug)]
pub struct Params {
    pub dns_seeders: &'static [&'static str],
    pub net: NetworkId,
    pub genesis: GenesisBlock,
    pub prior_ghostdag_k: KType,

    /// Timestamp deviation tolerance (in seconds)
    pub timestamp_deviation_tolerance: u64,

    /// Target time per block (in milliseconds)
    pub prior_target_time_per_block: u64,

    /// Defines the highest allowed proof of work difficulty value for a block as a [`Uint256`]
    pub max_difficulty_target: Uint256,

    /// Highest allowed proof of work difficulty as a floating number
    pub max_difficulty_target_f64: f64,

    /// Size of full blocks window that is inspected to calculate the required difficulty of each block
    pub prior_difficulty_window_size: usize,

    /// The minimum size a difficulty window (full or sampled) must have to trigger a DAA calculation
    pub min_difficulty_window_size: usize,

    pub prior_max_block_parents: u8,
    pub prior_mergeset_size_limit: u64,
    pub prior_merge_depth: u64,
    pub prior_finality_depth: u64,
    pub prior_pruning_depth: u64,

    pub coinbase_payload_script_public_key_max_len: u8,
    pub max_coinbase_payload_len: usize,

    pub prior_max_tx_inputs: usize,
    pub prior_max_tx_outputs: usize,
    pub prior_max_signature_script_len: usize,
    pub prior_max_script_public_key_len: usize,

    pub mass_per_tx_byte: u64,
    pub mass_per_script_pub_key_byte: u64,
    pub mass_per_sig_op: u64,
    pub max_block_mass: u64,

    /// The parameter for scaling inverse KAS value to mass units (KIP-0009)
    pub storage_mass_parameter: u64,

    /// DAA score after which the pre-deflationary period switches to the deflationary period
    pub deflationary_phase_daa_score: u64,

    pub pre_deflationary_phase_base_subsidy: u64,
    pub prior_coinbase_maturity: u64,
    pub skip_proof_of_work: bool,
    pub max_block_level: BlockLevel,
    pub pruning_proof_m: u64,

    pub crescendo: CrescendoParams,
    pub crescendo_activation: ForkActivation,
}

impl Params {
    /// Returns the size of the full blocks window that is inspected to calculate the past median time (legacy)
    #[inline]
    #[must_use]
    pub fn prior_past_median_time_window_size(&self) -> usize {
        (2 * self.timestamp_deviation_tolerance - 1) as usize
    }

    /// Returns the size of the sampled blocks window that is inspected to calculate the past median time
    #[inline]
    #[must_use]
    pub fn sampled_past_median_time_window_size(&self) -> usize {
        self.crescendo.past_median_time_sampled_window_size as usize
    }

    /// Returns the size of the blocks window that is inspected to calculate the past median time.
    #[inline]
    #[must_use]
    pub fn past_median_time_window_size(&self) -> ForkedParam<usize> {
        ForkedParam::new(
            self.prior_past_median_time_window_size(),
            self.sampled_past_median_time_window_size(),
            self.crescendo_activation,
        )
    }

    /// Returns the past median time sample rate
    #[inline]
    #[must_use]
    pub fn past_median_time_sample_rate(&self) -> ForkedParam<u64> {
        ForkedParam::new(1, self.crescendo.past_median_time_sample_rate, self.crescendo_activation)
    }

    /// Returns the size of the blocks window that is inspected to calculate the difficulty
    #[inline]
    #[must_use]
    pub fn difficulty_window_size(&self) -> ForkedParam<usize> {
        ForkedParam::new(
            self.prior_difficulty_window_size,
            self.crescendo.sampled_difficulty_window_size as usize,
            self.crescendo_activation,
        )
    }

    /// Returns the difficulty sample rate
    #[inline]
    #[must_use]
    pub fn difficulty_sample_rate(&self) -> ForkedParam<u64> {
        ForkedParam::new(1, self.crescendo.difficulty_sample_rate, self.crescendo_activation)
    }

    /// Returns the target time per block
    #[inline]
    #[must_use]
    pub fn target_time_per_block(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_target_time_per_block, self.crescendo.target_time_per_block, self.crescendo_activation)
    }

    /// Returns the expected number of blocks per second
    #[inline]
    #[must_use]
    pub fn bps(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            1000 / self.prior_target_time_per_block,
            1000 / self.crescendo.target_time_per_block,
            self.crescendo_activation,
        )
    }

    pub fn ghostdag_k(&self) -> ForkedParam<KType> {
        ForkedParam::new(self.prior_ghostdag_k, self.crescendo.ghostdag_k, self.crescendo_activation)
    }

    pub fn max_block_parents(&self) -> ForkedParam<u8> {
        ForkedParam::new(self.prior_max_block_parents, self.crescendo.max_block_parents, self.crescendo_activation)
    }

    pub fn mergeset_size_limit(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_mergeset_size_limit, self.crescendo.mergeset_size_limit, self.crescendo_activation)
    }

    pub fn merge_depth(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_merge_depth, self.crescendo.merge_depth, self.crescendo_activation)
    }

    pub fn finality_depth(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_finality_depth, self.crescendo.finality_depth, self.crescendo_activation)
    }

    pub fn pruning_depth(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_pruning_depth, self.crescendo.pruning_depth, self.crescendo_activation)
    }

    pub fn coinbase_maturity(&self) -> ForkedParam<u64> {
        ForkedParam::new(self.prior_coinbase_maturity, self.crescendo.coinbase_maturity, self.crescendo_activation)
    }

    pub fn finality_duration_in_milliseconds(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            self.prior_target_time_per_block * self.prior_finality_depth,
            self.crescendo.target_time_per_block * self.crescendo.finality_depth,
            self.crescendo_activation,
        )
    }

    pub fn difficulty_window_duration_in_block_units(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            self.prior_difficulty_window_size as u64,
            self.crescendo.difficulty_sample_rate * self.crescendo.sampled_difficulty_window_size,
            self.crescendo_activation,
        )
    }

    pub fn expected_difficulty_window_duration_in_milliseconds(&self) -> ForkedParam<u64> {
        ForkedParam::new(
            self.prior_target_time_per_block * self.prior_difficulty_window_size as u64,
            self.crescendo.target_time_per_block
                * self.crescendo.difficulty_sample_rate
                * self.crescendo.sampled_difficulty_window_size,
            self.crescendo_activation,
        )
    }

    /// Returns the depth at which the anticone of a chain block is final (i.e., is a permanently closed set).
    /// Based on the analysis at <https://github.com/kaspanet/docs/blob/main/Reference/prunality/Prunality.pdf>
    /// and on the decomposition of merge depth (rule R-I therein) from finality depth (φ)
    pub fn anticone_finalization_depth(&self) -> ForkedParam<u64> {
        let prior_anticone_finalization_depth = self.prior_finality_depth
            + self.prior_merge_depth
            + 4 * self.prior_mergeset_size_limit * self.prior_ghostdag_k as u64
            + 2 * self.prior_ghostdag_k as u64
            + 2;

        let new_anticone_finalization_depth = self.crescendo.finality_depth
            + self.crescendo.merge_depth
            + 4 * self.crescendo.mergeset_size_limit * self.crescendo.ghostdag_k as u64
            + 2 * self.crescendo.ghostdag_k as u64
            + 2;

        // In mainnet it's guaranteed that `self.pruning_depth` is greater
        // than `anticone_finalization_depth`, but for some tests we use
        // a smaller (unsafe) pruning depth, so we return the minimum of
        // the two to avoid a situation where a block can be pruned and
        // not finalized.
        ForkedParam::new(
            min(self.prior_pruning_depth, prior_anticone_finalization_depth),
            min(self.crescendo.pruning_depth, new_anticone_finalization_depth),
            self.crescendo_activation,
        )
    }

    pub fn max_tx_inputs(&self) -> ForkedParam<usize> {
        ForkedParam::new(self.prior_max_tx_inputs, self.crescendo.max_tx_inputs, self.crescendo_activation)
    }

    pub fn max_tx_outputs(&self) -> ForkedParam<usize> {
        ForkedParam::new(self.prior_max_tx_outputs, self.crescendo.max_tx_outputs, self.crescendo_activation)
    }

    pub fn max_signature_script_len(&self) -> ForkedParam<usize> {
        ForkedParam::new(self.prior_max_signature_script_len, self.crescendo.max_signature_script_len, self.crescendo_activation)
    }

    pub fn max_script_public_key_len(&self) -> ForkedParam<usize> {
        ForkedParam::new(self.prior_max_script_public_key_len, self.crescendo.max_script_public_key_len, self.crescendo_activation)
    }

    pub fn network_name(&self) -> String {
        self.net.to_prefixed()
    }

    pub fn prefix(&self) -> Prefix {
        self.net.into()
    }

    pub fn default_p2p_port(&self) -> u16 {
        self.net.default_p2p_port()
    }

    pub fn default_rpc_port(&self) -> u16 {
        self.net.default_rpc_port()
    }
}

impl From<NetworkType> for Params {
    fn from(value: NetworkType) -> Self {
        match value {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => TESTNET_PARAMS,
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
    }
}

impl From<NetworkId> for Params {
    fn from(value: NetworkId) -> Self {
        match value.network_type {
            NetworkType::Mainnet => MAINNET_PARAMS,
            NetworkType::Testnet => match value.suffix {
                Some(10) => TESTNET_PARAMS,
                Some(x) => panic!("Testnet suffix {} is not supported", x),
                None => panic!("Testnet suffix not provided"),
            },
            NetworkType::Devnet => DEVNET_PARAMS,
            NetworkType::Simnet => SIMNET_PARAMS,
        }
    }
}

pub const MAINNET_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Denis Mashkevich
        "mainnet-dnsseed-1.kaspanet.org",
        // This DNS seeder is run by Denis Mashkevich
        "mainnet-dnsseed-2.kaspanet.org",
        // This DNS seeder is run by Constantine Bytensky
        "dnsseed.cbytensky.org",
        // This DNS seeder is run by Georges Künzli
        "seeder1.kaspad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder2.kaspad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder3.kaspad.net",
        // This DNS seeder is run by Georges Künzli
        "seeder4.kaspad.net",
        // This DNS seeder is run by Tim
        "kaspadns.kaspacalc.net",
        // This DNS seeder is run by supertypo
        "n-mainnet.kaspa.ws",
        // This DNS seeder is run by -gerri-
        "dnsseeder-kaspa-mainnet.x-con.at",
        // This DNS seeder is run by H@H
        "ns-mainnet.kaspa-dnsseeder.net",
    ],
    net: NetworkId::new(NetworkType::Mainnet),
    genesis: GENESIS,
    prior_ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    prior_target_time_per_block: 1000,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    prior_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    prior_max_block_parents: 10,
    prior_mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    prior_merge_depth: 3600,
    prior_finality_depth: 86400,
    prior_pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    prior_max_tx_inputs: 1_000_000_000,
    prior_max_tx_outputs: 1_000_000_000,
    prior_max_signature_script_len: 1_000_000_000,
    prior_max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    prior_coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 225,
    pruning_proof_m: 1000,

    crescendo: CRESCENDO,
    // Temp: use a value which will trigger upper_bound/after calculations but is not actually scheduled.
    // We want the pre-release to test the influence of this change on mainnet nodes as well
    crescendo_activation: ForkActivation::new(u64::MAX - 1),
};

pub const TESTNET_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Tiram
        "seeder1-testnet.kaspad.net",
        // This DNS seeder is run by -gerri-
        "dnsseeder-kaspa-testnet.x-con.at",
        // This DNS seeder is run by H@H
        "ns-testnet10.kaspa-dnsseeder.net",
        // This DNS seeder is run by supertypo
        "n-testnet-10.kaspa.ws",
    ],
    net: NetworkId::with_suffix(NetworkType::Testnet, 10),
    genesis: TESTNET_GENESIS,
    prior_ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    prior_target_time_per_block: 1000,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    prior_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    prior_max_block_parents: 10,
    prior_mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    prior_merge_depth: 3600,
    prior_finality_depth: 86400,
    prior_pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    prior_max_tx_inputs: 1_000_000_000,
    prior_max_tx_outputs: 1_000_000_000,
    prior_max_signature_script_len: 1_000_000_000,
    prior_max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    prior_coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,

    crescendo: CRESCENDO,
    // 18:30 UTC, March 6, 2025
    crescendo_activation: ForkActivation::new(88_657_000),
};

pub const SIMNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Simnet),
    genesis: SIMNET_GENESIS,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    prior_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,

    //
    // ~~~~~~~~~~~~~~~~~~ BPS dependent constants ~~~~~~~~~~~~~~~~~~
    //
    // Note we use a 10 BPS configuration for simnet
    prior_ghostdag_k: TenBps::ghostdag_k(),
    prior_target_time_per_block: TenBps::target_time_per_block(),
    // For simnet, we deviate from TN11 configuration and allow at least 64 parents in order to support mempool benchmarks out of the box
    prior_max_block_parents: if TenBps::max_block_parents() > 64 { TenBps::max_block_parents() } else { 64 },
    prior_mergeset_size_limit: TenBps::mergeset_size_limit(),
    prior_merge_depth: TenBps::merge_depth_bound(),
    prior_finality_depth: TenBps::finality_depth(),
    prior_pruning_depth: TenBps::pruning_depth(),
    deflationary_phase_daa_score: TenBps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: TenBps::pre_deflationary_phase_base_subsidy(),
    prior_coinbase_maturity: TenBps::coinbase_maturity(),

    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    prior_max_tx_inputs: 10_000,
    prior_max_tx_outputs: 10_000,
    prior_max_signature_script_len: 1_000_000,
    prior_max_script_public_key_len: 1_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    skip_proof_of_work: true, // For simnet only, PoW can be simulated by default
    max_block_level: 250,
    pruning_proof_m: PRUNING_PROOF_M,

    crescendo: CRESCENDO,
    crescendo_activation: ForkActivation::always(),
};

pub const DEVNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Devnet),
    genesis: DEVNET_GENESIS,
    prior_ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    prior_target_time_per_block: 1000,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    prior_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_size: MIN_DIFFICULTY_WINDOW_SIZE,
    prior_max_block_parents: 10,
    prior_mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    prior_merge_depth: 3600,
    prior_finality_depth: 86400,
    prior_pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    prior_max_tx_inputs: 1_000_000_000,
    prior_max_tx_outputs: 1_000_000_000,
    prior_max_signature_script_len: 1_000_000_000,
    prior_max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,

    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    prior_coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,

    crescendo: CRESCENDO,
    crescendo_activation: ForkActivation::never(),
};
