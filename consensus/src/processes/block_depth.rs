use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, ORIGIN},
    config::params::ForkedParam,
};
use kaspa_hashes::Hash;
use std::sync::Arc;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        depth::DepthStoreReader,
        ghostdag::{GhostdagData, GhostdagStoreReader},
        headers::HeaderStoreReader,
        reachability::ReachabilityStoreReader,
    },
};

enum BlockDepthType {
    MergeRoot,
    Finality,
}

#[derive(Clone)]
pub struct BlockDepthManager<S: DepthStoreReader, U: ReachabilityStoreReader, V: GhostdagStoreReader, T: HeaderStoreReader> {
    merge_depth: ForkedParam<u64>,
    finality_depth: ForkedParam<u64>,
    genesis_hash: Hash,
    depth_store: Arc<S>,
    reachability_service: MTReachabilityService<U>,
    ghostdag_store: Arc<V>,
    headers_store: Arc<T>,
}

impl<S: DepthStoreReader, U: ReachabilityStoreReader, V: GhostdagStoreReader, T: HeaderStoreReader> BlockDepthManager<S, U, V, T> {
    pub fn new(
        merge_depth: ForkedParam<u64>,
        finality_depth: ForkedParam<u64>,
        genesis_hash: Hash,
        depth_store: Arc<S>,
        reachability_service: MTReachabilityService<U>,
        ghostdag_store: Arc<V>,
        headers_store: Arc<T>,
    ) -> Self {
        Self { merge_depth, finality_depth, genesis_hash, depth_store, reachability_service, ghostdag_store, headers_store }
    }
    pub fn calc_merge_depth_root(&self, ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        self.calculate_block_at_depth(ghostdag_data, BlockDepthType::MergeRoot, pruning_point)
    }

    pub fn calc_finality_point(&self, ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        self.calculate_block_at_depth(ghostdag_data, BlockDepthType::Finality, pruning_point)
    }

    fn calculate_block_at_depth(&self, ghostdag_data: &GhostdagData, depth_type: BlockDepthType, pruning_point: Hash) -> Hash {
        if ghostdag_data.selected_parent.is_origin() {
            return ORIGIN;
        }
        let depth = match depth_type {
            BlockDepthType::MergeRoot => self.merge_depth.after(),
            BlockDepthType::Finality => self.finality_depth.after(),
        };
        if ghostdag_data.blue_score < depth {
            return self.genesis_hash;
        }

        let pp_bs = self.ghostdag_store.get_blue_score(pruning_point).unwrap();

        if ghostdag_data.blue_score < pp_bs + depth {
            return ORIGIN;
        }

        if !self.reachability_service.is_chain_ancestor_of(pruning_point, ghostdag_data.selected_parent) {
            return ORIGIN;
        }

        // [Crescendo]: we start from the depth/finality point of the selected parent. This makes the selection monotonic
        // also when the depth increases in the fork activation point. The loop below will simply not progress for a while,
        // until a new block above the previous point reaches the *new increased depth*.
        let mut current = match depth_type {
            BlockDepthType::MergeRoot => self.depth_store.merge_depth_root(ghostdag_data.selected_parent).unwrap(),
            BlockDepthType::Finality => self.depth_store.finality_point(ghostdag_data.selected_parent).unwrap(),
        };

        // In this case we expect the pruning point or a block above it to be the block at depth.
        // Note that above we already verified the chain and distance conditions for this.
        // Additionally observe that if `current` is a valid hash it must not be pruned for the same reason.
        if current == ORIGIN {
            current = pruning_point;
        }

        let required_blue_score = ghostdag_data.blue_score - depth;

        for chain_block in self.reachability_service.forward_chain_iterator(current, ghostdag_data.selected_parent, true) {
            if self.ghostdag_store.get_blue_score(chain_block).unwrap() >= required_blue_score {
                break;
            }

            current = chain_block;
        }

        current
    }

    /// Returns the set of blues which are eligible for "kosherizing" merge bound violating blocks.
    /// By prunality rules, these blocks must have `merge_depth_root` on their selected chain.  
    pub fn kosherizing_blues<'a>(
        &'a self,
        ghostdag_data: &'a GhostdagData,
        merge_depth_root: Hash,
    ) -> impl DoubleEndedIterator<Item = Hash> + 'a {
        ghostdag_data
            .mergeset_blues
            .iter()
            .copied()
            .filter(move |blue| self.reachability_service.is_chain_ancestor_of(merge_depth_root, *blue))
    }
}
