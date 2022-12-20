use std::sync::Arc;

use super::reachability::ReachabilityResultExtensions;
use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        ghostdag::{CompactGhostdagData, GhostdagStoreReader},
        headers::HeaderStoreReader,
        past_pruning_points::PastPruningPointsStoreReader,
        pruning::PruningPointInfo,
        reachability::ReachabilityStoreReader,
    },
};
use hashes::Hash;

#[derive(Clone)]
pub struct PruningManager<S: GhostdagStoreReader, T: ReachabilityStoreReader, U: HeaderStoreReader, V: PastPruningPointsStoreReader> {
    pruning_depth: u64,
    finality_depth: u64,
    genesis_hash: Hash,

    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<S>,
    headers_store: Arc<U>,
    past_pruning_points_store: Arc<V>,
}

impl<S: GhostdagStoreReader, T: ReachabilityStoreReader, U: HeaderStoreReader, V: PastPruningPointsStoreReader>
    PruningManager<S, T, U, V>
{
    pub fn new(
        pruning_depth: u64,
        finality_depth: u64,
        genesis_hash: Hash,
        reachability_service: MTReachabilityService<T>,
        ghostdag_store: Arc<S>,
        headers_store: Arc<U>,
        past_pruning_points_store: Arc<V>,
    ) -> Self {
        Self {
            pruning_depth,
            finality_depth,
            genesis_hash,
            reachability_service,
            ghostdag_store,
            headers_store,
            past_pruning_points_store,
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
}

#[cfg(test)]
mod tests {
    // TODO: add unit-tests for next_pruning_point_and_candidate_by_block_hash and expected_header_pruning_point
}
