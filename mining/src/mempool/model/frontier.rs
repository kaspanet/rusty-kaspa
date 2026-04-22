use crate::{
    Policy, RebalancingWeightedTransactionSelector,
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    model::candidate_tx::CandidateTransaction,
};

use feerate_key::FeerateTransactionKey;
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    config::constants::consensus::{DEFAULT_GAS_PER_LANE_LIMIT, DEFAULT_LANES_PER_BLOCK_LIMIT},
    mass::BlockLaneLimits,
    subnets::SubnetworkId,
    tx::{Transaction, TransactionId},
};
use kaspa_core::trace;
use rand::{Rng, distributions::Uniform, prelude::Distribution};
use search_tree::SearchTree;
use selectors::{MutatingTreeSelector, SequenceSelector, SequenceSelectorInput, TakeAllSelector};
use std::{
    collections::{BTreeSet, BinaryHeap, HashMap, HashSet},
    iter::FusedIterator,
    sync::Arc,
};

pub(crate) mod feerate_key;
pub(crate) mod search_tree;
pub(crate) mod selectors;

/// If the frontier contains less than 4x the block mass limit, we consider
/// inplace sampling to be less efficient (due to collisions) and thus use
/// the mutating-tree selector
const COLLISION_FACTOR: u64 = 4;

/// Multiplication factor for in-place sampling. We sample 20% more than the
/// hard limit in order to allow the SequenceSelector to compensate for consensus rejections.
const MASS_LIMIT_FACTOR: f64 = 1.2;

/// Extra sampling stops once the greedy pack gap is at most this fraction of the block mass.
const TARGET_GAP_FACTOR: f64 = 0.05;

/// Bounds extra sampling caused by large transactions which increase sampled mass
/// but do not help the later greedy SequenceSelector fill the block.
const MAX_NULL_ATTEMPTS: usize = 8;

/// Initial estimation of the average transaction mass.
const INITIAL_AVG_MASS: f64 = 2036.0;

/// Decay factor of average mass weighting.
const AVG_MASS_DECAY_FACTOR: f64 = 0.99999;

const DEFAULT_BLOCK_LANE_LIMITS: BlockLaneLimits =
    BlockLaneLimits { lanes_per_block: DEFAULT_LANES_PER_BLOCK_LIMIT, gas_per_lane: DEFAULT_GAS_PER_LANE_LIMIT };

/// Management of the transaction pool frontier, that is, the set of transactions in
/// the transaction pool which have no mempool ancestors and are essentially ready
/// to enter the next block template.
pub struct Frontier {
    /// Frontier transactions sorted by feerate order and searchable for weight sampling
    search_tree: SearchTree,

    /// Frontier transactions additionally grouped by lane for post-LPB capped selection
    by_lane: HashMap<SubnetworkId, BTreeSet<FeerateTransactionKey>>,

    /// Total masses: Σ_{tx in frontier} tx.mass
    total_mass: u64,

    /// Tracks the average transaction mass throughout the mempool's lifespan using a decayed weighting mechanism
    average_transaction_mass: f64,

    target_time_per_block_seconds: f64,
}

#[derive(Default)]
struct Lanes {
    occupied: HashSet<SubnetworkId>,
    frozen: bool,
}

struct SampleMassTracker {
    /// Raw sampled mass. This counts every sampled tx, including txs that the
    /// downstream SequenceSelector might skip because they do not fit the
    /// remaining block gap.
    sampled: u64,

    /// Remaining mass that the downstream SequenceSelector would see after
    /// greedily accepting sampled txs that fit in their current order.
    gap: u64,

    /// The normal sampling target, currently 1.2x the block mass limit.
    desired: u64,

    /// Number of sampled txs that failed to shrink the greedy pack gap.
    null_attempts: usize,

    /// Acceptable remaining greedy-pack gap after the normal sampling target is reached.
    target_gap: u64,
}

impl SampleMassTracker {
    fn new(policy: &Policy) -> Self {
        // Sample 20% more than the hard limit in order to allow the SequenceSelector to
        // compensate for consensus rejections.
        // Note that this is a soft target: sampling may pass it by one tx, and the
        // tracker may extend it when a block-sized tx exposes a greedy-pack gap.
        let desired = (policy.max_block_mass as f64 * MASS_LIMIT_FACTOR) as u64;

        // Target a remaining greedy-pack gap of at most 5% of block mass
        // (25K mass for the current 500K block mass limit).
        let target_gap = (policy.max_block_mass as f64 * TARGET_GAP_FACTOR) as u64;

        Self { sampled: 0, gap: policy.max_block_mass, desired, null_attempts: 0, target_gap }
    }

