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
        reachability::ReachabilityStoreReader,
    },
};
use kaspa_consensus_core::{blockhash::BlockHashExtensions, config::params::ForkedParam, BlockHashMap};
use kaspa_core::warn;
use kaspa_hashes::Hash;
use parking_lot::{Mutex, RwLock};

#[derive(Clone)]
pub struct PruningPointManager<
    S: GhostdagStoreReader,
    T: ReachabilityStoreReader,
    U: HeaderStoreReader,
    V: PastPruningPointsStoreReader,
    W: HeadersSelectedTipStoreReader,
> {
    pruning_depth: ForkedParam<u64>,
    finality_depth: ForkedParam<u64>,
    genesis_hash: Hash,

    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<S>,
    headers_store: Arc<U>,
    past_pruning_points_store: Arc<V>,
    header_selected_tip_store: Arc<RwLock<W>>,

    /// The number of hopes to go through pruning samples in order to get the pruning point of a sample
    pruning_samples_steps: u64,

    // tmp
    pruning_samples_store: Arc<Mutex<BlockHashMap<Hash>>>,
}

impl<
        S: GhostdagStoreReader,
        T: ReachabilityStoreReader,
        U: HeaderStoreReader,
        V: PastPruningPointsStoreReader,
        W: HeadersSelectedTipStoreReader,
    > PruningPointManager<S, T, U, V, W>
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
    ) -> Self {
        // [Crescendo]: These conditions ensure that blue score points with the same finality score before
        // the fork will remain with the same finality score post the fork. See below for the usage.
        assert!(finality_depth.before() <= finality_depth.after());
        assert!(finality_depth.after() % finality_depth.before() == 0);
        assert!(pruning_depth.before() <= pruning_depth.after());

        // TODO (Crescendo): assert P is not multiple of F + noise(K)

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
            pruning_samples_store: Default::default(),
        }
    }

    pub fn expected_header_pruning_point(
        &self,
        ghostdag_data: CompactGhostdagData,
        _pruning_info: PruningPointInfo,
        hash: Option<Hash>,
    ) -> Hash {
        // let v1 = self.expected_header_pruning_point_v1(ghostdag_data, pruning_info);
        // let v2 = self.expected_header_pruning_point_v2(ghostdag_data, hash);
        // assert_eq!(v1, v2);
        // v2

        self.expected_header_pruning_point_v2(ghostdag_data, hash)
    }

    pub fn expected_header_pruning_point_v2(&self, ghostdag_data: CompactGhostdagData, hash: Option<Hash>) -> Hash {
        let selected_parent_daa_score = self.headers_store.get_daa_score(ghostdag_data.selected_parent).unwrap();
        let pruning_depth = self.pruning_depth.get(selected_parent_daa_score);
        let finality_depth = self.finality_depth.get(selected_parent_daa_score);

        let selected_parent_blue_score = self.headers_store.get_blue_score(ghostdag_data.selected_parent).unwrap();

        let pruning_sample = self.calc_pruning_sample(ghostdag_data, finality_depth);

        if let Some(hash) = hash {
            self.pruning_samples_store.lock().insert(hash, pruning_sample);
        }

        // TODO: fix 2nd arg naming
        let is_self_pruning_sample = self.is_pruning_sample(ghostdag_data.blue_score, selected_parent_blue_score, finality_depth);
        let selected_parent_pruning_point = self.headers_store.get_header(ghostdag_data.selected_parent).unwrap().pruning_point;
        let mut steps = 1;
        let mut current = pruning_sample;
        let pruning_point = loop {
            if current == self.genesis_hash {
                break current;
            }
            let current_blue_score = self.headers_store.get_blue_score(current).unwrap();
            if current_blue_score + pruning_depth <= ghostdag_data.blue_score {
                break current;
            }
            // For samples: special clamp for the period right after the fork
            if is_self_pruning_sample && steps == self.pruning_samples_steps {
                break current;
            }
            // For non samples: clamp to samples
            if current == selected_parent_pruning_point {
                break current;
            }
            current = self.pruning_samples_store.lock().get(&current).copied().unwrap();
            steps += 1;
        };

        pruning_point
    }

    pub fn calc_pruning_sample(&self, ghostdag_data: CompactGhostdagData, finality_depth: u64) -> Hash {
        if ghostdag_data.selected_parent == self.genesis_hash {
            return self.genesis_hash;
        }

        let selected_parent_blue_score = self.headers_store.get_blue_score(ghostdag_data.selected_parent).unwrap();
        let selected_parent_pruning_sample = self.pruning_samples_store.lock().get(&ghostdag_data.selected_parent).copied().unwrap();
        let selected_parent_pruning_sample_blue_score = self.headers_store.get_blue_score(selected_parent_pruning_sample).unwrap();

        if self.is_pruning_sample(selected_parent_blue_score, selected_parent_pruning_sample_blue_score, finality_depth) {
            ghostdag_data.selected_parent
        } else {
            selected_parent_pruning_sample
        }
    }

    /// A block is a pruning sample *iff* its own finality score is larger than its pruning sample's finality score
    fn is_pruning_sample(&self, self_blue_score: u64, pruning_sample_blue_score: u64, finality_depth: u64) -> bool {
        self.finality_score(pruning_sample_blue_score, finality_depth) < self.finality_score(self_blue_score, finality_depth)
    }

    pub fn next_pruning_points(
        &self,
        ghostdag_data: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, Hash) {
        let v2 = self.next_pruning_points_v2(ghostdag_data, current_pruning_point);
        // let (v1, candidate) = self.next_pruning_points_v1(ghostdag_data, current_candidate, current_pruning_point);
        // assert_eq!(v1, v2);
        (v2, current_candidate)
    }

    pub fn next_pruning_points_v2(&self, ghostdag_data: CompactGhostdagData, current_pruning_point: Hash) -> Vec<Hash> {
        // Handle the edge case where sink is genesis
        if ghostdag_data.selected_parent.is_origin() {
            return vec![];
        }
        let mut deque = VecDeque::with_capacity(self.pruning_samples_steps as usize);

        let mut current = self.expected_header_pruning_point_v2(ghostdag_data, None);
        while current != current_pruning_point {
            deque.push_front(current);
            current = self.pruning_samples_store.lock().get(&current).copied().unwrap();
        }

        deque.into()
    }

    fn next_pruning_points_v1(
        &self,
        ghostdag_data: CompactGhostdagData,
        current_candidate: Hash,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, Hash) {
        // Handle the edge case where sink is genesis
        if ghostdag_data.selected_parent.is_origin() {
            return (vec![], current_candidate);
        }
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

    /// finality_score is the number of finality intervals which have passed since
    /// genesis and up to the given blue_score.
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
            if self.pruning_depth.activation().is_active(selected_parent_daa_score)
                && ghostdag_data.blue_score.saturating_sub(sp_pp_blue_score) < self.pruning_depth.after()
                && CoinFlip::new(1.0 / 1000.0).flip()
            {
                warn!(
                    "[Crescendo] Pruning depth increasing post activation: {} (target: {})",
                    ghostdag_data.blue_score.saturating_sub(sp_pp_blue_score),
                    self.pruning_depth.after()
                );
            }
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
        // full new pruning depth is expected, so we use the DAA score of the pruning point itself as an indicator.
        // This means that in the first few days following the fork we err on the side of a shorter period which is
        // a weaker requirement
        let pruning_depth = self.pruning_depth.get(self.headers_store.get_daa_score(pp_candidate).unwrap());
        self.is_pruning_point_in_pruning_depth(tip_bs, pp_candidate, pruning_depth)
    }

    pub fn are_pruning_points_in_valid_chain(&self, pruning_info: PruningPointInfo, syncer_sink: Hash) -> bool {
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
        for current in self.reachability_service.backward_chain_iterator(syncer_sink, pruning_info.pruning_point, false) {
            let current_header = self.headers_store.get_header(current).unwrap();
            // Post-crescendo: expected header pruning point is no longer part of header validity, but we want to make sure
            // the syncer's virtual chain indeed coincides with the pruning point and past pruning points before downloading
            // the UTXO set and resolving virtual. Hence we perform the check over this chain here.
            let expected_header_pruning_point =
                self.expected_header_pruning_point(self.ghostdag_store.get_compact_data(current).unwrap(), pruning_info, None);
            if expected_header_pruning_point != current_header.pruning_point {
                return false;
            }
            if expected_pps_queue.back().is_none_or(|&h| h != current_header.pruning_point) {
                expected_pps_queue.push_back(current_header.pruning_point);
            }
        }

        for idx in (0..=pruning_info.index).rev() {
            let pp = self.past_pruning_points_store.get(idx).unwrap();
            let pp_header = self.headers_store.get_header(pp).unwrap();
            let Some(expected_pp) = expected_pps_queue.pop_front() else {
                // If we have less than expected pruning points.
                return false;
            };

            if expected_pp != pp {
                return false;
            }

            if idx == 0 {
                // The 0th pruning point should always be genesis, and no
                // more pruning points should be expected below it.
                if !expected_pps_queue.is_empty() || pp != self.genesis_hash {
                    return false;
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
                    return false;
                }
            }
        }

        true
    }
}
