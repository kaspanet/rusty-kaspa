pub use super::{
    bps::{Bps, Testnet11Bps},
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
use std::{
    cmp::min,
    time::{SystemTime, UNIX_EPOCH},
};

/// Consensus parameters. Contains settings and configurations which are consensus-sensitive.
/// Changing one of these on a network node would exclude and prevent it from reaching consensus
/// with the other unmodified nodes.
#[derive(Clone, Debug)]
pub struct Params {
    pub dns_seeders: &'static [&'static str],
    pub net: NetworkId,
    pub genesis: GenesisBlock,
    pub ghostdag_k: KType,

    /// Legacy timestamp deviation tolerance (in seconds)
    pub legacy_timestamp_deviation_tolerance: u64,

    /// New timestamp deviation tolerance (in seconds, activated with sampling)
    pub new_timestamp_deviation_tolerance: u64,

    /// Block sample rate for filling the past median time window (selects one every N blocks)
    pub past_median_time_sample_rate: u64,

    /// Size of sampled blocks window that is inspected to calculate the past median time of each block
    pub past_median_time_sampled_window_size: u64,

    /// Target time per block (in milliseconds)
    pub target_time_per_block: u64,

    /// DAA score from which the window sampling starts for difficulty and past median time calculation
    pub sampling_activation_daa_score: u64,

    /// Defines the highest allowed proof of work difficulty value for a block as a [`Uint256`]
    pub max_difficulty_target: Uint256,

    /// Highest allowed proof of work difficulty as a floating number
    pub max_difficulty_target_f64: f64,

    /// Block sample rate for filling the difficulty window (selects one every N blocks)
    pub difficulty_sample_rate: u64,

    /// Size of sampled blocks window that is inspected to calculate the required difficulty of each block
    pub sampled_difficulty_window_size: usize,

    /// Size of full blocks window that is inspected to calculate the required difficulty of each block
    pub legacy_difficulty_window_size: usize,

    /// The minimum length a difficulty window (full or sampled) must have to trigger a DAA calculation
    pub min_difficulty_window_len: usize,

    pub max_block_parents: u8,
    pub mergeset_size_limit: u64,
    pub merge_depth: u64,
    pub finality_depth: u64,
    pub pruning_depth: u64,
    pub coinbase_payload_script_public_key_max_len: u8,
    pub max_coinbase_payload_len: usize,
    pub max_tx_inputs: usize,
    pub max_tx_outputs: usize,
    pub max_signature_script_len: usize,
    pub max_script_public_key_len: usize,
    pub mass_per_tx_byte: u64,
    pub mass_per_script_pub_key_byte: u64,
    pub mass_per_sig_op: u64,
    pub max_block_mass: u64,

    /// The parameter for scaling inverse KAS value to mass units (unpublished KIP-0009)
    pub storage_mass_parameter: u64,

    /// DAA score from which storage mass calculation and transaction mass field are activated as a consensus rule
    pub storage_mass_activation_daa_score: u64,

    /// DAA score after which the pre-deflationary period switches to the deflationary period
    pub deflationary_phase_daa_score: u64,

    pub pre_deflationary_phase_base_subsidy: u64,
    pub coinbase_maturity: u64,
    pub skip_proof_of_work: bool,
    pub max_block_level: BlockLevel,
    pub pruning_proof_m: u64,
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

impl Params {
    /// Returns the size of the full blocks window that is inspected to calculate the past median time (legacy)
    #[inline]
    #[must_use]
    pub fn legacy_past_median_time_window_size(&self) -> usize {
        (2 * self.legacy_timestamp_deviation_tolerance - 1) as usize
    }

    /// Returns the size of the sampled blocks window that is inspected to calculate the past median time
    #[inline]
    #[must_use]
    pub fn sampled_past_median_time_window_size(&self) -> usize {
        self.past_median_time_sampled_window_size as usize
    }

    /// Returns the size of the blocks window that is inspected to calculate the past median time,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn past_median_time_window_size(&self, selected_parent_daa_score: u64) -> usize {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.legacy_past_median_time_window_size()
        } else {
            self.sampled_past_median_time_window_size()
        }
    }

    /// Returns the timestamp deviation tolerance,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn timestamp_deviation_tolerance(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.legacy_timestamp_deviation_tolerance
        } else {
            self.new_timestamp_deviation_tolerance
        }
    }

    /// Returns the past median time sample rate,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn past_median_time_sample_rate(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            1
        } else {
            self.past_median_time_sample_rate
        }
    }

    /// Returns the size of the blocks window that is inspected to calculate the difficulty,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn difficulty_window_size(&self, selected_parent_daa_score: u64) -> usize {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.legacy_difficulty_window_size
        } else {
            self.sampled_difficulty_window_size
        }
    }

    /// Returns the difficulty sample rate,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn difficulty_sample_rate(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            1
        } else {
            self.difficulty_sample_rate
        }
    }

    /// Returns the target time per block,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn target_time_per_block(&self, _selected_parent_daa_score: u64) -> u64 {
        self.target_time_per_block
    }

    /// Returns the expected number of blocks per second
    #[inline]
    #[must_use]
    pub fn bps(&self) -> u64 {
        1000 / self.target_time_per_block
    }

    pub fn daa_window_duration_in_blocks(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.legacy_difficulty_window_size as u64
        } else {
            self.difficulty_sample_rate * self.sampled_difficulty_window_size as u64
        }
    }

    fn expected_daa_window_duration_in_milliseconds(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.target_time_per_block * self.legacy_difficulty_window_size as u64
        } else {
            self.target_time_per_block * self.difficulty_sample_rate * self.sampled_difficulty_window_size as u64
        }
    }

    /// Returns the depth at which the anticone of a chain block is final (i.e., is a permanently closed set).
    /// Based on the analysis at <https://github.com/kaspanet/docs/blob/main/Reference/prunality/Prunality.pdf>
    /// and on the decomposition of merge depth (rule R-I therein) from finality depth (φ)
    pub fn anticone_finalization_depth(&self) -> u64 {
        let anticone_finalization_depth = self.finality_depth
            + self.merge_depth
            + 4 * self.mergeset_size_limit * self.ghostdag_k as u64
            + 2 * self.ghostdag_k as u64
            + 2;

        // In mainnet it's guaranteed that `self.pruning_depth` is greater
        // than `anticone_finalization_depth`, but for some tests we use
        // a smaller (unsafe) pruning depth, so we return the minimum of
        // the two to avoid a situation where a block can be pruned and
        // not finalized.
        min(self.pruning_depth, anticone_finalization_depth)
    }

    /// Returns whether the sink timestamp is recent enough and the node is considered synced or nearly synced.
    pub fn is_nearly_synced(&self, sink_timestamp: u64, sink_daa_score: u64) -> bool {
        if self.net.is_mainnet() {
            // We consider the node close to being synced if the sink (virtual selected parent) block
            // timestamp is within DAA window duration far in the past. Blocks mined over such DAG state would
            // enter the DAA window of fully-synced nodes and thus contribute to overall network difficulty
            unix_now() < sink_timestamp + self.expected_daa_window_duration_in_milliseconds(sink_daa_score)
        } else {
            // For testnets we consider the node to be synced if the sink timestamp is within a time range which
            // is overwhelmingly unlikely to pass without mined blocks even if net hashrate decreased dramatically.
            //
            // This period is smaller than the above mainnet calculation in order to ensure that an IBDing miner
            // with significant testnet hashrate does not overwhelm the network with deep side-DAGs.
            //
            // We use DAA duration as baseline and scale it down with BPS (and divide by 3 for mining only when very close to current time on TN11)
            let max_expected_duration_without_blocks_in_milliseconds = self.target_time_per_block * NEW_DIFFICULTY_WINDOW_DURATION / 3; // = DAA duration in milliseconds / bps / 3
            unix_now() < sink_timestamp + max_expected_duration_without_blocks_in_milliseconds
        }
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

    pub fn finality_duration(&self) -> u64 {
        self.target_time_per_block * self.finality_depth
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
                Some(11) => TESTNET11_PARAMS,
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
    ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    legacy_timestamp_deviation_tolerance: LEGACY_TIMESTAMP_DEVIATION_TOLERANCE,
    new_timestamp_deviation_tolerance: NEW_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: Bps::<1>::past_median_time_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: Bps::<1>::difficulty_adjustment_sample_rate(),
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    legacy_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
    pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    max_tx_inputs: 1_000_000_000,
    max_tx_outputs: 1_000_000_000,
    max_signature_script_len: 1_000_000_000,
    max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    storage_mass_activation_daa_score: u64::MAX,

    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 225,
    pruning_proof_m: 1000,
};

pub const TESTNET_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Tiram
        "seeder1-testnet.kaspad.net",
        // This DNS seeder is run by -gerri-
        "dnsseeder-kaspa-testnet.x-con.at",
        // This DNS seeder is run by H@H
        "ns-testnet10.kaspa-dnsseeder.net",
    ],
    net: NetworkId::with_suffix(NetworkType::Testnet, 10),
    genesis: TESTNET_GENESIS,
    ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    legacy_timestamp_deviation_tolerance: LEGACY_TIMESTAMP_DEVIATION_TOLERANCE,
    new_timestamp_deviation_tolerance: NEW_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: Bps::<1>::past_median_time_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: Bps::<1>::difficulty_adjustment_sample_rate(),
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    legacy_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
    pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    max_tx_inputs: 1_000_000_000,
    max_tx_outputs: 1_000_000_000,
    max_signature_script_len: 1_000_000_000,
    max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    storage_mass_activation_daa_score: u64::MAX,

    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,
};

