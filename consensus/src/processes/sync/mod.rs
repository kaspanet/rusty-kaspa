use std::{cmp::min, ops::Deref, sync::Arc};

use itertools::Itertools;
use kaspa_consensus_core::{
    config::params::ForkedParam,
    errors::sync::{SyncManagerError, SyncManagerResult},
};
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use kaspa_math::uint::malachite_base::num::arithmetic::traits::CeilingLogBase2;
use parking_lot::RwLock;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        ghostdag::GhostdagStoreReader, headers_selected_tip::HeadersSelectedTipStoreReader, pruning::PruningStoreReader,
        reachability::ReachabilityStoreReader, relations::RelationsStoreReader, selected_chain::SelectedChainStoreReader,
        statuses::StatusesStoreReader,
    },
};

use super::traversal_manager::DagTraversalManager;

#[derive(Clone)]
pub struct SyncManager<
    S: RelationsStoreReader,
    T: ReachabilityStoreReader,
    U: GhostdagStoreReader,
    V: SelectedChainStoreReader,
    W: HeadersSelectedTipStoreReader,
    X: PruningStoreReader,
    Y: StatusesStoreReader,
> {
    mergeset_size_limit: ForkedParam<u64>,
    reachability_service: MTReachabilityService<T>,
    traversal_manager: DagTraversalManager<U, T, S>,
    ghostdag_store: Arc<U>,
    selected_chain_store: Arc<RwLock<V>>,
    header_selected_tip_store: Arc<RwLock<W>>,
    pruning_point_store: Arc<RwLock<X>>,
    statuses_store: Arc<RwLock<Y>>,
}

