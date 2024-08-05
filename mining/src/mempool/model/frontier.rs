use crate::{
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    model::candidate_tx::CandidateTransaction,
    Policy, RebalancingWeightedTransactionSelector,
};

use feerate_key::FeerateTransactionKey;
use kaspa_consensus_core::block::TemplateTransactionSelector;
use kaspa_core::trace;
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use search_tree::SearchTree;
use selectors::{SequenceSelector, SequenceSelectorInput, TakeAllSelector};
use std::collections::HashSet;

pub(crate) mod feerate_key;
pub(crate) mod search_tree;
pub(crate) mod selectors;

/// If the frontier contains less than 4x the block mass limit, we consider
/// inplace sampling to be less efficient (due to collisions) and thus use
/// the rebalancing selector
const COLLISION_FACTOR: u64 = 4;

/// Multiplication factor for in-place sampling. We sample 20% more than the
/// hard limit in order to allow the SequenceSelector to compensate for consensus rejections.
const MASS_LIMIT_FACTOR: f64 = 1.2;

/// A rough estimation for the average transaction mass. The usage is a non-important edge case
/// hence we just throw this here (as oppose to performing an accurate estimation)
const TYPICAL_TX_MASS: f64 = 2000.0;

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

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier.
    ///
    /// The logic is divided into three cases:
    ///     1. The frontier is small and can fit entirely into a block: perform no sampling and return
    ///        a TakeAllSelector
    ///     2. The frontier has at least ~4x the capacity of a block: expected collision rate is low, perform
    ///        in-place k*log(n) sampling and return a SequenceSelector
    ///     3. The frontier has 1-4x capacity of a block. In this case we expect a high collision rate while
    ///        the number of overall transactions is still low, so we take all of the transactions and use the
    ///        rebalancing weighted selector (performing the actual sampling out of the mempool lock)
    ///
    /// The above thresholds were selected based on benchmarks. Overall, this dynamic selection provides
    /// full transaction selection in less than 150 µs even if the frontier has 1M entries (!!). See mining/benches
    /// for more details.  
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

    /// Builds a feerate estimator based on internal state of the ready transactions frontier
    pub fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        let average_transaction_mass = match self.len() {
            0 => TYPICAL_TX_MASS,
            n => self.total_mass() as f64 / n as f64,
        };
        let bps = args.network_blocks_per_second as f64;
        let mut mass_per_block = args.maximum_mass_per_block as f64;
        let mut inclusion_interval = average_transaction_mass / (mass_per_block * bps);
        let mut estimator = FeerateEstimator::new(self.total_weight(), inclusion_interval);

        // Corresponds to the removal of the top item, hence the skip(1) below
        mass_per_block -= average_transaction_mass;
        inclusion_interval = average_transaction_mass / (mass_per_block * bps);

        // Search for better estimators by possibly removing extremely high outliers
        for key in self.search_tree.descending_iter().skip(1) {
            // Compute the weight up to, and including, current key
            let prefix_weight = self.search_tree.prefix_weight(key);
            let pending_estimator = FeerateEstimator::new(prefix_weight, inclusion_interval);

            // Test the pending estimator vs. the current one
            if pending_estimator.feerate_to_time(1.0) < estimator.feerate_to_time(1.0) {
                estimator = pending_estimator;
            } else {
                // The pending estimator is no better, break. Indicates that the reduction in
                // network mass per second is more significant than the removed weight
                break;
            }

            // Update values for the next iteration. In order to remove the outlier from the
            // total weight, we must compensate by capturing a block slot.
            mass_per_block -= average_transaction_mass;
            if mass_per_block <= 0.0 {
                // Out of block slots, break (this is rarely reachable code due to dynamics related to the above break)
                break;
            }

            // Re-calc the inclusion interval based on the new block "capacity"
            inclusion_interval = average_transaction_mass / (mass_per_block * bps);
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

    #[test]
    fn test_feerate_estimator() {
        let mut rng = thread_rng();
        let cap = 2000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let mut fee: u64 = rng.gen_range(1..1000000);
            let mass: u64 = 1650;
            // 304 (~500,000/1650) extreme outliers is an edge case where the build estimator logic should be tested at
            if i <= 303 {
                // Add an extremely large fee in order to create extremely high variance
                fee = i * 10_000_000 * 1_000_000;
            }
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        for len in [10, 100, 200, 300, 500, 750, cap / 2, (cap * 2) / 3, (cap * 4) / 5, (cap * 5) / 6, cap] {
            let mut frontier = Frontier::default();
            for item in map.values().take(len).cloned() {
                frontier.insert(item).then_some(()).unwrap();
            }

            let args = FeerateEstimatorArgs { network_blocks_per_second: 1, maximum_mass_per_block: 500_000 };
            // We are testing that the build function actually returns and is not looping indefinitely
            let estimator = frontier.build_feerate_estimator(args);
            let _estimations = estimator.calc_estimations();
            // dbg!(_estimations);
        }
    }
}