    /// Returns whether sampling should keep trying to build a useful sequence.
    fn should_continue(&self) -> bool {
        // Halt only if both:
        // 1. raw sampled mass already reached the normal target; and
        // 2. either the greedy-pack gap is small enough, or too many samples failed to shrink it.
        self.sampled <= self.desired || (self.null_attempts < MAX_NULL_ATTEMPTS && self.gap > self.target_gap)
    }

    /// Adds the sampled mass and updates the greedy-pack gap/null-attempt counters.
    fn record(&mut self, mass: u64) {
        self.sampled = self.sampled.saturating_add(mass);

        if let Some(gap) = self.gap.checked_sub(mass) {
            self.gap = gap;
        } else {
            // This tx increased raw sampled mass but did not shrink the greedy
            // pack gap. Bound how many such null attempts can keep sampling alive.
            self.null_attempts += 1;
        }
    }
}

impl Frontier {
    pub fn new(target_time_per_block_seconds: f64) -> Self {
        Self {
            search_tree: Default::default(),
            by_lane: Default::default(),
            total_mass: Default::default(),
            average_transaction_mass: INITIAL_AVG_MASS,
            target_time_per_block_seconds,
        }
    }
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
        let lane = key.lane();
        if self.search_tree.insert(key.clone()) {
            self.by_lane.entry(lane).or_default().insert(key);
            self.total_mass += mass;
            // A decaying average formula. Denote ɛ = 1 - AVG_MASS_DECAY_FACTOR. A transaction inserted N slots ago has
            // ɛ * (1 - ɛ)^N weight within the updated average. This gives some weight to the full mempool history while
            // giving higher importance to more recent samples.
            self.average_transaction_mass =
                self.average_transaction_mass * AVG_MASS_DECAY_FACTOR + mass as f64 * (1.0 - AVG_MASS_DECAY_FACTOR);
            true
        } else {
            false
        }
    }

    pub fn remove(&mut self, key: &FeerateTransactionKey) -> bool {
        let mass = key.mass;
        if self.search_tree.remove(key) {
            let lane = key.lane();
            let mut remove_lane = false;
            if let Some(entries) = self.by_lane.get_mut(&lane) {
                entries.remove(key);
                remove_lane = entries.is_empty();
            }
            if remove_lane {
                self.by_lane.remove(&lane);
            }
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

        let mut distr = Uniform::new(0f64, self.total_weight());
        let mut down_iter = self.search_tree.descending_iter();
        let mut top = down_iter.next().unwrap();
        let mut cache = HashSet::new();
        let mut sequence = SequenceSelectorInput::default();
        let mut mass = SampleMassTracker::new(policy);
        let mut collisions = 0;
        let mut lanes = Lanes::default();

        // The sampling process is converging so the cache will eventually hold all entries, which guarantees loop exit
        'outer: while cache.len() < self.search_tree.len() && mass.should_continue() {
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
            if lanes.occupied.len() < policy.lanes_per_block_limit {
                lanes.occupied.insert(item.lane());
            } else if !lanes.occupied.contains(&item.lane()) {
                // The weighted sampler wants to spill outside the first LPB discovered lanes.
                // Freeze L here and complete the remaining selection within those lanes only.
                lanes.frozen = true;
                break;
            }
            sequence.push(item.tx.clone(), item.mass);
            mass.record(item.mass);
        }

        if lanes.frozen {
            self.finish_intra_lane_selection(&mut sequence, &cache, &lanes, &mut mass);
        }
        trace!("[mempool frontier sample inplace] collisions: {collisions}, cache: {}", cache.len());
        *_collisions += collisions;
        sequence
    }

    /// Completes a sample by selecting transactions only from lanes that already occupy LPB slots.
    ///
    /// The initial sampling phase remains fully weighted until it first attempts to spill outside
    /// the first LPB lanes. From that point on, we deterministically fill from the frozen lane set,
    /// using a heap of per-lane heads so each selected transaction costs `O(log LPB)`.
    ///
    /// This is a temporary, deliberately simple policy. It is not globally optimal for miner fees,
    /// but keeps template construction fast while bounding lane fanout.
    fn finish_intra_lane_selection(
        &self,
        sequence: &mut SequenceSelectorInput,
        cache: &HashSet<TransactionId>,
        lanes: &Lanes,
        mass: &mut SampleMassTracker,
    ) {
        let mut lane_iters = lanes
            .occupied
            .iter()
            .filter_map(|lane| {
                self.by_lane.get(lane).map(|entries| entries.iter().rev().filter(|item| !cache.contains(&item.tx.id())))
            })
            .collect::<Vec<_>>();
        let mut heads = BinaryHeap::new();

        // Seed the heap with the best uncached transaction from each occupied lane.
        for (idx, iter) in lane_iters.iter_mut().enumerate() {
            if let Some(item) = iter.next() {
                heads.push((item.clone(), idx));
            }
        }

        // Standard k-way merge: pop the best lane head, then replenish only that lane.
        while mass.should_continue() {
            let Some((item, best_idx)) = heads.pop() else {
                break;
            };

            sequence.push(item.tx.clone(), item.mass);
            mass.record(item.mass);

            // Advance the lane we just consumed. The iterator already skips pre-freeze samples.
            let iter = &mut lane_iters[best_idx];
            if let Some(next) = iter.next() {
                heads.push((next.clone(), best_idx));
            }
        }
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier.
    ///
    /// The logic is divided into three cases:
    ///     1. The frontier is small and can fit entirely into a block: perform no sampling and return
    ///        a TakeAllSelector
    ///     2. The frontier has at least ~4x the capacity of a block: expected collision rate is low, perform
    ///        in-place k*log(n) sampling and return a SequenceSelector
    ///     3. The frontier has 1-4x capacity of a block. In this case we expect a high collision rate while
    ///        the number of overall transactions is still low, so we clone the weighted tree and remove selected
    ///        or skipped candidates from the clone (performing the actual sampling out of the mempool lock)
    ///
    /// The above thresholds were selected based on benchmarks. Overall, this dynamic selection provides
    /// full transaction selection in less than 150 µs even if the frontier has 1M entries (!!). See mining/benches
    /// for more details.  
    pub fn build_selector(&self, policy: &Policy) -> Box<dyn TemplateTransactionSelector> {
        if self.total_mass <= policy.max_block_mass {
            // TakeAll can still filter by LPB/gas, so feed it best-first.
            Box::new(TakeAllSelector::new(self.search_tree.descending_iter().map(|k| k.tx.clone()).collect(), policy.clone()))
        } else if self.total_mass > policy.max_block_mass * COLLISION_FACTOR {
            let mut rng = rand::thread_rng();
            Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, policy, &mut 0), policy.clone()))
        } else {
            Box::new(MutatingTreeSelector::new(policy.clone(), self.search_tree.clone()))
        }
    }

    /// Exposed for benchmarking purposes
    pub fn build_selector_sample_inplace(&self, _collisions: &mut u64) -> Box<dyn TemplateTransactionSelector> {
        let mut rng = rand::thread_rng();
        let policy = Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS);
        Box::new(SequenceSelector::new(self.sample_inplace(&mut rng, &policy, _collisions), policy))
    }

    /// Exposed for benchmarking purposes
    pub fn build_selector_take_all(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(TakeAllSelector::new(
            self.search_tree.descending_iter().map(|k| k.tx.clone()).collect(),
            Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS),
        ))
    }

    /// Exposed for benchmarking purposes
    pub fn build_rebalancing_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(RebalancingWeightedTransactionSelector::new(
            Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS),
            self.search_tree.ascending_iter().cloned().map(CandidateTransaction::from_key).collect(),
        ))
    }

    /// Exposed for benchmarking purposes
    pub fn build_mutating_tree_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        Box::new(MutatingTreeSelector::new(Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS), self.search_tree.clone()))
    }

    /// Builds a feerate estimator based on internal state of the ready transactions frontier
    pub fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        let average_transaction_mass = self.average_transaction_mass;
        let bps = args.network_blocks_per_second as f64;
        let mut mass_per_block = args.maximum_mass_per_block as f64;
        let mut inclusion_interval = average_transaction_mass / (mass_per_block * bps);
        let mut estimator = FeerateEstimator::new(self.total_weight(), inclusion_interval, self.target_time_per_block_seconds);

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
            let pending_estimator = FeerateEstimator::new(prefix_weight, inclusion_interval, self.target_time_per_block_seconds);

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
    use kaspa_consensus_core::{
        subnets::SubnetworkId,
        tx::{Transaction, TransactionInput, TransactionOutpoint},
    };
    use kaspa_hashes::{HasherBase, TransactionID};
    use rand::{SeedableRng, rngs::StdRng, thread_rng};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn build_feerate_key_with_lane(fee: u64, mass: u64, id: u64, lane: SubnetworkId) -> FeerateTransactionKey {
        let mut hasher = TransactionID::new();
        let prev = hasher.update(id.to_le_bytes()).clone().finalize();
        let input = TransactionInput::new(TransactionOutpoint::new(prev, 0), vec![], 0, 0);
        let tx = Arc::new(Transaction::new(0, vec![input], vec![], 0, lane, 0, vec![]));
        FeerateTransactionKey::new(fee, mass, tx)
    }

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

        let mut frontier = Frontier::new(1.0);
        for item in map.values().cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let _sample = frontier.sample_inplace(&mut rng, &Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS), &mut 0);
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

        let mut frontier = Frontier::new(1.0);
        for item in map.values().cloned() {
            frontier.insert(item).then_some(()).unwrap();
        }

        let mut selector = frontier.build_selector(&Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS));
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_rebalancing_selector();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_sample_inplace(&mut 0);
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector_take_all();
        selector.select_transactions().iter().map(|k| k.gas).sum::<u64>();

        let mut selector = frontier.build_selector(&Policy::new(500_000, DEFAULT_BLOCK_LANE_LIMITS));
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
        let mut frontier = Frontier::new(1.0);
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
    fn test_sample_inplace_respects_lane_limit_after_freeze() {
        let mut rng = StdRng::seed_from_u64(42);
        let lanes = [
            SubnetworkId::from_namespace([1, 1, 0, 0]),
            SubnetworkId::from_namespace([2, 1, 0, 0]),
            SubnetworkId::from_namespace([3, 1, 0, 0]),
        ];

        let mut frontier = Frontier::new(1.0);
        for i in 0..90u64 {
            let lane = lanes[(i as usize) % lanes.len()];
            let fee = 1_000_000 - i;
            frontier.insert(build_feerate_key_with_lane(fee, 100, i, lane)).then_some(()).unwrap();
        }

        let mut policy = Policy::new(1_000, DEFAULT_BLOCK_LANE_LIMITS);
        policy.lanes_per_block_limit = 2;
        let sample = frontier.sample_inplace(&mut rng, &policy, &mut 0);
        let selected_lanes = sample.iter().map(|tx| tx.tx.subnetwork_id).collect::<HashSet<_>>();

        assert_eq!(selected_lanes.len(), policy.lanes_per_block_limit);
    }

    /// Epsilon used for various test comparisons
    const EPS: f64 = 0.000001;

    #[test]
    fn test_feerate_estimator() {
        const MIN_FEERATE: f64 = 1.0;
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
            let mut frontier = Frontier::new(1.0);
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
                    b.feerate.is_normal() && b.feerate >= MIN_FEERATE - EPS,
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
            let mut frontier = Frontier::new(1.0);
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
                    b.feerate.is_normal() && b.feerate >= MIN_FEERATE - EPS,
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
        let mut frontier = Frontier::new(1.0);
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
                b.feerate.is_normal() && b.feerate >= MIN_FEERATE - EPS,
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
        const MIN_FEERATE: f64 = 1.0;
        let mut map = HashMap::new();
        for i in 0..304 {
            let mass: u64 = 1650;
            let fee = 10_000_000 * 1_000_000;
            let key = build_feerate_key(fee, mass, i);
            map.insert(key.tx.id(), key);
        }

        // All lens make for less than block capacity (given the mass used)
        for len in [0, 1, 10, 100, 200, 250, 300] {
            let mut frontier = Frontier::new(1.0);
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
                // Expect min feerate bcs blocks are not full
                assert!(
                    (b.feerate - MIN_FEERATE).abs() <= EPS,
                    "bucket feerate is expected to be equal to the minimum standard feerate"
                );
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
