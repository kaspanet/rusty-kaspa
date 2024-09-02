use crate::{
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    model::candidate_tx::CandidateTransaction,
    Policy, RebalancingWeightedTransactionSelector,
};

use feerate_key::FeerateTransactionKey;
use kaspa_consensus_core::{block::TemplateTransactionSelector, tx::Transaction};
use kaspa_core::trace;
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use search_tree::SearchTree;
use selectors::{SequenceSelector, SequenceSelectorInput, TakeAllSelector};
use std::{collections::HashSet, iter::FusedIterator, sync::Arc};

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

    /// Samples the frontier in-place based on the provided policy and returns a SequenceSelector.
    ///
    /// This sampling algorithm should be used when frontier total mass is high enough compared to
    /// policy mass limit so that the probability of sampling collisions remains low.
    ///
    /// Convergence analysis:
    ///     1. Based on the above we can safely assume that `k << n`, where `n` is the total number of frontier items
    ///        and `k` is the number of actual samples (since `desired_mass << total_mass` and mass per item is bounded)
    ///     2. Indeed, if the weight distribution is not too spread (i.e., `max(weights) = O(min(weights))`), `k << n` means
    ///        that the probability of collisions is low enough and the sampling process will converge in `O(k log(n))` w.h.p.
    ///     3. It remains to deal with the case where the weight distribution is highly biased. The process implemented below
    ///        keeps track of the top-weight element. If the distribution is highly biased, this element will be sampled with
    ///        sufficient probability (in constant time). Following each sampling collision we search for a consecutive range of
    ///        top elements which were already sampled and narrow the sampling space to exclude them all. We do this by computing
    ///        the prefix weight up to the top most item which wasn't sampled yet (inclusive) and then continue the sampling process
    ///        over the narrowed space. This process is repeated until acquiring the desired mass.  
    ///     4. Numerical stability. Naively, one would simply subtract `total_weight -= top.weight` in order to narrow the sampling
    ///        space. However, if `top.weight` is much larger than the remaining weight, the above f64 subtraction will yield a number
    ///        close or equal to zero. We fix this by implementing a `log(n)` prefix weight operation.
    ///     5. Q. Why not just use u64 weights?
    ///        A. The current weight calculation is `feerate^alpha` with `alpha=3`. Using u64 would mean that the feerate space
    ///           is limited to a range of size `(2^64)^(1/3) = ~2^21 = ~2M`. Already with current usages, the feerate can vary
    ///           from `~1/50` (2000 sompi for a transaction with 100K storage mass), to `5M` (100 KAS fee for a transaction with
    ///           2000 mass = 100·100_000_000/2000), resulting in a range of size 250M (`5M/(1/50)`).
    ///           By using floating point arithmetics we gain the adjustment of the probability space to the accuracy level required for
    ///           current samples. And if the space is highly biased, the repeated elimination of top items and the prefix weight computation
    ///           will readjust it.
    pub fn sample_inplace<R>(&self, rng: &mut R, policy: &Policy, _collisions: &mut u64) -> SequenceSelectorInput
    where
        R: Rng + ?Sized,
    {
        debug_assert!(!self.search_tree.is_empty(), "expected to be called only if not empty");

        // Sample 20% more than the hard limit in order to allow the SequenceSelector to
        // compensate for consensus rejections.
        // Note: this is a soft limit which is why the loop below might pass it if the
        //       next sampled transaction happens to cross the bound
        let desired_mass = (policy.max_block_mass as f64 * MASS_LIMIT_FACTOR) as u64;

        let mut distr = Uniform::new(0f64, self.total_weight());
        let mut down_iter = self.search_tree.descending_iter();
        let mut top = down_iter.next().unwrap();
        let mut cache = HashSet::new();
        let mut sequence = SequenceSelectorInput::default();
        let mut total_selected_mass: u64 = 0;
        let mut collisions = 0;

        // The sampling process is converging so the cache will eventually hold all entries, which guarantees loop exit
        'outer: while cache.len() < self.search_tree.len() && total_selected_mass <= desired_mass {
            let query = distr.sample(rng);
            let item = {
                let mut item = self.search_tree.search(query);
                while !cache.insert(item.tx.id()) {
                    collisions += 1;
                    // Try to narrow the sampling space in order to reduce further sampling collisions
                    if cache.contains(&top.tx.id()) {
                        loop {
                            match down_iter.next() {
                                Some(next) => top = next,
                                None => break 'outer,
                            }
                            // Loop until finding a top item which was not sampled yet
                            if !cache.contains(&top.tx.id()) {
                                break;
                            }
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
        trace!("[mempool frontier sample inplace] collisions: {collisions}, cache: {}", cache.len());
        *_collisions += collisions;
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
            Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, policy, &mut 0), policy.clone()))
        } else {
            Box::new(RebalancingWeightedTransactionSelector::new(
                policy.clone(),
                self.search_tree.ascending_iter().cloned().map(CandidateTransaction::from_key).collect(),
            ))
        }
    }

    /// Exposed for benchmarking purposes
    pub fn build_selector_sample_inplace(&self, _collisions: &mut u64) -> Box<dyn TemplateTransactionSelector> {
        let mut rng = rand::thread_rng();
        let policy = Policy::new(500_000);
        Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, &policy, _collisions), policy))
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

        // Search for better estimators by possibly removing extremely high outliers
        let mut down_iter = self.search_tree.descending_iter().peekable();
        while let Some(current) = down_iter.next() {
            // Update values for the coming iteration. In order to remove the outlier from the
            // total weight, we must compensate by capturing a block slot. Note we capture the
            // slot with correspondence to the outlier actual mass. This is important in cases
            // where the high-feerate txs have mass which deviates from the average.
            mass_per_block -= current.mass as f64;
            if mass_per_block <= average_transaction_mass {
                // Out of block slots, break
                break;
            }

            // Re-calc the inclusion interval based on the new block "capacity".
            // Note that inclusion_interval < 1.0 as required by the estimator, since mass_per_block > average_transaction_mass (by condition above) and bps >= 1
            inclusion_interval = average_transaction_mass / (mass_per_block * bps);

            // Compute the weight up to, and excluding, current key (which translates to zero weight if peek() is none)
            let prefix_weight = down_iter.peek().map(|key| self.search_tree.prefix_weight(key)).unwrap_or_default();
            let pending_estimator = FeerateEstimator::new(prefix_weight, inclusion_interval);

            // Test the pending estimator vs. the current one
            if pending_estimator.feerate_to_time(1.0) < estimator.feerate_to_time(1.0) {
                estimator = pending_estimator;
            } else {
                // The pending estimator is no better, break. Indicates that the reduction in
                // network mass per second is more significant than the removed weight
                break;
            }
        }
        estimator
    }

    /// Returns an iterator to the transactions in the frontier in increasing feerate order
    pub fn ascending_iter(&self) -> impl DoubleEndedIterator<Item = &Arc<Transaction>> + ExactSizeIterator + FusedIterator {
        self.search_tree.ascending_iter().map(|key| &key.tx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use feerate_key::tests::build_feerate_key;
    use itertools::Itertools;
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

        let mut frontier = Frontier::default();
        for item in map.values().cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let _sample = frontier.sample_inplace(&mut rng, &Policy::new(500_000), &mut 0);
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

        let mut frontier = Frontier::default();
        for item in map.values().cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let mut selector = frontier.build_selector(&Policy::new(500_000));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_rebalancing_selector();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_sample_inplace(&mut 0);
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_take_all();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector(&Policy::new(500_000));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();
    }

    #[test]
    pub fn test_total_mass_tracking() {
        let mut rng = thread_rng();
        let cap = 10000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let fee: u64 = if i % (cap as u64 / 100) == 0 { 1000000 } else { rng.gen_range(1..10000) };
            let mass: u64 = rng.gen_range(1..100000); // Use distinct mass values to challenge the test
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        let len = cap / 2;
        let mut frontier = Frontier::default();
        for item in map.values().take(len).cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let prev_total_mass = frontier.total_mass();
        // Assert the total mass
        assert_eq!(frontier.total_mass(), frontier.search_tree.ascending_iter().map(|k| k.mass).sum::<u64>());

        // Add a bunch of duplicates and make sure the total mass remains the same
        let mut dup_items = frontier.search_tree.ascending_iter().take(len / 2).cloned().collect_vec();
        for dup in dup_items.iter().cloned() {
            (!frontier.insert(dup)).then_some(()).unwrap();
        }
        assert_eq!(prev_total_mass, frontier.total_mass());
        assert_eq!(frontier.total_mass(), frontier.search_tree.ascending_iter().map(|k| k.mass).sum::<u64>());

        // Remove a few elements from the map in order to randomize the iterator
        dup_items.iter().take(10).for_each(|k| {
            map.remove(&k.tx.id());
        });

        // Add and remove random elements some of which will be duplicate insertions and some missing removals
        for item in map.values().step_by(2) {
            frontier.remove(item);
            if let Some(item2) = dup_items.pop() {
                frontier.insert(item2);
            }
        }
        assert_eq!(frontier.total_mass(), frontier.search_tree.ascending_iter().map(|k| k.mass).sum::<u64>());
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

        for len in [0, 1, 10, 100, 200, 300, 500, 750, cap / 2, (cap * 2) / 3, (cap * 4) / 5, (cap * 5) / 6, cap] {
            let mut frontier = Frontier::default();
            for item in map.values().take(len).cloned() {
                frontier.insert(item).then_some(()).unwrap();
            }

            let args = FeerateEstimatorArgs { network_blocks_per_second: 1, maximum_mass_per_block: 500_000 };
            // We are testing that the build function actually returns and is not looping indefinitely
            let estimator = frontier.build_feerate_estimator(args);
            let estimations = estimator.calc_estimations(1.0);

            let buckets = estimations.ordered_buckets();
            // Test for the absence of NaN, infinite or zero values in buckets
            for b in buckets.iter() {
                assert!(
                    b.feerate.is_normal() && b.feerate >= 1.0,
                    "bucket feerate must be a finite number greater or equal to the minimum standard feerate"
                );
                assert!(
                    b.estimated_seconds.is_normal() && b.estimated_seconds > 0.0,
                    "bucket estimated seconds must be a finite number greater than zero"
                );
            }
            dbg!(len, estimator);
            dbg!(estimations);
        }
    }

    #[test]
    fn test_constant_feerate_estimator() {
        const MIN_FEERATE: f64 = 1.0;
        let cap = 20_000;
        let mut map = HashMap::with_capacity(cap);
        for i in 0..cap as u64 {
            let mass: u64 = 1650;
            let fee = (mass as f64 * MIN_FEERATE) as u64;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        for len in [0, 1, 10, 100, 200, 300, 500, 750, cap / 2, (cap * 2) / 3, (cap * 4) / 5, (cap * 5) / 6, cap] {
            println!();
            println!("Testing a frontier with {} txs...", len.min(cap));
            let mut frontier = Frontier::default();
            for item in map.values().take(len).cloned() {
                frontier.insert(item).then_some(()).unwrap();
            }

            let args = FeerateEstimatorArgs { network_blocks_per_second: 1, maximum_mass_per_block: 500_000 };
            // We are testing that the build function actually returns and is not looping indefinitely
            let estimator = frontier.build_feerate_estimator(args);
            let estimations = estimator.calc_estimations(MIN_FEERATE);
            let buckets = estimations.ordered_buckets();
            // Test for the absence of NaN, infinite or zero values in buckets
            for b in buckets.iter() {
                assert!(
                    b.feerate.is_normal() && b.feerate >= MIN_FEERATE,
                    "bucket feerate must be a finite number greater or equal to the minimum standard feerate"
                );
                assert!(
                    b.estimated_seconds.is_normal() && b.estimated_seconds > 0.0,
                    "bucket estimated seconds must be a finite number greater than zero"
                );
            }
            dbg!(len, estimator);
            dbg!(estimations);
        }
    }

    #[test]
    fn test_feerate_estimator_with_low_mass_outliers() {
        const MIN_FEERATE: f64 = 1.0;
        const STD_FEERATE: f64 = 10.0;
        const HIGH_FEERATE: f64 = 1000.0;

        let cap = 20_000;
        let mut frontier = Frontier::default();
        for i in 0..cap as u64 {
            let (mass, fee) = if i < 200 {
                let mass = 1650;
                (mass, (HIGH_FEERATE * mass as f64) as u64)
            } else {
                let mass = 90_000;
                (mass, (STD_FEERATE * mass as f64) as u64)
            };
            let key = build_feerate_key(fee, mass, i);
            frontier.insert(key).then_some(()).unwrap();
        }

        let args = FeerateEstimatorArgs { network_blocks_per_second: 1, maximum_mass_per_block: 500_000 };
        // We are testing that the build function actually returns and is not looping indefinitely
        let estimator = frontier.build_feerate_estimator(args);
        let estimations = estimator.calc_estimations(MIN_FEERATE);

        // Test that estimations are not biased by the average high mass
        let normal_feerate = estimations.normal_buckets.first().unwrap().feerate;
        assert!(
            normal_feerate < HIGH_FEERATE / 10.0,
            "Normal bucket feerate is expected to be << high feerate due to small mass of high feerate txs ({}, {})",
            normal_feerate,
            HIGH_FEERATE
        );

        let buckets = estimations.ordered_buckets();
        // Test for the absence of NaN, infinite or zero values in buckets
        for b in buckets.iter() {
            assert!(
                b.feerate.is_normal() && b.feerate >= MIN_FEERATE,
                "bucket feerate must be a finite number greater or equal to the minimum standard feerate"
            );
            assert!(
                b.estimated_seconds.is_normal() && b.estimated_seconds > 0.0,
                "bucket estimated seconds must be a finite number greater than zero"
            );
        }
        dbg!(estimator);
        dbg!(estimations);
    }

    #[test]
    fn test_feerate_estimator_with_less_than_block_capacity() {
        let mut map = HashMap::new();
        for i in 0..304 {
            let mass: u64 = 1650;
            let fee = 10_000_000 * 1_000_000;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        // All lens make for less than block capacity (given the mass used)
        for len in [0, 1, 10, 100, 200, 250, 300] {
            let mut frontier = Frontier::default();
            for item in map.values().take(len).cloned() {
                frontier.insert(item).then_some(()).unwrap();
            }

            let args = FeerateEstimatorArgs { network_blocks_per_second: 1, maximum_mass_per_block: 500_000 };
            // We are testing that the build function actually returns and is not looping indefinitely
            let estimator = frontier.build_feerate_estimator(args);
            let estimations = estimator.calc_estimations(1.0);

            let buckets = estimations.ordered_buckets();
            // Test for the absence of NaN, infinite or zero values in buckets
            for b in buckets.iter() {
                // Expect min feerate bcs blocks are not full
                assert!(b.feerate == 1.0, "bucket feerate is expected to be equal to the minimum standard feerate");
                assert!(
                    b.estimated_seconds.is_normal() && b.estimated_seconds > 0.0 && b.estimated_seconds <= 1.0,
                    "bucket estimated seconds must be a finite number greater than zero & less than 1.0"
                );
            }
            dbg!(len, estimator);
            dbg!(estimations);
        }
    }
}
