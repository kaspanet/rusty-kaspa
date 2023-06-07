use super::genesis::{GenesisBlock, DEVNET_GENESIS, GENESIS, SIMNET_GENESIS, TESTNET_GENESIS};
use crate::{networktype::NetworkType, BlockLevel, KType};
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
    pub net: NetworkType,
    pub net_suffix: Option<u32>,
    pub genesis: GenesisBlock,
    pub ghostdag_k: KType,

    /// Timestamp deviation tolerance expressed in number of blocks
    pub full_timestamp_deviation_tolerance: u64,

    /// Timestamp deviation tolerance expressed in number of un-sampled blocks when a sampled window is used
    pub sampled_timestamp_deviation_tolerance: u64,

    /// Block sample rate for filling the past median time window (selects one every N blocks)
    pub past_median_time_sample_rate: u64,

    /// Current/legacy target time per block
    pub target_time_per_block: u64,

    /// DAA score from which the window sampling starts for difficulty and past median time calculation
    pub sampling_activation_daa_score: u64,

    /// Defines the highest allowed proof of work difficulty value for a block as a [`Uint256`]
    pub max_difficulty_target: Uint256,
    pub max_difficulty_target_f64: f64,

    /// Block sample rate for filling the difficulty window (selects one every N blocks)
    pub difficulty_sample_rate: u64,

    /// Size of sampled blocks window that is inspected to calculate the required difficulty of each block
    pub sampled_difficulty_window_size: usize,

    /// Size of full blocks window that is inspected to calculate the required difficulty of each block
    pub full_difficulty_window_size: usize,

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
    /// Returns the size of the full blocks window that is inspected to calculate the past median time
    #[inline]
    #[must_use]
    pub fn full_past_median_time_window_size(&self) -> usize {
        (2 * self.full_timestamp_deviation_tolerance - 1) as usize
    }

    /// Returns the size of the sampled blocks window that is inspected to calculate the past median time
    #[inline]
    #[must_use]
    pub fn sampled_past_median_time_window_size(&self) -> usize {
        let deviation_tolerance_sampled_blocks =
            (self.sampled_timestamp_deviation_tolerance + self.past_median_time_sample_rate / 2) / self.past_median_time_sample_rate;
        (2 * deviation_tolerance_sampled_blocks - 1) as usize
    }

    /// Returns the size of the blocks window that is inspected to calculate the past median time,
    /// depending on a selected parent DAA score
    #[inline]
    #[must_use]
    pub fn past_median_time_window_size(&self, selected_parent_daa_score: u64) -> usize {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.full_past_median_time_window_size()
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
            self.full_timestamp_deviation_tolerance
        } else {
            self.sampled_timestamp_deviation_tolerance
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
            self.full_difficulty_window_size
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

    fn expected_daa_window_duration_in_milliseconds(&self, selected_parent_daa_score: u64) -> u64 {
        if selected_parent_daa_score < self.sampling_activation_daa_score {
            self.target_time_per_block * self.full_difficulty_window_size as u64
        } else {
            self.target_time_per_block * self.difficulty_sample_rate * self.sampled_difficulty_window_size as u64
        }
    }

    /// Returns the depth at which the anticone of a chain block is final (i.e., is a permanently closed set).
    /// Based on the analysis at https://github.com/kaspanet/docs/blob/main/Reference/prunality/Prunality.pdf
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
        // We consider the node close to being synced if the sink (virtual selected parent) block
        // timestamp is within DAA window duration far in the past. Blocks mined over such DAG state would
        // enter the DAA window of fully-synced nodes and thus contribute to overall network difficulty
        unix_now() < sink_timestamp + self.expected_daa_window_duration_in_milliseconds(sink_daa_score)
    }

    pub fn network_name(&self) -> String {
        self.net.name(self.net_suffix)
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

pub const TIMESTAMP_DEVIATION_TOLERANCE: u64 = 132;
pub const SAMPLE_TIMESTAMP_DEVIATION_TOLERANCE: u64 = 600; // KIP-0004: 20/2 = 10 minutes, so 600 @ current BPS
pub const PAST_MEDIAN_TIME_SAMPLE_RATE: u64 = 10; // KIP-0004: every 10 seconds, so 10 @ current BPS

/// Highest proof of work difficulty target a Kaspa block can have for each network.
/// This value is: 2^255 - 1.
///
/// Computed value: `Uint256::from_u64(1).wrapping_shl(255) - 1.into()`
pub const MAX_DIFFICULTY_TARGET: Uint256 =
    Uint256([18446744073709551615, 18446744073709551615, 18446744073709551615, 9223372036854775807]);
pub const MAX_DIFFICULTY_TARGET_AS_F64: f64 = 5.78960446186581e76;

pub const MIN_DIFFICULTY_WINDOW_LEN: usize = 2;

pub const DIFFICULTY_WINDOW_SIZE: usize = 2641;
pub const DIFFICULTY_SAMPLE_WINDOW_SIZE: usize = 1001; // KIP-0004: 500 minutes, so 1000 + 1 @ current BPS and sample rate;
pub const DIFFICULTY_SAMPLE_RATE: u64 = 30; // KIP-0004: every 30 seconds, so 30 @ current BPS

const DEFAULT_GHOSTDAG_K: KType = 18;
pub const MAINNET_PARAMS: Params = Params {
    dns_seeders: &[
        // This DNS seeder is run by Wolfie
        "mainnet-dnsseed.kas.pa",
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
    ],
    net: NetworkType::Mainnet,
    net_suffix: None,
    genesis: GENESIS,
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    full_timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    sampled_timestamp_deviation_tolerance: SAMPLE_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: PAST_MEDIAN_TIME_SAMPLE_RATE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: DIFFICULTY_SAMPLE_RATE,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLE_WINDOW_SIZE,
    full_difficulty_window_size: DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
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
        "testnet-10-dnsseed.kas.pa",
        // This DNS seeder is run by Tiram
        "seeder1-testnet.kaspad.net",
    ],
    net: NetworkType::Testnet,
    net_suffix: Some(10),
    genesis: TESTNET_GENESIS,
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    full_timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    sampled_timestamp_deviation_tolerance: SAMPLE_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: PAST_MEDIAN_TIME_SAMPLE_RATE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: DIFFICULTY_SAMPLE_RATE,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLE_WINDOW_SIZE,
    full_difficulty_window_size: DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
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

pub const SIMNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkType::Simnet,
    net_suffix: None,
    genesis: SIMNET_GENESIS,
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    full_timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    sampled_timestamp_deviation_tolerance: SAMPLE_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: PAST_MEDIAN_TIME_SAMPLE_RATE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: DIFFICULTY_SAMPLE_RATE,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLE_WINDOW_SIZE,
    full_difficulty_window_size: DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
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

pub const DEVNET_PARAMS: Params = Params {
    dns_seeders: &[],
    net: NetworkType::Devnet,
    net_suffix: None,
    genesis: DEVNET_GENESIS,
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    full_timestamp_deviation_tolerance: TIMESTAMP_DEVIATION_TOLERANCE,
    sampled_timestamp_deviation_tolerance: SAMPLE_TIMESTAMP_DEVIATION_TOLERANCE,
    past_median_time_sample_rate: PAST_MEDIAN_TIME_SAMPLE_RATE,
    target_time_per_block: 1000,
    sampling_activation_daa_score: u64::MAX,
    max_difficulty_target: MAX_DIFFICULTY_TARGET,
    max_difficulty_target_f64: MAX_DIFFICULTY_TARGET_AS_F64,
    difficulty_sample_rate: DIFFICULTY_SAMPLE_RATE,
    sampled_difficulty_window_size: DIFFICULTY_SAMPLE_WINDOW_SIZE,
    full_difficulty_window_size: DIFFICULTY_WINDOW_SIZE,
    min_difficulty_window_len: MIN_DIFFICULTY_WINDOW_LEN,
    max_block_parents: 10,
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
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

#[cfg(test)]
mod tests {
    use crate::config::params::{MAX_DIFFICULTY_TARGET, MAX_DIFFICULTY_TARGET_AS_F64};
    use kaspa_math::Uint256;

    #[test]
    fn test_difficulty_max_consts() {
        assert_eq!(MAX_DIFFICULTY_TARGET, Uint256::from_u64(1).wrapping_shl(255) - 1.into());
        assert_eq!(MAX_DIFFICULTY_TARGET_AS_F64, MAX_DIFFICULTY_TARGET.as_f64());
    }
}