impl<
        S: RelationsStoreReader,
        T: ReachabilityStoreReader,
        U: GhostdagStoreReader,
        V: SelectedChainStoreReader,
        W: HeadersSelectedTipStoreReader,
        X: PruningStoreReader,
        Y: StatusesStoreReader,
    > SyncManager<S, T, U, V, W, X, Y>
{
    pub fn new(
        mergeset_size_limit: ForkedParam<u64>,
        reachability_service: MTReachabilityService<T>,
        traversal_manager: DagTraversalManager<U, T, S>,
        ghostdag_store: Arc<U>,
        selected_chain_store: Arc<RwLock<V>>,
        header_selected_tip_store: Arc<RwLock<W>>,
        pruning_point_store: Arc<RwLock<X>>,
        statuses_store: Arc<RwLock<Y>>,
    ) -> Self {
        Self {
            mergeset_size_limit,
            reachability_service,
            traversal_manager,
            ghostdag_store,
            selected_chain_store,
            header_selected_tip_store,
            pruning_point_store,
            statuses_store,
        }
    }

    /// Returns the hashes of the blocks between low's antipast and high's antipast, or up to `max_blocks`, if provided.
    /// The result excludes low and includes high. If low == high, returns nothing. If max_blocks is some then it MUST be >= MergeSetSizeLimit
    /// because it returns blocks with MergeSet granularity, so if MergeSet > max_blocks, the function will return nothing which is undesired behavior.
    pub fn antipast_hashes_between(&self, low: Hash, high: Hash, max_blocks: Option<usize>) -> (Vec<Hash>, Hash) {
        let max_blocks = max_blocks.unwrap_or(usize::MAX);
        assert!(max_blocks >= self.mergeset_size_limit.after() as usize);

        // If low is not in the chain of high - forward_chain_iterator will fail.
        // Therefore, we traverse down low's chain until we reach a block that is in
        // high's chain.
        // We keep original_low to filter out blocks in its past later down the road
        let original_low = low;
        let low = self.find_highest_common_chain_block(low, high);

        let low_bs = self.ghostdag_store.get_blue_score(low).unwrap();
        let high_bs = self.ghostdag_store.get_blue_score(high).unwrap();
        assert!(low_bs <= high_bs);

        let mut highest_reached = low; // The highest chain block we reached before completing/reaching a limit
        let mut blocks = Vec::with_capacity(min(max_blocks, (high_bs - low_bs) as usize));
        for current in self.reachability_service.forward_chain_iterator(low, high, true).skip(1) {
            let gd = self.ghostdag_store.get_data(current).unwrap();
            if blocks.len() + gd.mergeset_size() > max_blocks {
                break;
            }
            blocks.extend(
                gd.consensus_ordered_mergeset(self.ghostdag_store.deref())
                    .filter(|hash| !self.reachability_service.is_dag_ancestor_of(*hash, original_low)),
            );
            highest_reached = current;
        }

        // The process above doesn't return `highest_reached`, so include it explicitly unless it is `low`
        if low != highest_reached {
            blocks.push(highest_reached);
        }

        (blocks, highest_reached)
    }

    pub fn find_highest_common_chain_block(&self, low: Hash, high: Hash) -> Hash {
        self.reachability_service
            .default_backward_chain_iterator(low)
            .find(|candidate| self.reachability_service.is_chain_ancestor_of(*candidate, high))
            .expect("because of the pruning rules such block has to exist")
    }

    /// Returns a logarithmic amount of blocks sampled from the virtual selected chain between `low` and `high`.
    /// Expects both blocks to be on the virtual selected chain, otherwise an error is returned
    pub fn create_virtual_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> SyncManagerResult<Vec<Hash>> {
        let low = low.unwrap_or_else(|| self.pruning_point_store.read().pruning_point().unwrap());
        let sc_read = self.selected_chain_store.read();
        let high = high.unwrap_or_else(|| sc_read.get_tip().unwrap().1);
        if low == high {
            return Ok(vec![low]);
        }

        let low_index = match sc_read.get_by_hash(low).unwrap_option() {
            Some(index) => index,
            None => return Err(SyncManagerError::BlockNotInSelectedParentChain(low)),
        };

        let high_index = match sc_read.get_by_hash(high).unwrap_option() {
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
            locator.push(sc_read.get_by_index(current_index).unwrap());
            if current_index < step {
                break;
            }

            current_index -= step;
            step *= 2;
        }

        locator.push(low);
        Ok(locator)
    }

    pub fn get_missing_block_body_hashes(&self, high: Hash) -> SyncManagerResult<Vec<Hash>> {
        let pp = self.pruning_point_store.read().pruning_point().unwrap();
        if !self.reachability_service.is_chain_ancestor_of(pp, high) {
            return Err(SyncManagerError::PruningPointNotInChain(pp, high));
        }

        let mut highest_with_body = None;
        let mut forward_iterator = self.reachability_service.forward_chain_iterator(pp, high, true).tuple_windows();
        let mut backward_iterator = self.reachability_service.backward_chain_iterator(high, pp, true);
        loop {
            // We loop from both directions in parallel in order to use the shorter path
            let Some((parent, current)) = forward_iterator.next() else {
                break;
            };
            let status = self.statuses_store.read().get(current).unwrap();
            if status.is_header_only() {
                // Going up, the first parent which has a header-only child is our target
                highest_with_body = Some(parent);
                break;
            }

            let Some(backward_current) = backward_iterator.next() else {
                break;
            };
            let status = self.statuses_store.read().get(backward_current).unwrap();
            if status.has_block_body() {
                // Since this iterator is going down, current must be the highest with body
                highest_with_body = Some(backward_current);
                break;
            }
        }

        if highest_with_body.is_none_or(|h| h == high) {
            return Ok(vec![]);
        };

        let (mut hashes_between, _) = self.antipast_hashes_between(highest_with_body.unwrap(), high, None);
        let statuses = self.statuses_store.read();
        hashes_between.retain(|&h| statuses.get(h).unwrap().is_header_only());

        Ok(hashes_between)
    }

    pub fn create_block_locator_from_pruning_point(
        &self,
        high: Hash,
        low: Hash,
        limit: Option<usize>,
    ) -> SyncManagerResult<Vec<Hash>> {
        if !self.reachability_service.is_chain_ancestor_of(low, high) {
            return Err(SyncManagerError::LocatorLowHashNotInHighHashChain(low, high));
        }

        let low_bs = self.ghostdag_store.get_blue_score(low).unwrap();
        let mut current = high;
        let mut step = 1;
        let mut locator = Vec::new();
        loop {
            locator.push(current);
            if limit == Some(locator.len()) {
                break;
            }

            let current_gd = self.ghostdag_store.get_compact_data(current).unwrap();

            // Nothing more to add once the low node has been added.
            if current_gd.blue_score <= low_bs {
                break;
            }

            // Calculate blue score of previous block to include ensuring the
            // final block is `low`.
            let next_bs = if current_gd.blue_score < step || current_gd.blue_score - step < low_bs {
                low_bs
            } else {
                current_gd.blue_score - step
            };

            // Walk down current's selected parent chain to the appropriate ancestor
            current = self.traversal_manager.lowest_chain_block_above_or_equal_to_blue_score(current, next_bs);

            // Double the distance between included hashes
            step *= 2;
        }

        Ok(locator)
    }
}
