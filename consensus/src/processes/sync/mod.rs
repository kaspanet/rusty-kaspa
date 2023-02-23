use std::{cmp::max, iter::once, sync::Arc};

use hashes::Hash;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{ghostdag::GhostdagStoreReader, reachability::ReachabilityStoreReader},
};

#[derive(Clone)]
pub struct SyncManager<T: ReachabilityStoreReader, U: GhostdagStoreReader> {
    mergeset_size_limit: usize,
    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<U>,
}

impl<T: ReachabilityStoreReader, U: GhostdagStoreReader> SyncManager<T, U> {
    pub fn new(mergeset_size_limit: usize, reachability_service: MTReachabilityService<T>, ghostdag_store: Arc<U>) -> Self {
        Self { mergeset_size_limit, reachability_service, ghostdag_store }
    }

    pub fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: Option<usize>) -> (Vec<Hash>, Hash) {
        assert!(match max_blocks {
            Some(max_blocks) => max_blocks >= self.mergeset_size_limit,
            None => true,
        });

        let low_bs = self.ghostdag_store.get_blue_score(low).unwrap();
        let high_bs = self.ghostdag_store.get_blue_score(high).unwrap();
        assert!(low_bs <= high_bs);

        // If low is not in the chain of high - forward_chain_iterator will fail.
        // Therefore, we traverse down low's chain until we reach a block that is in
        // high's chain.
        // We keep originalLow to filter out blocks in its past later down the road
        let original_low = low;
        let low = self.find_higher_common_chain_block(low, high);
        let mut highest = None;
        let mut blocks = Vec::with_capacity(match max_blocks {
            Some(max_blocks) => max(max_blocks, (high_bs - low_bs) as usize),
            None => (high_bs - low_bs) as usize,
        });
        for current in self.reachability_service.forward_chain_iterator(low, high, false) {
            let gd = self.ghostdag_store.get_data(current).unwrap();
            if let Some(max_blocks) = max_blocks {
                if blocks.len() + gd.mergeset_size() > max_blocks {
                    break;
                }
            }

            highest = Some(current);
            blocks.extend(
                once(gd.selected_parent)
                    .chain(gd.ascending_mergeset_without_selected_parent(&*self.ghostdag_store).map(|sb| sb.hash))
                    .filter(|hash| self.reachability_service.is_dag_ancestor_of(*hash, original_low)),
            );
        }

        // The process above doesn't return highHash, so include it explicitly, unless highHash == lowHash
        let highest = highest.expect("`blocks` should have at least one block");
        if low != highest {
            blocks.push(highest);
        }

        (blocks, highest)
    }

    fn find_higher_common_chain_block(&self, low: Hash, high: Hash) -> Hash {
        self.reachability_service
            .default_backward_chain_iterator(low)
            .find(|candidate| self.reachability_service.is_chain_ancestor_of(*candidate, high))
            .expect("because of the pruning rules such block has to exist")
    }
}
