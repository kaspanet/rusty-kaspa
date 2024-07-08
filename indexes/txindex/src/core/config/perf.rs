use std::{cmp::max, mem::size_of, sync::Arc};

use kaspa_consensus::model::stores::pruning;
use kaspa_consensus_core::{
    acceptance_data::{MergesetBlockAcceptanceData, TxEntry},
    config::Config as ConsensusConfig,
    tx::TransactionId,
    Hash,
};
use kaspa_database::cache_policy_builder::bounded_size;
use kaspa_index_core::models::txindex::{BlockAcceptanceOffset, TxIndexEntry, TxOffset};

use crate::core::config::{
    constants::{DEFAULT_MAX_TXINDEX_MEMORY_BUDGET, DEFAULT_TXINDEX_DB_PARALLELISM, DEFAULT_TXINDEX_EXTRA_FD_BUDGET},
    params::Params,
};

use super::constants::DEFAULT_TXINDEX_PRUNING_BUDGET;

#[derive(Clone, Debug)]
pub struct PerfParams {
    pub mem_budget_total: usize,
    pub resync_chunksize_units: usize,
    pub pruning_chunksize_units: usize,
    pub db_budget: usize,
    pub extra_fd_budget: usize,
    pub db_parallelism: usize,
}

impl PerfParams {
    pub fn new(consensus_config: &Arc<ConsensusConfig>, params: &Params) -> Self {
        let scale_factor = consensus_config.ram_scale;
        let scaled = |s| (s as f64 * scale_factor) as usize;
        let mem_budget_total = scaled(DEFAULT_MAX_TXINDEX_MEMORY_BUDGET);
        let resync_chunksize_units = scaled(bounded_size(
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
        let pruning_chunksize_units = scaled(DEFAULT_TXINDEX_PRUNING_BUDGET / size_of::<TransactionId>() + size_of::<TxIndexEntry>());

        Self {
            resync_chunksize_units,
            pruning_chunksize_units,
            db_budget: db_cache_budget,
            mem_budget_total,
            extra_fd_budget: DEFAULT_TXINDEX_EXTRA_FD_BUDGET,
            db_parallelism: DEFAULT_TXINDEX_DB_PARALLELISM,
        }
    }

    pub fn mem_size_accepted_tx_entries(&self) -> usize {
        size_of::<TransactionId>() + size_of::<TxIndexEntry>()
    }

    pub fn mem_budget_tx_entries(&self) -> usize {
        self.db_budget / self.mem_budget_tx_entries()
    }
}
