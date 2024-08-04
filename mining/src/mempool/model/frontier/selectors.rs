use super::FrontierTree;
use crate::Policy;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{Transaction, TransactionId},
};
use rand::Rng;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

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

pub struct SequenceSelectorTransaction {
    pub tx: Arc<Transaction>,
    pub mass: u64,
}

impl SequenceSelectorTransaction {
    pub fn new(tx: Arc<Transaction>, mass: u64) -> Self {
        Self { tx, mass }
    }
}

pub type SequenceSelectorPriorityIndex = u32;

pub type SequenceSelectorPriorityMap = BTreeMap<SequenceSelectorPriorityIndex, SequenceSelectorTransaction>;

/// A selector which selects transactions in the order they are provided. The selector assumes
/// that the transactions were already selected via weighted sampling and simply tries them one
/// after the other until the block mass limit is reached.  
pub struct SequenceSelector {
    priority_map: SequenceSelectorPriorityMap,
    selected: HashMap<TransactionId, (u64, SequenceSelectorPriorityIndex)>,
    total_selected_mass: u64,
    overall_candidates: usize,
    overall_rejections: usize,
    policy: Policy,
}

impl SequenceSelector {
    pub fn new(priority_map: SequenceSelectorPriorityMap, policy: Policy) -> Self {
        Self {
            overall_candidates: priority_map.len(),
            priority_map,
            selected: Default::default(),
            total_selected_mass: Default::default(),
            overall_rejections: Default::default(),
            policy,
        }
    }
}

impl TemplateTransactionSelector for SequenceSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        self.selected.clear();
        let mut transactions = Vec::new();
        for (&priority, tx) in self.priority_map.iter() {
            if self.total_selected_mass.saturating_add(tx.mass) > self.policy.max_block_mass {
                // We assume the sequence is relatively small, hence we keep on searching
                // for transactions with lower mass which might fit into the remaining gap
                continue;
            }
            self.total_selected_mass += tx.mass;
            self.selected.insert(tx.tx.id(), (tx.mass, priority));
            transactions.push(tx.tx.as_ref().clone())
        }
        for (_, priority) in self.selected.values() {
            self.priority_map.remove(priority);
        }
        transactions
    }

    fn reject_selection(&mut self, tx_id: TransactionId) {
        let &(mass, _) = self.selected.get(&tx_id).expect("only previously selected txs can be rejected (and only once)");
        self.total_selected_mass -= mass;
        self.overall_rejections += 1;
    }

    fn is_successful(&self) -> bool {
        const SUFFICIENT_MASS_THRESHOLD: f64 = 0.8;
        const LOW_REJECTION_FRACTION: f64 = 0.2;

        // We consider the operation successful if either mass occupation is above 80% or rejection rate is below 20%
        self.overall_rejections == 0
            || (self.total_selected_mass as f64) > self.policy.max_block_mass as f64 * SUFFICIENT_MASS_THRESHOLD
            || (self.overall_rejections as f64) < self.overall_candidates as f64 * LOW_REJECTION_FRACTION
    }
}

/// A selector that selects all the transactions it holds and is always considered successful.
/// If all mempool transactions have combined mass which is <= block mass limit, this selector
/// should be called and provided with all the transactions.
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
        // Drain on the first call so that subsequent calls return nothing
        self.txs.drain(..).map(|tx| tx.as_ref().clone()).collect()
    }

    fn reject_selection(&mut self, _tx_id: TransactionId) {
        // No need to track rejections (for reduced mass), since there's nothing else to select
    }

    fn is_successful(&self) -> bool {
        // Considered successful because we provided all mempool transactions to this
        // selector, so there's point in retries
        true
    }
}
