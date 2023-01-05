use crate::model::owner_txs::{OwnerSetTransactions, ScriptPublicKeySet};

use self::{
    config::Config,
    model::{orphan_pool::OrphanPool, pool::Pool, transactions_pool::TransactionsPool, utxo_set::MempoolUtxoSet},
};
use consensus_core::{
    api::DynConsensus,
    tx::{MutableTransaction, TransactionId},
};
use std::rc::Rc;

pub(crate) mod check_transaction_standard;
pub mod config;
pub mod errors;
pub(crate) mod fill_inputs_and_get_missing_parents;
pub(crate) mod handle_new_block_transactions;
mod model;
pub(crate) mod remove_transaction;
pub(crate) mod revalidate_high_priority_transactions;
pub(crate) mod validate_and_insert_transaction;

pub(crate) struct Mempool {
    pub(crate) config: Rc<Config>,
    pub(crate) consensus: DynConsensus,
    pub(crate) mempool_utxo_set: MempoolUtxoSet,
    pub(crate) transaction_pool: TransactionsPool,
    pub(crate) orphan_pool: OrphanPool,
}

impl Mempool {
    pub(crate) fn new(
        consensus: DynConsensus,
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
    ) -> Self {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(consensus, config)
    }

    pub(crate) fn with_config(consensus: DynConsensus, config: Config) -> Self {
        let config = Rc::new(config);
        let mempool_utxo_set = MempoolUtxoSet::new();
        let transaction_pool = TransactionsPool::new(consensus.clone(), config.clone());
        let orphan_pool = OrphanPool::new(consensus.clone(), config.clone());
        Self { config, consensus, mempool_utxo_set, transaction_pool, orphan_pool }
    }

    pub(crate) fn consensus(&self) -> DynConsensus {
        self.consensus.clone()
    }

    pub(crate) fn get_transaction(
        &self,
        transaction_id: &TransactionId,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> Option<MutableTransaction> {
        let mut transaction = None;
        if include_transaction_pool {
            transaction = self.transaction_pool.all().get(transaction_id);
        }
        if transaction.is_none() && include_orphan_pool {
            transaction = self.orphan_pool.get(transaction_id);
        }
        transaction.map(|x| x.mtx.clone())
    }

    pub(crate) fn get_all_transactions(
        &self,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        let mut transactions = vec![];
        let mut orphans = vec![];
        if include_transaction_pool {
            transactions = self.transaction_pool.get_all_transactions()
        }
        if include_orphan_pool {
            orphans = self.orphan_pool.get_all_transactions()
        }
        (transactions, orphans)
    }

    pub(crate) fn get_transactions_by_addresses(
        &self,
        script_public_keys: &ScriptPublicKeySet,
        include_transaction_pool: bool,
        include_orphan_pool: bool,
    ) -> OwnerSetTransactions {
        let mut owner_set = OwnerSetTransactions::default();
        if include_transaction_pool {
            self.transaction_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        if include_orphan_pool {
            self.orphan_pool.fill_owner_set_transactions(script_public_keys, &mut owner_set);
        }
        owner_set
    }

    pub(crate) fn transaction_count(&self, include_transaction_pool: bool, include_orphan_pool: bool) -> usize {
        let mut count = 0;
        if include_transaction_pool {
            count += self.transaction_pool.len()
        }
        if include_orphan_pool {
            count += self.orphan_pool.len()
        }
        count
    }

    pub(crate) fn block_candidate_transactions(&self) -> Vec<MutableTransaction> {
        self.transaction_pool.get_all_transactions()
    }
}
