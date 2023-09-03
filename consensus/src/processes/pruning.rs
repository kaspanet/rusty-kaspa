use std::{collections::VecDeque, sync::Arc};

use super::reachability::ReachabilityResultExtensions;
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
use kaspa_hashes::Hash;
use kaspa_utils::option::OptionExtensions;
use parking_lot::RwLock;

#[derive(Clone)]
pub struct PruningPointManager<
    S: GhostdagStoreReader,
    T: ReachabilityStoreReader,
    U: HeaderStoreReader,
    V: PastPruningPointsStoreReader,
    W: HeadersSelectedTipStoreReader,
> {
    pruning_depth: u64,
    finality_depth: u64,
    genesis_hash: Hash,

    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<S>,
    headers_store: Arc<U>,
    past_pruning_points_store: Arc<V>,
    header_selected_tip_store: Arc<RwLock<W>>,
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
        pruning_depth: u64,
        finality_depth: u64,
        genesis_hash: Hash,
        reachability_service: MTReachabilityService<T>,
        ghostdag_store: Arc<S>,
        headers_store: Arc<U>,
        past_pruning_points_store: Arc<V>,
        header_selected_tip_store: Arc<RwLock<W>>,
    ) -> Self {
        Self {
            pruning_depth,
            finality_depth,
            genesis_hash,
            reachability_service,
            ghostdag_store,
            headers_store,
            past_pruning_points_store,
            header_selected_tip_store,
        }
    }

    pub fn next_pruning_points_and_candidate_by_ghostdag_data(
        &self,
        ghostdag_data: CompactGhostdagData,
        suggested_low_hash: Option<Hash>,
        current_candidate: Hash,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, Hash) {
        let low_hash = match suggested_low_hash {
            Some(suggested) => {
                if !self.reachability_service.is_chain_ancestor_of(suggested, current_candidate) {
                    assert!(self.reachability_service.is_chain_ancestor_of(current_candidate, suggested));
                    suggested
                } else {
                    current_candidate
                }
            }
            None => current_candidate,
        };

        // If the pruning point is more out of date than that, an IBD with headers proof is needed anyway.
        let mut new_pruning_points = Vec::with_capacity((self.pruning_depth / self.finality_depth) as usize);
        let mut latest_pruning_point_bs = self.ghostdag_store.get_blue_score(current_pruning_point).unwrap();

        if latest_pruning_point_bs + self.pruning_depth > ghostdag_data.blue_score {
            // The pruning point is not in depth of self.pruning_depth, so there's
            // no point in checking if it is required to update it. This can happen
            // because the virtual is not updated after IBD, so the pruning point
            // might be in depth less than self.pruning_depth.
            return (vec![], current_candidate);
        }

        let mut new_candidate = current_candidate;

        for selected_child in self.reachability_service.forward_chain_iterator(low_hash, ghostdag_data.selected_parent, true) {
            let selected_child_bs = self.ghostdag_store.get_blue_score(selected_child).unwrap();

            if ghostdag_data.blue_score - selected_child_bs < self.pruning_depth {
                break;
            }

            new_candidate = selected_child;
            let new_candidate_bs = selected_child_bs;

            if self.finality_score(new_candidate_bs) > self.finality_score(latest_pruning_point_bs) {
                new_pruning_points.push(new_candidate);
                latest_pruning_point_bs = new_candidate_bs;
            }
        }

        (new_pruning_points, new_candidate)
    }

    // finality_score is the number of finality intervals passed since
    // the given block.
    fn finality_score(&self, blue_score: u64) -> u64 {
        blue_score / self.finality_depth
    }

    pub fn expected_header_pruning_point(&self, ghostdag_data: CompactGhostdagData, pruning_info: PruningPointInfo) -> Hash {
        if ghostdag_data.selected_parent == self.genesis_hash {
            return self.genesis_hash;
        }

        let (current_pruning_point, current_candidate, current_pruning_point_index) = pruning_info.decompose();

        let sp_header_pp = self.headers_store.get_header(ghostdag_data.selected_parent).unwrap().pruning_point;
        let sp_header_pp_blue_score = self.headers_store.get_blue_score(sp_header_pp).unwrap();

        // If the block doesn't have the pruning in its selected chain we know for sure that it can't trigger a pruning point
        // change (we check the selected parent to take care of the case where the block is the virtual which doesn't have reachability data).
        let has_pruning_point_in_its_selected_chain =
            self.reachability_service.is_chain_ancestor_of(current_pruning_point, ghostdag_data.selected_parent);

        // Note: the pruning point from the POV of the current block is the first block in its chain that is in depth of self.pruning_depth and
        // its finality score is greater than the previous pruning point. This is why if the diff between finality_score(selected_parent.blue_score + 1) * finality_interval
        // and the current block blue score is less than self.pruning_depth we can know for sure that this block didn't trigger a pruning point change.
        let min_required_blue_score_for_next_pruning_point = (self.finality_score(sp_header_pp_blue_score) + 1) * self.finality_depth;
        let next_or_current_pp = if has_pruning_point_in_its_selected_chain
            && min_required_blue_score_for_next_pruning_point + self.pruning_depth <= ghostdag_data.blue_score
        {
            // If the selected parent pruning point is in the future of current global pruning point, then provide it as a suggestion
            let suggested_low_hash = self
                .reachability_service
                .is_dag_ancestor_of_result(current_pruning_point, sp_header_pp)
                .unwrap_option()
                .and_then(|b| if b { Some(sp_header_pp) } else { None });
            let (new_pruning_points, _) = self.next_pruning_points_and_candidate_by_ghostdag_data(
                ghostdag_data,
                suggested_low_hash,
                current_candidate,
                current_pruning_point,
            );

            new_pruning_points.last().copied().unwrap_or(current_pruning_point)
        } else {
            sp_header_pp
        };

        if self.is_pruning_point_in_pruning_depth(ghostdag_data.blue_score, next_or_current_pp) {
            return next_or_current_pp;
        }

        for i in (0..=current_pruning_point_index).rev() {
            let past_pp = self.past_pruning_points_store.get(i).unwrap();
            if self.is_pruning_point_in_pruning_depth(ghostdag_data.blue_score, past_pp) {
                return past_pp;
            }
        }

        self.genesis_hash
    }

    fn is_pruning_point_in_pruning_depth(&self, pov_blue_score: u64, pruning_point: Hash) -> bool {
        let pp_bs = self.headers_store.get_blue_score(pruning_point).unwrap();
        pov_blue_score >= pp_bs + self.pruning_depth
    }

    pub fn is_valid_pruning_point(&self, pp_candidate: Hash, hst: Hash) -> bool {
        if pp_candidate == self.genesis_hash {
            return true;
        }
        if !self.reachability_service.is_chain_ancestor_of(pp_candidate, hst) {
            return false;
        }

        let hst_bs = self.ghostdag_store.get_blue_score(hst).unwrap();
        self.is_pruning_point_in_pruning_depth(hst_bs, pp_candidate)
    }

    pub fn are_pruning_points_in_valid_chain(&self, pruning_info: PruningPointInfo, hst: Hash) -> bool {
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
        for current in self.reachability_service.backward_chain_iterator(hst, pruning_info.pruning_point, false) {
            let current_header = self.headers_store.get_header(current).unwrap();
            if expected_pps_queue.back().is_none_or(|&&h| h != current_header.pruning_point) {
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

#[cfg(test)]
mod tests {
    // TODO: add unit-tests for next_pruning_point_and_candidate_by_block_hash and expected_header_pruning_point
}
