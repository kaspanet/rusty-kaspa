use crate::{
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    model::candidate_tx::CandidateTransaction,
    Policy, RebalancingWeightedTransactionSelector,
};

use feerate_key::FeerateTransactionKey;
use feerate_weight::SearchTree;
use kaspa_consensus_core::block::TemplateTransactionSelector;
use kaspa_core::trace;
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use selectors::{SequenceSelector, SequenceSelectorInput, TakeAllSelector};
use std::collections::HashSet;

pub(crate) mod feerate_key;
pub(crate) mod feerate_weight;
pub(crate) mod selectors;

/// If the frontier contains less than 4x the block mass limit, we consider
/// inplace sampling to be less efficient (due to collisions) and thus use
/// the rebalancing selector
const COLLISION_FACTOR: u64 = 4;

/// Multiplication factor for in-place sampling. We sample 20% more than the
/// hard limit in order to allow the SequenceSelector to compensate for consensus rejections.
const MASS_LIMIT_FACTOR: f64 = 1.2;

/// Management of the transaction pool frontier, that is, the set of transactions in
/// the transaction pool which have no mempool ancestors and are essentially ready
/// to enter the next block template.
#[derive(Default)]
pub struct Frontier {
    /// Frontier transactions sorted by feerate order and searchable for weight sampling
    search_tree: SearchTree,

    /// Total masses: Σ_{tx in frontier} tx.mass
    total_mass: u64,
}

impl Frontier {
    pub fn total_weight(&self) -> f64 {
        self.search_tree.total_weight()
    }

    pub fn total_mass(&self) -> u64 {
        self.total_mass
    }