pub const TESTNET11_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Tiram
        "seeder1-testnet-11.kaspad.net",
        // This DNS seeder is run by supertypo
        "n-testnet-11.kaspa.ws",
        // This DNS seeder is run by -gerri-
        "dnsseeder-kaspa-testnet11.x-con.at",
        // This DNS seeder is run by H@H
        "ns-testnet11.kaspa-dnsseeder.net",
    ],
    net: NetworkId::with_suffix(NetworkType::Testnet, 11),
    genesis: TESTNET11_GENESIS,
    legacy_timestamp_deviation_tolerance: LEGACY_TIMESTAMP_DEVIATION_TOLERANCE,
    new_timestamp_deviation_tolerance: NEW_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    sampling_activation_daa_score: 0, // Sampling is activated from network inception
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    legacy_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,

    //
    // ~~~~~~~~~~~~~~~~~~ BPS dependent constants ~~~~~~~~~~~~~~~~~~
    //
    ghostdag_k: Testnet11Bps::ghostdag_k(),
    target_time_per_block: Testnet11Bps::target_time_per_block(),
    past_median_time_sample_rate: Testnet11Bps::past_median_time_sample_rate(),
    difficulty_sample_rate: Testnet11Bps::difficulty_adjustment_sample_rate(),
    max_block_parents: Testnet11Bps::max_block_parents(),
    mergeset_size_limit: Testnet11Bps::mergeset_size_limit(),
    merge_depth: Testnet11Bps::merge_depth_bound(),
    finality_depth: Testnet11Bps::finality_depth(),
    pruning_depth: Testnet11Bps::pruning_depth(),
    pruning_proof_m: Testnet11Bps::pruning_proof_m(),
    deflationary_phase_daa_score: Testnet11Bps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: Testnet11Bps::pre_deflationary_phase_base_subsidy(),
    coinbase_maturity: Testnet11Bps::coinbase_maturity(),

    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    max_tx_inputs: 10_000,
    max_tx_outputs: 10_000,
    max_signature_script_len: 1_000_000,
    max_script_public_key_len: 1_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    storage_mass_activation_daa_score: 0,

    skip_proof_of_work: false,
    max_block_level: 250,
};

