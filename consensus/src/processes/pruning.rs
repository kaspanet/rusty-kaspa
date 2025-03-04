use std::{collections::VecDeque, sync::Arc};

use super::{reachability::ReachabilityResultExtensions, utils::CoinFlip};
use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        ghostdag::{CompactGhostdagData, GhostdagStoreReader},
        headers::HeaderStoreReader,
        headers_selected_tip::HeadersSelectedTipStoreReader,
        past_pruning_points::PastPruningPointsStoreReader,
        pruning::PruningPointInfo,
        pruning_samples::PruningSamplesStore,
        reachability::ReachabilityStoreReader,
    },
};
use kaspa_consensus_core::{
    blockhash::BlockHashExtensions,
    config::params::ForkedParam,
    errors::pruning::{PruningImportError, PruningImportResult},
};
use kaspa_core::{info, log::CRESCENDO_KEYWORD};
use kaspa_database::prelude::StoreResultEmptyTuple;
use kaspa_hashes::Hash;
use parking_lot::RwLock;

pub struct PruningPointReply {
    /// The most recent pruning sample from POV of the queried block (with distance up to ~F)
    pub pruning_sample: Hash,

    /// The pruning point of the queried block. I.e., the most recent pruning sample with
    /// depth P (except for shortly after the fork where the new P' is gradually reached)
    pub pruning_point: Hash,
}

#[derive(Clone)]
pub struct PruningPointManager<
    S: GhostdagStoreReader,
    T: ReachabilityStoreReader,
    U: HeaderStoreReader,
    V: PastPruningPointsStoreReader,
    W: HeadersSelectedTipStoreReader,
    Y: PruningSamplesStore,
> {
    /// Forked pruning depth param. Throughout this file we use P, P' to indicate the pre, post activation depths respectively
    pruning_depth: ForkedParam<u64>,

    /// Forked finality depth param. Throughout this file we use F, F' to indicate the pre, post activation depths respectively.
    /// Note that this quantity represents here the interval between pruning point samples and is not tightly coupled with the
    /// actual concept of finality as used by virtual processor to reject deep reorgs   
    finality_depth: ForkedParam<u64>,

    genesis_hash: Hash,

    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<S>,
    headers_store: Arc<U>,
    past_pruning_points_store: Arc<V>,
    header_selected_tip_store: Arc<RwLock<W>>,
    pruning_samples_store: Arc<Y>,

    /// The number of hops to go through pruning samples in order to get the pruning point of a sample
    pruning_samples_steps: u64,
}

