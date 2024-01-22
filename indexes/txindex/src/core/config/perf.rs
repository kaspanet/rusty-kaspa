use std::{cmp::max, mem::size_of, sync::Arc};

use kaspa_consensus_core::{
    acceptance_data::{MergesetBlockAcceptanceData, TxEntry},
    config::Config as ConsensusConfig,
    tx::TransactionId,
    Hash,
};
use kaspa_database::cache_policy_builder::bounded_size;
use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, TxOffset};

use crate::core::config::{
    constants::{DEFAULT_TXINDEX_DB_PARALLELISM, DEFAULT_TXINDEX_EXTRA_FD_BUDGET, DEFAULT_MAX_TXINDEX_MEMORY_BUDGET},
    params::Params,
};

#[derive(Clone, Debug)]
pub struct PerfParams {
    pub mem_budget_total: usize,
    pub resync_chunksize: usize,
    pub extra_fd_budget: usize,
    pub db_parallelism: usize,
    unit_ratio_tx_offset_to_block_acceptance_offset: usize,
}

impl PerfParams {
    pub fn new(consensus_config: &Arc<ConsensusConfig>, params: &Params) -> Self {
        let scale_factor = consensus_config.ram_scale;
        let scaled = |s| (s as f64 * scale_factor) as usize;
        
        let mem_budget_total = scaled(DEFAULT_MAX_TXINDEX_MEMORY_BUDGET);
        let resync_chunksize = scaled(bounded_size(
            params.max_blocks_in_mergeset_depth as usize,
            DEFAULT_MAX_TXINDEX_MEMORY_BUDGET,
            max(
                //per chain block
                ((size_of::<TransactionId>() + size_of::<TxOffset>()) * params.max_default_txs_per_block as usize
                    + (size_of::<BlockAcceptanceOffset>() + size_of::<Hash>()))
                    * consensus_config.params.mergeset_size_limit as usize,
                (size_of::<TxEntry>() * params.max_default_txs_per_block as usize
                    + size_of::<MergesetBlockAcceptanceData>()
                    + size_of::<Hash>())
                    * consensus_config.params.mergeset_size_limit as usize,
            ),
        ));

        Self {
            unit_ratio_tx_offset_to_block_acceptance_offset: params.max_default_txs_per_block as usize,
            resync_chunksize,
            mem_budget_total,
            extra_fd_budget: DEFAULT_TXINDEX_EXTRA_FD_BUDGET,
            db_parallelism: DEFAULT_TXINDEX_DB_PARALLELISM,
        }
    }

    pub fn mem_size_tx_offset(&self) -> usize {
        size_of::<TransactionId>() + size_of::<TxOffset>()
    }

    pub fn mem_size_block_acceptance_offset(&self) -> usize {
        size_of::<BlockAcceptanceOffset>() + size_of::<Hash>()
    }

    pub fn mem_budget_tx_offset(&self) -> usize {
        self.mem_budget_total - self.mem_budget_block_acceptance_offset()
    }

    pub fn mem_budget_block_acceptance_offset(&self) -> usize {
        self.mem_budget_total
            / ((size_of::<TransactionId>() + size_of::<TxOffset>()) * self.unit_ratio_tx_offset_to_block_acceptance_offset
                / (size_of::<Hash>() + size_of::<BlockAcceptanceOffset>()))
    }
}
