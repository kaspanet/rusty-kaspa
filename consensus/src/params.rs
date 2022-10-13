use hashes::{Hash, HASH_SIZE};

use crate::model::stores::ghostdag::KType;

#[derive(Clone)]
pub struct Params {
    pub genesis_hash: Hash,
    pub ghostdag_k: KType,
    pub timestamp_deviation_tolerance: u64,
    pub target_time_per_block: u64,
    pub max_block_parents: u8,
    pub difficulty_window_size: usize,
    pub genesis_timestamp: u64,
    pub genesis_bits: u32,
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
    pub max_block_level: u8,
}

impl Params {
    /// Clones the params instance and sets `skip_proof_of_work = true`.
    /// Should be used for testing purposes only.
    pub fn clone_with_skip_pow(&self) -> Self {
        let mut cloned_params = self.clone();
        cloned_params.skip_proof_of_work = true;
        cloned_params
    }
}

const DEFAULT_GHOSTDAG_K: KType = 18;
pub const MAINNET_PARAMS: Params = Params {
    genesis_hash: Hash::from_bytes([1u8; HASH_SIZE]), // TODO: Use real mainnet genesis here
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: 132,
    target_time_per_block: 1000,
    max_block_parents: 10,
    difficulty_window_size: 2641,
    genesis_timestamp: 0,     // TODO: Use real value
    genesis_bits: 0x207fffff, // TODO: Use real value
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
    pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's encorced by the network layer that limits the message
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
};

pub const DEVNET_PARAMS: Params = Params {
    genesis_hash: Hash::from_bytes([1u8; HASH_SIZE]), // TODO: Use real mainnet genesis here
    ghostdag_k: DEFAULT_GHOSTDAG_K,
    timestamp_deviation_tolerance: 132,
    target_time_per_block: 1000,
    max_block_parents: 10,
    difficulty_window_size: 2641,
    genesis_timestamp: 0,     // TODO: Use real value
    genesis_bits: 0x207fffff, // TODO: Use real value
    mergeset_size_limit: (DEFAULT_GHOSTDAG_K as u64) * 10,
    merge_depth: 3600,
    finality_depth: 86400,
    pruning_depth: 185798,
    coinbase_payload_script_public_key_max_len: 150,
    max_coinbase_payload_len: 204,

    // This is technically a soft fork from the Go implementation since kaspad's consensus doesn't
    // check these rules, but in practice it's encorced by the network layer that limits the message
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
};