    pub fn len(&self) -> usize {
        self.search_tree.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn insert(&mut self, key: FeerateTransactionKey) -> bool {
        let mass = key.mass;
        if self.search_tree.insert(key) {
            self.total_mass += mass;
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, key: &FeerateTransactionKey) -> bool {
        let mass = key.mass;
        if self.search_tree.remove(key) {
            self.total_mass -= mass;
            true
        } else {
            false
        }
    }

    pub fn sample_inplace<R>(&self, rng: &mut R, policy: &Policy) -> SequenceSelectorInput
    where
        R: Rng + ?Sized,
    {
        debug_assert!(!self.search_tree.is_empty(), "expected to be called only if not empty");

        // Sample 20% more than the hard limit in order to allow the SequenceSelector to
        // compensate for consensus rejections.
        // Note: this is a soft limit which is why the loop below might pass it if the
        //       next sampled transaction happens to cross the bound
        let extended_mass_limit = (policy.max_block_mass as f64 * MASS_LIMIT_FACTOR) as u64;

        let mut distr = Uniform::new(0f64, self.total_weight());
        let mut down_iter = self.search_tree.descending_iter();
        let mut top = down_iter.next().unwrap();
        let mut cache = HashSet::new();
        let mut sequence = SequenceSelectorInput::default();
        let mut total_selected_mass: u64 = 0;
        let mut _collisions = 0;

        // The sampling process is converging thus the cache will hold all entries eventually, which guarantees loop exit
        'outer: while cache.len() < self.search_tree.len() && total_selected_mass <= extended_mass_limit {
            let query = distr.sample(rng);
            let item = {
                let mut item = self.search_tree.search(query);
                while !cache.insert(item.tx.id()) {
                    _collisions += 1;
                    if top == item {
                        // Narrow the search to reduce further sampling collisions
                        match down_iter.next() {
                            Some(next) => top = next,
                            None => break 'outer,
                        }
                        let remaining_weight = self.search_tree.prefix_weight(top);
                        distr = Uniform::new(0f64, remaining_weight);
                    }
                    let query = distr.sample(rng);
                    item = self.search_tree.search(query);
                }
                item
            };
            sequence.push(item.tx.clone(), item.mass);
            total_selected_mass += item.mass; // Max standard mass + Mempool capacity bound imply this will not overflow
        }
        trace!("[mempool frontier sample inplace] collisions: {_collisions}, cache: {}", cache.len());
        sequence
    }

    pub fn build_selector(&self, policy: &Policy) -> Box<dyn TemplateTransactionSelector> {
        if self.total_mass <= policy.max_block_mass {
            Box::new(TakeAllSelector::new(self.search_tree.ascending_iter().map(|k| k.tx.clone()).collect()))
        } else if self.total_mass > policy.max_block_mass * COLLISION_FACTOR {
            let mut rng = rand::thread_rng();
            Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, policy), policy.clone()))
        } else {
            Box::new(RebalancingWeightedTransactionSelector::new(
                policy.clone(),
                self.search_tree.ascending_iter().cloned().map(CandidateTransaction::from_key).collect(),
            ))
        }
    }

    /// Exposed for benchmarking purposes
    pub fn build_selector_sample_inplace(&self) -> Box<dyn TemplateTransactionSelector> {
        let mut rng = rand::thread_rng();
        let policy = Policy::new(500_000);
        Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, &policy), policy))
    }

    /// Exposed for benchmarking purposes
    pub fn build_selector_take_all(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(TakeAllSelector::new(self.search_tree.ascending_iter().map(|k| k.tx.clone()).collect()))
    }

    /// Exposed for benchmarking purposes
    pub fn build_rebalancing_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(RebalancingWeightedTransactionSelector::new(
            Policy::new(500_000),
            self.search_tree.ascending_iter().cloned().map(CandidateTransaction::from_key).collect(),
        ))
    }

    pub fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        let mut total_mass = self.total_mass();
        let mut mass_per_second = args.network_mass_per_second;
        let mut count = self.len();
        let mut average_transaction_mass = match self.len() {
            // TODO (PR): remove consts
            0 => 500_000.0 / 300.0,
            n => total_mass as f64 / n as f64,
        };
        let mut inclusion_interval = average_transaction_mass / mass_per_second as f64;
        let mut estimator = FeerateEstimator::new(self.total_weight(), inclusion_interval);

        // Search for better estimators by possibly removing extremely high outliers
        for key in self.search_tree.descending_iter() {
            // TODO (PR): explain the importance of this visitor for numerical stability
            let prefix_weight = self.search_tree.prefix_weight(key);
            let pending_estimator = FeerateEstimator::new(prefix_weight, inclusion_interval);

            // Test the pending estimator vs the current one
            if pending_estimator.feerate_to_time(1.0) < estimator.feerate_to_time(1.0) {
                estimator = pending_estimator;
            }

            // Update values for the next iteration
            count -= 1;
            total_mass -= key.mass;
            mass_per_second -= key.mass; // TODO (PR): remove per block? lower bound?
            average_transaction_mass = total_mass as f64 / count as f64;
            inclusion_interval = average_transaction_mass / mass_per_second as f64
        }
        estimator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feerate_key::tests::build_feerate_key;
    use rand::thread_rng;
    use std::collections::HashMap;

    #[test]
    pub fn test_highly_irregular_sampling() {
        let mut rng = thread_rng();
        let cap = 1000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let mut fee: u64 = if i % (cap as u64 / 100) == 0 { 1000000 } else { rng.gen_range(1..10000) };
            if i == 0 {
                // Add an extremely large fee in order to create extremely high variance
                fee = 100_000_000 * 1_000_000; // 1M KAS
            }
            let mass: u64 = 1650;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let len = cap;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let _sample = frontier.sample_inplace(&mut rng, &Policy::new(500_000));
        // assert_eq!(100, sample.len());
    }

    #[test]
    pub fn test_mempool_sampling_small() {
        let mut rng = thread_rng();
        let cap = 2000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = rng.gen_range(1..1000000);
            let mass: u64 = 1650;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let len = cap;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let mut selector = frontier.build_selector(&Policy::new(500_000));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_rebalancing_selector();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_sample_inplace();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_take_all();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector(&Policy::new(500_000));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();
    }
}