impl<
        S: GhostdagStoreReader,
        T: ReachabilityStoreReader,
        U: HeaderStoreReader,
        V: PastPruningPointsStoreReader,
        W: HeadersSelectedTipStoreReader,
        Y: PruningSamplesStore,
    > PruningPointManager<S, T, U, V, W, Y>
{
    pub fn new(
        pruning_depth: ForkedParam<u64>,
        finality_depth: ForkedParam<u64>,
        genesis_hash: Hash,
        reachability_service: MTReachabilityService<T>,
        ghostdag_store: Arc<S>,
        headers_store: Arc<U>,
        past_pruning_points_store: Arc<V>,
        header_selected_tip_store: Arc<RwLock<W>>,
        pruning_samples_store: Arc<Y>,
    ) -> Self {
        // [Crescendo]: These conditions ensure that blue score points with the same finality score before
        // the fork will remain with the same finality score post the fork. See below for the usage.
        assert!(finality_depth.before() <= finality_depth.after());
        assert!(finality_depth.after() % finality_depth.before() == 0);
        assert!(pruning_depth.before() <= pruning_depth.after());

        let pruning_samples_steps = pruning_depth.before().div_ceil(finality_depth.before());
        assert_eq!(pruning_samples_steps, pruning_depth.after().div_ceil(finality_depth.after()));

        Self {
            pruning_depth,
            finality_depth,
            genesis_hash,
            reachability_service,
            ghostdag_store,
            headers_store,
            past_pruning_points_store,
            header_selected_tip_store,
            pruning_samples_steps,
            pruning_samples_store,
        }
    }

    /// The new method for calculating the expected pruning point from some POV (header/virtual) using the new
    /// pruning samples store. Except for edge cases during fork transition, this method is expected to retain
    /// the exact semantics of current rules (v1).
    ///
    /// Let B denote the current block (represented by `ghostdag_data`)
    /// Assumptions:
    ///     1. Unlike v1 this method assumes that the current global pruning point is on B's chain, which
    ///        is why it should be called only for chain candidates / sink / virtual
    ///     2. All chain ancestors of B up to the pruning point are expected to have a
    ///        `pruning_sample_from_pov` store entry    
    pub fn expected_header_pruning_point_v2(&self, ghostdag_data: CompactGhostdagData) -> PruningPointReply {
        //
        // Note that past pruning samples are only assumed to have a header store entry and a pruning sample
        // store entry, se we only use these stores here (and specifically do not use the ghostdag store)
        //

        let selected_parent_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        let pruning_depth = self.pruning_depth.get(selected_parent_daa_score);
        let finality_depth = self.finality_depth.get(selected_parent_daa_score);

        let selected_parent_blue_score = self.headers_store.get_blue_score(ghostdag_data.selected_parent).unwrap();

        let pruning_sample = if ghostdag_data.selected_parent == self.genesis_hash {
            self.genesis_hash
        } else {
            let selected_parent_pruning_sample =
                self.pruning_samples_store.pruning_sample_from_pov(ghostdag_data.selected_parent).unwrap();
            let selected_parent_pruning_sample_blue_score = self.headers_store.get_blue_score(selected_parent_pruning_sample).unwrap();

            if self.is_pruning_sample(selected_parent_blue_score, selected_parent_pruning_sample_blue_score, finality_depth) {
                // The selected parent is the most recent sample
                ghostdag_data.selected_parent
            } else {
                // ...otherwise take the sample from its pov
                selected_parent_pruning_sample
            }
        };

        let is_self_pruning_sample = self.is_pruning_sample(ghostdag_data.blue_score, selected_parent_blue_score, finality_depth);
        let selected_parent_pruning_point = self.headers_store.get_header(ghostdag_data.selected_parent).unwrap().pruning_point;
        let mut steps = 1;
        let mut current = pruning_sample;
        let pruning_point = loop {
            if current == self.genesis_hash {
                break current;
            }
            let current_blue_score = self.headers_store.get_blue_score(current).unwrap();
            // Find the most recent sample with pruning depth
            if current_blue_score + pruning_depth <= ghostdag_data.blue_score {
                break current;
            }
            // For samples: special clamp for the period right after the fork (where we reach ceiling(P/F) steps before reaching P' depth)
            if is_self_pruning_sample && steps == self.pruning_samples_steps {
                break current;
            }
            // For non samples: clamp to selected parent pruning point to maintain monotonicity (needed because of the previous condition)
            if current == selected_parent_pruning_point {
                break current;
            }
            current = self.pruning_samples_store.pruning_sample_from_pov(current).unwrap();
            steps += 1;
        };

        PruningPointReply { pruning_sample, pruning_point }
    }

    fn log_pruning_depth_post_activation(
        &self,
        ghostdag_data: CompactGhostdagData,
        selected_parent_daa_score: u64,
        pruning_point_blue_score: u64,
    ) {
        if self.pruning_depth.activation().is_active(selected_parent_daa_score)
            && ghostdag_data.blue_score.saturating_sub(pruning_point_blue_score) < self.pruning_depth.after()
            && CoinFlip::new(1.0 / 1000.0).flip()
        {
            info!(target: CRESCENDO_KEYWORD,
                "[Crescendo] Pruning depth increasing post activation: {} (target: {})",
                ghostdag_data.blue_score.saturating_sub(pruning_point_blue_score),
                self.pruning_depth.after()
            );
        }
    }

    /// A block is a pruning sample *iff* its own finality score is larger than its pruning sample
    /// finality score or its selected parent finality score (or any block in between them).
    ///
    /// To see why we can compare to any such block, observe that by definition all blocks in the range
    /// `[pruning sample, selected parent]` must have the same finality score.
    fn is_pruning_sample(&self, self_blue_score: u64, epoch_chain_ancestor_blue_score: u64, finality_depth: u64) -> bool {
        self.finality_score(epoch_chain_ancestor_blue_score, finality_depth) < self.finality_score(self_blue_score, finality_depth)
    }

    pub fn next_pruning_points(
        &self,
        sink_ghostdag: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, Hash) {
        if sink_ghostdag.selected_parent.is_origin() {
            // This only happens when sink is genesis
            return (vec![], current_candidate);
        }
        let selected_parent_daa_score = self.headers_store.get_daa_score(sink_ghostdag.selected_parent).unwrap();
        if self.pruning_depth.activation().is_active(selected_parent_daa_score) {
            let v2 = self.next_pruning_points_v2(sink_ghostdag, selected_parent_daa_score, current_pruning_point);
            // Keep the candidate valid also post activation just in case it's still used by v1 calls
            let candidate = v2.last().copied().unwrap_or(current_candidate);
            (v2, candidate)
        } else {
            let (v1, candidate) = self.next_pruning_points_v1(sink_ghostdag, current_candidate, current_pruning_point);
            // [Crescendo]: sanity check that v2 logic pre activation is equivalent to v1
            let v2 = self.next_pruning_points_v2(sink_ghostdag, selected_parent_daa_score, current_pruning_point);
            assert_eq!(v1, v2, "v1 = v2 pre activation");
            (v1, candidate)
        }
    }

    fn next_pruning_points_v2(
        &self,
        sink_ghostdag: CompactGhostdagData,
        selected_parent_daa_score: u64,
        current_pruning_point: Hash,
    ) -> Vec<Hash> {
        let current_pruning_point_blue_score = self.headers_store.get_blue_score(current_pruning_point).unwrap();

        // Sanity check #1: global pruning point depth from sink >= min(P, P')
        if current_pruning_point_blue_score + self.pruning_depth.lower_bound() > sink_ghostdag.blue_score {
            // During initial IBD the sink can be close to the global pruning point.
            // We use min(P, P') here and rely on sanity check #2 for post activation edge cases
            return vec![];
        }

        let sink_pruning_point = self.expected_header_pruning_point_v2(sink_ghostdag).pruning_point;
        let sink_pruning_point_blue_score = self.headers_store.get_blue_score(sink_pruning_point).unwrap();

        // Log the current pruning depth if it has not reached P' yet
        self.log_pruning_depth_post_activation(sink_ghostdag, selected_parent_daa_score, sink_pruning_point_blue_score);

        // Sanity check #2: if the sink pruning point is lower or equal to current, there is no need to search
        if sink_pruning_point_blue_score <= current_pruning_point_blue_score {
            return vec![];
        }

        let mut current = sink_pruning_point;
        let mut deque = VecDeque::with_capacity(self.pruning_samples_steps as usize);
        // At this point we have verified that sink_pruning_point is a chain block above current_pruning_point
        // (by comparing blue score) so we know the loop must eventually exit correctly
        while current != current_pruning_point {
            deque.push_front(current);
            current = self.pruning_samples_store.pruning_sample_from_pov(current).unwrap();
        }

        deque.into()
    }

    fn next_pruning_points_v1(
        &self,
        ghostdag_data: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, Hash) {
        let selected_parent_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        let pruning_depth = self.pruning_depth.get(selected_parent_daa_score);
        let finality_depth = self.finality_depth.get(selected_parent_daa_score);
        self.next_pruning_points_v1_inner(ghostdag_data, current_candidate, current_pruning_point, pruning_depth, finality_depth)
    }

    /// Returns the next pruning points and an updated pruning point candidate given the current
    /// pruning point (P), a current candidate (C) and a target block B (represented by GD data).
    ///
    /// The pruning point candidate C is a moving block which usually has pruning depth from sink but
    /// its finality score is still equal to P. It serves as an optimal starting point for searching
    /// up rather then restarting the search from P each time.    
    ///
    /// Assumptions: P ∈ chain(C), C ∈ chain(B), P and C have the same finality score
    ///
    /// Returns: new pruning points ordered from bottom up and an updated candidate
    fn next_pruning_points_v1_inner(
        &self,
        ghostdag_data: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
        pruning_depth: u64,
        finality_depth: u64,
    ) -> (Vec<Hash>, Hash) {
        // If the pruning point is more out of date than that, an IBD with headers proof is needed anyway.
        let mut new_pruning_points = Vec::with_capacity((pruning_depth / finality_depth) as usize);
        let mut latest_pruning_point_bs = self.ghostdag_store.get_blue_score(current_pruning_point).unwrap();

        if latest_pruning_point_bs + pruning_depth > ghostdag_data.blue_score {
            // The pruning point is not in depth of self.pruning_depth, so there's
            // no point in checking if it is required to update it. This can happen
            // because virtual is not immediately updated during IBD, so the pruning point
            // might be in depth less than self.pruning_depth.
            return (vec![], current_candidate);
        }

        let mut new_candidate = current_candidate;

        /*
            [Crescendo]

            Notation:
                P = pruning point
                C = candidate
                F0 = the finality depth before the fork
                F1 = the finality depth after the fork

            Property 1: F0 <= F1 AND F1 % F0 == 0 (validated in Self::new)

            Remark 1: if P,C had the same finality score with regard to F0, they have the same finality score also with regard to F1

            Proof by picture (based on Property 1):
                F0:    [    0    ] [    1    ] [    2    ] [    3    ] [    4    ] [    5    ]                 ...                 [    9    ] ...
                F1:    [                            0                            ] [                            1                            ] ...

                (each row divides the blue score space into finality score buckets with F0 or F1 numbers in each bucket correspondingly)

            This means we can safely begin the search from C even in the few moments post the fork (i.e., there's no fear of needing to "pull" C back)

            Note that overall this search is guaranteed to provide the desired monotonicity described in KIP-14:
            https://github.com/kaspanet/kips/blob/master/kip-0014.md#pruning-point-adjustment
        */
        for selected_child in self.reachability_service.forward_chain_iterator(current_candidate, ghostdag_data.selected_parent, true)
        {
            let selected_child_bs = self.ghostdag_store.get_blue_score(selected_child).unwrap();

            if ghostdag_data.blue_score - selected_child_bs < pruning_depth {
                break;
            }

            new_candidate = selected_child;
            let new_candidate_bs = selected_child_bs;

            if self.finality_score(new_candidate_bs, finality_depth) > self.finality_score(latest_pruning_point_bs, finality_depth) {
                new_pruning_points.push(new_candidate);
                latest_pruning_point_bs = new_candidate_bs;
            }
        }

        (new_pruning_points, new_candidate)
    }

    /// Returns the floored integer division of blue score by finality depth.
    /// The returned number represent the sampling epoch this blue score point belongs to.   
    fn finality_score(&self, blue_score: u64, finality_depth: u64) -> u64 {
        blue_score / finality_depth
    }

    fn expected_header_pruning_point_v1_inner(
        &self,
        ghostdag_data: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
        pruning_depth: u64,
        finality_depth: u64,
    ) -> Hash {
        self.next_pruning_points_v1_inner(ghostdag_data, current_candidate, current_pruning_point, pruning_depth, finality_depth)
            .0
            .last()
            .copied()
            .unwrap_or(current_pruning_point)
    }

    pub fn expected_header_pruning_point_v1(&self, ghostdag_data: CompactGhostdagData, pruning_info: PruningPointInfo) -> Hash {
        if ghostdag_data.selected_parent == self.genesis_hash {
            return self.genesis_hash;
        }

        let selected_parent_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        let pruning_depth = self.pruning_depth.get(selected_parent_daa_score);
        let finality_depth = self.finality_depth.get(selected_parent_daa_score);

        let (current_pruning_point, current_candidate, current_pruning_point_index) = pruning_info.decompose();

        let sp_pp = self.headers_store.get_header(ghostdag_data.selected_parent).unwrap().pruning_point;
        let sp_pp_blue_score = self.headers_store.get_blue_score(sp_pp).unwrap();

        // If the block doesn't have the pruning in its selected chain we know for sure that it can't trigger a pruning point
        // change (we check the selected parent to take care of the case where the block is the virtual which doesn't have reachability data).
        let has_pruning_point_in_its_selected_chain =
            self.reachability_service.is_chain_ancestor_of(current_pruning_point, ghostdag_data.selected_parent);

        // Note: the pruning point from the POV of the current block is the first block in its chain that is in depth of self.pruning_depth and
        // its finality score is greater than the previous pruning point. This is why if the diff between finality_score(selected_parent.blue_score + 1) * finality_interval
        // and the current block blue score is less than self.pruning_depth we can know for sure that this block didn't trigger a pruning point change.
        let min_required_blue_score_for_next_pruning_point =
            (self.finality_score(sp_pp_blue_score, finality_depth) + 1) * finality_depth;
        let next_or_current_pp = if has_pruning_point_in_its_selected_chain
            && min_required_blue_score_for_next_pruning_point + pruning_depth <= ghostdag_data.blue_score
        {
            // If the selected parent pruning point is in the future of current global pruning point, then provide it as a suggestion
            let sp_pp_in_global_pp_future =
                self.reachability_service.is_dag_ancestor_of_result(current_pruning_point, sp_pp).unwrap_option().is_some_and(|b| b);

            /*
                Notation:
                    P = global pruning point
                    C = global candidate
                    B = current block (can be virtual)
                    S = B's selected parent
                    R = S's pruning point
                    F = the finality depth
            */

            let (pp, cc) = if sp_pp_in_global_pp_future {
                if self.reachability_service.is_chain_ancestor_of(sp_pp, current_candidate) {
                    // R ∈ future(P), R ∈ chain(C): use R as pruning point and C as candidate
                    // There are two cases: (i)  C is not deep enough from B, R will be returned
                    //                      (ii) C is deep enough and the search will start from it, possibly finding a new pruning point for B
                    (sp_pp, current_candidate)
                } else {
                    // R ∈ future(P), R ∉ chain(C): Use R as candidate as well.
                    // This might require a long walk up from R (bounded by F), however it is highly unlikely since it
                    // requires a ~pruning depth deep parallel chain
                    (sp_pp, sp_pp)
                }
            } else if self.reachability_service.is_chain_ancestor_of(current_candidate, ghostdag_data.selected_parent) {
                // R ∉ future(P), P,C ∈ chain(B)
                (current_pruning_point, current_candidate)
            } else {
                // R ∉ future(P), P ∈ chain(B), C ∉ chain(B)
                (current_pruning_point, current_pruning_point)
            };

            self.expected_header_pruning_point_v1_inner(ghostdag_data, cc, pp, pruning_depth, finality_depth)
        } else {
            sp_pp
        };

        // [Crescendo]: shortly after fork activation, R is not guaranteed to comply with the new
        // increased pruning depth, so we must manually verify not to go below it
        if sp_pp_blue_score >= self.headers_store.get_blue_score(next_or_current_pp).unwrap() {
            return sp_pp;
        }

        if self.is_pruning_point_in_pruning_depth(ghostdag_data.blue_score, next_or_current_pp, pruning_depth) {
            return next_or_current_pp;
        }

        for i in (0..=current_pruning_point_index).rev() {
            let past_pp = self.past_pruning_points_store.get(i).unwrap();

            // [Crescendo]: shortly after fork activation, R is not guaranteed to comply with the new
            // increased pruning depth, so we must manually verify not to go below it
            if sp_pp_blue_score >= self.headers_store.get_blue_score(past_pp).unwrap() {
                return sp_pp;
            }

            if self.is_pruning_point_in_pruning_depth(ghostdag_data.blue_score, past_pp, pruning_depth) {
                return past_pp;
            }
        }

        self.genesis_hash
    }

    fn is_pruning_point_in_pruning_depth(&self, pov_blue_score: u64, pruning_point: Hash, pruning_depth: u64) -> bool {
        let pp_bs = self.headers_store.get_blue_score(pruning_point).unwrap();
        pov_blue_score >= pp_bs + pruning_depth
    }

    pub fn is_valid_pruning_point(&self, pp_candidate: Hash, tip: Hash) -> bool {
        if pp_candidate == self.genesis_hash {
            return true;
        }
        if !self.reachability_service.is_chain_ancestor_of(pp_candidate, tip) {
            return false;
        }

        let tip_bs = self.ghostdag_store.get_blue_score(tip).unwrap();
        // [Crescendo]: for new nodes syncing right after the fork, it might be difficult to determine whether the
        // new pruning depth is expected, so we use the DAA score of the pruning point itself as an indicator.
        // This means that in the first few days following the fork we err on the side of a shorter period which is
        // a weaker requirement
        let pruning_depth = self.pruning_depth.get(self.headers_store.get_daa_score(pp_candidate).unwrap());
        self.is_pruning_point_in_pruning_depth(tip_bs, pp_candidate, pruning_depth)
    }

    pub fn are_pruning_points_in_valid_chain(&self, pruning_info: PruningPointInfo, syncer_sink: Hash) -> PruningImportResult<()> {
        // We want to validate that the past pruning points form a chain to genesis. Since
        // each pruning point's header doesn't point to the previous pruning point, but to
        // the pruning point from its POV, we can't just traverse from one pruning point to
        // the next one by merely relying on the current pruning point header, but instead
        // we rely on the fact that each pruning point is pointed by another known block or
        // pruning point.
        // So in the first stage we go over the selected chain and add to the queue of expected
        // pruning points all the pruning points from the POV of some chain block. In the second
        // stage we go over the past pruning points from recent to older, check that it's the head
        // of the queue (by popping the queue), and add its header pruning point to the queue since
        // we expect to see it later on the list.
        // The first stage is important because the most recent pruning point is pointing to a few
        // pruning points before, so the first few pruning points on the list won't be pointed by
        // any other pruning point in the list, so we are compelled to check if it's referenced by
        // the selected chain.
        let mut expected_pps_queue = VecDeque::new();
        for current in self.reachability_service.forward_chain_iterator(pruning_info.pruning_point, syncer_sink, true).skip(1) {
            let current_header = self.headers_store.get_header(current).unwrap();
            // Post-crescendo: expected header pruning point is no longer part of header validity, but we want to make sure
            // the syncer's virtual chain indeed coincides with the pruning point and past pruning points before downloading
            // the UTXO set and resolving virtual. Hence we perform the check over this chain here.
            let reply = self.expected_header_pruning_point_v2(self.ghostdag_store.get_compact_data(current).unwrap());
            if reply.pruning_point != current_header.pruning_point {
                return Err(PruningImportError::WrongHeaderPruningPoint(current_header.pruning_point, current));
            }
            // Save so that following blocks can recursively use this value
            self.pruning_samples_store.insert(current, reply.pruning_sample).unwrap_or_exists();
            /*
               Going up the chain from the pruning point to the sink. The goal is to exit this loop with a queue [P(0), P(-1), P(-2), ..., P(-n)]
               where P(0) is the current pruning point, P(-1) is the point before it and P(-n) is the pruning point of P(0). That is,
               ceiling(P/F) = n (where n is usually 3).

               Let C be the current block's pruning point. Push to the front of the queue if:
                   1. the queue is empty; OR
                   2. the front of the queue is different than C; AND
                   3. the front of the queue is different than P(0) (if it is P(0), we already filled the queue with what we need)
            */
            if expected_pps_queue.front().is_none_or(|&h| h != current_header.pruning_point && h != pruning_info.pruning_point) {
                expected_pps_queue.push_front(current_header.pruning_point);
            }
        }

        for idx in (0..=pruning_info.index).rev() {
            let pp = self.past_pruning_points_store.get(idx).unwrap();
            let pp_header = self.headers_store.get_header(pp).unwrap();
            let Some(expected_pp) = expected_pps_queue.pop_front() else {
                // If we have less than expected pruning points.
                return Err(PruningImportError::MissingPointedPruningPoint);
            };

            if expected_pp != pp {
                return Err(PruningImportError::WrongPointedPruningPoint);
            }

            if idx == 0 {
                // The 0th pruning point should always be genesis, and no
                // more pruning points should be expected below it.
                if !expected_pps_queue.is_empty() || pp != self.genesis_hash {
                    return Err(PruningImportError::UnpointedPruningPoint);
                }
                break;
            }

            // Add the pruning point from the POV of the current one if it's
            // not already added.
            match expected_pps_queue.back() {
                Some(last_added_pp) => {
                    if *last_added_pp != pp_header.pruning_point {
                        expected_pps_queue.push_back(pp_header.pruning_point);
                    }
                }
                None => {
                    // expected_pps_queue should always have one block in the queue
                    // until we reach genesis.
                    return Err(PruningImportError::MissingPointedPruningPoint);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use kaspa_consensus_core::{config::params::Params, network::NetworkType};

    #[test]
    fn assert_pruning_depth_consistency() {
        for net in NetworkType::iter() {
            let params: Params = net.into();

            let pruning_depth = params.pruning_depth();
            let finality_depth = params.finality_depth();
            let ghostdag_k = params.ghostdag_k();

            // Assert P is not a multiple of F +- noise(K)
            let mod_before = pruning_depth.before() % finality_depth.before();
            assert!((ghostdag_k.before() as u64) < mod_before && mod_before < finality_depth.before() - ghostdag_k.before() as u64);

            let mod_after = pruning_depth.after() % finality_depth.after();
            assert!((ghostdag_k.after() as u64) < mod_after && mod_after < finality_depth.after() - ghostdag_k.after() as u64);
        }
    }
}
