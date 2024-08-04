use super::FrontierTree;
use crate::{FeerateTransactionKey, Policy};
use indexmap::IndexMap;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{Transaction, TransactionId},
};
use rand::Rng;
use std::{collections::HashMap, sync::Arc};

pub struct WeightTreeSelector {
    search_tree: FrontierTree,
    mass_map: HashMap<TransactionId, u64>,
    total_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
    policy: Policy,
}

impl WeightTreeSelector {
    pub fn new(search_tree: FrontierTree, policy: Policy) -> Self {
        Self {
            search_tree,
            mass_map: Default::default(),
            total_mass: Default::default(),
            overall_candidates: Default::default(),
            overall_rejections: Default::default(),
            policy,
        }
    }

    pub fn total_weight(&self) -> f64 {
        self.search_tree.root_argument().weight()
    }
}

impl TemplateTransactionSelector for WeightTreeSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        let mut rng = rand::thread_rng();
        let mut transactions = Vec::new();
        self.mass_map.clear();

        while self.total_mass <= self.policy.max_block_mass {
            if self.search_tree.is_empty() {
                break;
            }
            let query = rng.gen_range(0.0..self.total_weight());
            let (key, _) = self.search_tree.remove_by_argument(query).unwrap();
            if self.total_mass.saturating_add(key.mass) > self.policy.max_block_mass {
                break; // TODO: or break
            }
            self.mass_map.insert(key.tx.id(), key.mass);
            self.total_mass += key.mass;
            transactions.push(key.tx.as_ref().clone())
        }

        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        let mass = self.mass_map.get(&tx_id).unwrap();
        self.total_mass -= mass;
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.79;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 79% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

pub struct SequenceSelector {
    map: IndexMap<TransactionId, FeerateTransactionKey>,
    total_mass: u64,
    // overall_candidates: usize,
    // overall_rejections: usize,
    policy: Policy,
}

impl SequenceSelector {
    pub fn new(map: IndexMap<TransactionId, FeerateTransactionKey>, policy: Policy) -> Self {
        Self {
            map,
            total_mass: Default::default(),
            // overall_candidates: Default::default(),
            // overall_rejections: Default::default(),
            policy,
        }
    }
}

impl TemplateTransactionSelector for SequenceSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        let mut transactions = Vec::new();
        for (_, key) in self.map.iter() {
            if self.total_mass.saturating_add(key.mass) > self.policy.max_block_mass {
                break; // TODO
            }
            // self.mass_map.insert(key.tx.id(), key.mass);
            self.total_mass += key.mass;
            transactions.push(key.tx.as_ref().clone())
        }
        transactions
    }

    fn reject_selection(&mut self, _tx_id: TransactionId) {
        todo!()
    }

    fn is_successful(&self) -> bool {
        todo!()
    }
}

pub struct TakeAllSelector {
    txs: Vec<Arc<Transaction>>,
}

impl TakeAllSelector {
    pub fn new(txs: Vec<Arc<Transaction>>) -> Self {
        Self { txs }
    }
}

impl TemplateTransactionSelector for TakeAllSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.txs.drain(..).map(|tx| tx.as_ref().clone()).collect()
    }

    fn reject_selection(&mut self, _tx_id: TransactionId) {}

    fn is_successful(&self) -> bool {
        true
    }
}
