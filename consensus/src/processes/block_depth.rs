use consensus_core::blockhash::ORIGIN;
use hashes::Hash;
use std::sync::Arc;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        depth::DepthStoreReader,
        ghostdag::{GhostdagData, GhostdagStoreReader},
        reachability::ReachabilityStoreReader,
    },
};

#[derive(Clone)]
pub struct BlockDepthManager<S: DepthStoreReader, U: ReachabilityStoreReader, V: GhostdagStoreReader> {
    merge_depth: u64,
    finality_depth: u64,
    genesis_hash: Hash,
    depth_store: Arc<S>,
    reachability_service: MTReachabilityService<U>,
    ghostdag_store: Arc<V>,
}

impl<S: DepthStoreReader, U: ReachabilityStoreReader, V: GhostdagStoreReader> BlockDepthManager<S, U, V> {
    pub fn new(
        merge_depth: u64,
        finality_depth: u64,
        genesis_hash: Hash,
        depth_store: Arc<S>,
        reachability_service: MTReachabilityService<U>,
        ghostdag_store: Arc<V>,
    ) -> Self {
        Self { merge_depth, finality_depth, genesis_hash, depth_store, reachability_service, ghostdag_store }
    }
    pub fn calc_merge_depth_root(&self, ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        self.calculate_block_at_depth(ghostdag_data, self.merge_depth, pruning_point)
    }

    pub fn calc_finality_point(&self, ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        self.calculate_block_at_depth(ghostdag_data, self.finality_depth, pruning_point)
    }

    fn calculate_block_at_depth(&self, ghostdag_data: &GhostdagData, depth: u64, pruning_point: Hash) -> Hash {
        assert!(depth == self.merge_depth || depth == self.finality_depth);

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

        let mut current = if depth == self.merge_depth {
            self.depth_store.merge_depth_root(ghostdag_data.selected_parent).unwrap()
        } else {
            self.depth_store.finality_point(ghostdag_data.selected_parent).unwrap()
        };

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
            .cloned()
            .filter(move |blue| self.reachability_service.is_chain_ancestor_of(merge_depth_root, *blue))
    }
}