pub const SIMNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Simnet),
    genesis: SIMNET_GENESIS,
    legacy_timestamp_deviation_tolerance: LEGACY_TIMESTAMP_DEVIATION_TOLERANCE,
    new_timestamp_deviation_tolerance: NEW_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    sampling_activation_daa_score: 0, // Sampling is activated from network inception
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    legacy_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,

    //
    // ~~~~~~~~~~~~~~~~~~ BPS dependent constants ~~~~~~~~~~~~~~~~~~
    //
    // Note we use a 10 BPS configuration for simnet
    ghostdag_k: Testnet11Bps::ghostdag_k(),
    target_time_per_block: Testnet11Bps::target_time_per_block(),
    past_median_time_sample_rate: Testnet11Bps::past_median_time_sample_rate(),
    difficulty_sample_rate: Testnet11Bps::difficulty_adjustment_sample_rate(),
    // For simnet, we deviate from TN11 configuration and allow at least 64 parents in order to support mempool benchmarks out of the box
    max_block_parents: if Testnet11Bps::max_block_parents() > 64 { Testnet11Bps::max_block_parents() } else { 64 },
    mergeset_size_limit: Testnet11Bps::mergeset_size_limit(),
    merge_depth: Testnet11Bps::merge_depth_bound(),
    finality_depth: Testnet11Bps::finality_depth(),
    pruning_depth: Testnet11Bps::pruning_depth(),
    pruning_proof_m: Testnet11Bps::pruning_proof_m(),
    deflationary_phase_daa_score: Testnet11Bps::deflationary_phase_daa_score(),
    pre_deflationary_phase_base_subsidy: Testnet11Bps::pre_deflationary_phase_base_subsidy(),
    coinbase_maturity: Testnet11Bps::coinbase_maturity(),

    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    max_tx_inputs: 10_000,
    max_tx_outputs: 10_000,
    max_signature_script_len: 1_000_000,
    max_script_public_key_len: 1_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    storage_mass_activation_daa_score: 0,

    skip_proof_of_work: true, // For simnet only, PoW can be simulated by default
    max_block_level: 250,
};

