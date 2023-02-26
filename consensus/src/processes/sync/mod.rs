use std::{cmp::max, iter::once, sync::Arc};

use consensus_core::errors::sync::{SyncManagerError, SyncManagerResult};
use database::prelude::StoreResultExtensions;
use hashes::Hash;
use math::uint::malachite_base::num::arithmetic::traits::CeilingLogBase2;
use parking_lot::RwLock;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        ghostdag::GhostdagStoreReader, headers_selected_tip::HeadersSelectedTipStoreReader, pruning::PruningStoreReader,
        reachability::ReachabilityStoreReader, selected_chain::SelectedChainStoreReader,
    },
};

#[derive(Clone)]
pub struct SyncManager<
    T: ReachabilityStoreReader,
    U: GhostdagStoreReader,
    V: SelectedChainStoreReader,
    W: HeadersSelectedTipStoreReader,
    X: PruningStoreReader,
> {
    mergeset_size_limit: usize,
    reachability_service: MTReachabilityService<T>,
    ghostdag_store: Arc<U>,
    selected_chain_store: Arc<RwLock<V>>,
    header_selected_tip_store: Arc<RwLock<W>>,
    pruning_store: Arc<RwLock<X>>,
}

impl<
        T: ReachabilityStoreReader,
        U: GhostdagStoreReader,
        V: SelectedChainStoreReader,
        W: HeadersSelectedTipStoreReader,
        X: PruningStoreReader,
    > SyncManager<T, U, V, W, X>
{
    pub fn new(
        mergeset_size_limit: usize,
        reachability_service: MTReachabilityService<T>,
        ghostdag_store: Arc<U>,
        selected_chain_store: Arc<RwLock<V>>,
        header_selected_tip_store: Arc<RwLock<W>>,
        pruning_store: Arc<RwLock<X>>,
    ) -> Self {
        Self {
            mergeset_size_limit,
            reachability_service,
            ghostdag_store,
            selected_chain_store,
            header_selected_tip_store,
            pruning_store,
        }
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

    pub fn create_headers_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> SyncManagerResult<Vec<Hash>> {
        let sc_read_guard = self.selected_chain_store.read();
        let hst_read_guard = self.header_selected_tip_store.read();
        let pp_read_guard = self.pruning_store.read();

        let low = low.unwrap_or_else(|| pp_read_guard.get().unwrap().pruning_point);
        let high = high.unwrap_or_else(|| hst_read_guard.get().unwrap().hash);

        if low == high {
            return Ok(vec![low]);
        }

        let low_index = match sc_read_guard.get_by_hash(low).unwrap_option() {
            Some(index) => index,
            None => return Err(SyncManagerError::BlockNotInSelectedParentChain(low)),
        };

        let high_index = match sc_read_guard.get_by_hash(high).unwrap_option() {
            Some(index) => index,
            None => return Err(SyncManagerError::BlockNotInSelectedParentChain(high)),
        };

        if low_index > high_index {
            return Err(SyncManagerError::LowHashHigherThanHighHash(low, high));
        }

        let mut locator = Vec::with_capacity((high_index - low_index).ceiling_log_base_2() as usize);
        let mut step = 1;
        let mut current_index = high_index;
        while current_index > low_index {
            locator.push(sc_read_guard.get_by_index(current_index).unwrap());
            if current_index < step {
                break;
            }

            current_index -= step;
            step *= 2;
        }

        Ok(locator)
    }
}
