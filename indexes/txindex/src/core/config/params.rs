use crate::core::config::constants::{
    DEFAULT_TRANSACTION_SIG_OPS, DEFAULT_TRANSACTION_SIZE, SCHNORR_SCRIPT_PUBLIC_KEY_BYTES_PER_TRANSACTION,
};
use kaspa_consensus_core::config::Config as ConsensusConfig;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Params {
    pub max_default_txs_per_block: u64,
    pub max_blocks_in_mergeset_depth: u64,
}

impl Params {
    pub fn new(consensus_config: &Arc<ConsensusConfig>) -> Self {
        Self {
            max_default_txs_per_block: consensus_config.params.max_block_mass
                / (SCHNORR_SCRIPT_PUBLIC_KEY_BYTES_PER_TRANSACTION * consensus_config.params.mass_per_script_pub_key_byte
                    + DEFAULT_TRANSACTION_SIG_OPS * consensus_config.params.mass_per_sig_op
                    + DEFAULT_TRANSACTION_SIZE * consensus_config.params.mass_per_tx_byte),
            max_blocks_in_mergeset_depth: consensus_config.params.merge_depth * consensus_config.params.mergeset_size_limit,
        }
    }
    pub fn max_default_txs_in_merge_depth(&self) -> u64 {
        self.max_blocks_in_mergeset_depth * self.max_default_txs_per_block
    }
}