pub const DEVNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkId::new(NetworkType::Devnet),
    genesis: DEVNET_GENESIS,
    ghostdag_k: LEGACY_DEFAULT_GHOSTDAG_K,
    legacy_timestamp_deviation_tolerance: LEGACY_TIMESTAMP_DEVIATION_TOLERANCE,
    new_timestamp_deviation_tolerance: NEW_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: Bps::<1>::past_median_time_sample_rate(),
    past_median_time_sampled_window_size: MEDIAN_TIME_SAMPLED_WINDOW_SIZE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: Bps::<1>::difficulty_adjustment_sample_rate(),
    sampled_difficulty_window_size: DIFFICULTY_SAMPLED_WINDOW_SIZE as usize,
    legacy_difficulty_window_size: LEGACY_DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (LEGACY_DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
    pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's enforced by the network layer that limits the message
    // size to 1 GB.
    // These values should be lowered to more reasonable amounts on the next planned HF/SF.
    max_tx_inputs: 1_000_000_000,
    max_tx_outputs: 1_000_000_000,
    max_signature_script_len: 1_000_000_000,
    max_script_public_key_len: 1_000_000_000,

    mass_per_tx_byte: 1,
    mass_per_script_pub_key_byte: 10,
    mass_per_sig_op: 1000,
    max_block_mass: 500_000,

    storage_mass_parameter: STORAGE_MASS_PARAMETER,
    storage_mass_activation_daa_score: u64::MAX,

    // deflationary_phase_daa_score is the DAA score after which the pre-deflationary period
    // switches to the deflationary period. This number is calculated as follows:
    // We define a year as 365.25 days
    // Half a year in seconds = 365.25 / 2 * 24 * 60 * 60 = 15778800
    // The network was down for three days shortly after launch
    // Three days in seconds = 3 * 24 * 60 * 60 = 259200
    deflationary_phase_daa_score: 15778800 - 259200,
    pre_deflationary_phase_base_subsidy: 50000000000,
    coinbase_maturity: 100,
    skip_proof_of_work: false,
    max_block_level: 250,
    pruning_proof_m: 1000,
};
