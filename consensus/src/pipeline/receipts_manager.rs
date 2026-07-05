use super::receipts_errors::ReceiptsErrors;
use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{
        acceptance_data::AcceptanceDataStoreReader, headers::HeaderStoreReader, reachability::ReachabilityStoreReader,
        smt_metadata::DbSmtMetadataStore,
    },
};
use crate::{
    consensus::services::DbDagTraversalManager,
    model::stores::{
        block_transactions::BlockTransactionsStoreReader, pruning::PruningStoreReader, selected_chain::SelectedChainStoreReader,
    },
};
use kaspa_consensus_core::{
    config::{genesis::GenesisBlock, params::ForkActivation},
    header::Header,
    receipts::{AcceptingBlkContext, LaneTipUpdateContext, PosterityReceiptContext, TxContext, TxReceipt},
};
use kaspa_hashes::Hash;
use kaspa_merkle::create_merkle_witness;
use kaspa_seq_commit::{
    hashing::{activity_digest_lane, activity_leaf, lane_key, mergeset_context_hash},
    types::MergesetContext,
};
use kaspa_smt_store::processor::{SmtReadBounds, SmtStores};

use parking_lot::RwLock;

use std::{cmp::min, sync::Arc};
#[derive(Clone)]
pub struct TxReceiptsManager<
    T: SelectedChainStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    X: AcceptanceDataStoreReader,
    W: BlockTransactionsStoreReader,
    Y: PruningStoreReader,
> {
    pub genesis: GenesisBlock,

    pub posterity_depth: u64,
    pub reachability_service: MTReachabilityService<U>,

    pub headers_store: Arc<V>,
    pub selected_chain_store: Arc<RwLock<T>>,
    pub acceptance_data_store: Arc<X>,
    pub block_transactions_store: Arc<W>,
    pub pruning_point_store: Arc<RwLock<Y>>,
    pub smt_stores: Arc<SmtStores>,
    pub smt_metadata_store: Arc<DbSmtMetadataStore>,

    pub traversal_manager: DbDagTraversalManager,
}

impl<
    T: SelectedChainStoreReader,
    U: ReachabilityStoreReader,
    V: HeaderStoreReader,
    X: AcceptanceDataStoreReader,
    W: BlockTransactionsStoreReader,
    Y: PruningStoreReader,
> TxReceiptsManager<T, U, V, X, W, Y>
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis: GenesisBlock,
        posterity_depth: u64,
        reachability_service: MTReachabilityService<U>,
        headers_store: Arc<V>,
        selected_chain_store: Arc<RwLock<T>>,
        acceptance_data_store: Arc<X>,
        block_transactions_store: Arc<W>,
        pruning_point_store: Arc<RwLock<Y>>,
        smt_stores: Arc<SmtStores>,
        smt_metadata_store: Arc<DbSmtMetadataStore>,
        traversal_manager: DbDagTraversalManager,
    ) -> Self {
        Self {
            genesis: genesis.clone(),
            posterity_depth,
            headers_store,
            selected_chain_store: selected_chain_store.clone(),
            acceptance_data_store: acceptance_data_store.clone(),
            reachability_service,
            block_transactions_store: block_transactions_store.clone(),
            pruning_point_store: pruning_point_store.clone(),
            smt_stores,
            smt_metadata_store,
            traversal_manager: traversal_manager.clone(),
        }
    }

    pub fn generate_tx_receipt(&self, accepting_block_header: Arc<Header>, tracked_tx_id: Hash) -> Result<TxReceipt, ReceiptsErrors> {
        let selected_parent = self.reachability_service.get_chain_parent(accepting_block_header.hash);
        let selected_parent_header = self.headers_store.get_header(selected_parent)?;
        let posterity_block = self.get_post_posterity_block(selected_parent)?;
        let posterity_header = self.headers_store.get_header(posterity_block)?;

        let accepting_entries = self.collect_acceptance_entries(accepting_block_header.hash)?;
        let tracked_entry = self.find_tracked_entry(&accepting_entries, tracked_tx_id, accepting_block_header.hash)?;
        let lane_key_hash = lane_key(&tracked_entry.lane_id);

        let tx_context = TxContext { tracked_tx_id, tx_version: tracked_entry.version, tx_merge_idx: tracked_entry.merge_idx };
        let accepting_blk_context = self.build_accepting_blk_context(
            &accepting_block_header,
            &accepting_entries,
            tracked_entry,
            lane_key_hash,
            selected_parent,
            &selected_parent_header,
        )?;

        let lane_tip_updates_to_posterity =
            self.build_lane_tip_updates_to_posterity(accepting_block_header.hash, posterity_block, &tracked_entry.lane_id)?;
        let posterity_context = self.build_posterity_materials(posterity_block, posterity_header.blue_score, lane_key_hash)?;

        Ok(TxReceipt::new(
            posterity_block,
            tx_context,
            accepting_blk_context,
            lane_tip_updates_to_posterity,
            lane_key_hash,
            posterity_context,
        ))
    }

    pub fn verify_tx_receipt(&self, tx_receipt: &TxReceipt) -> bool {
        if !self.verify_is_posterity(tx_receipt.posterity_hash) {
            return false;
        }

        let Ok(post_posterity_header) = self.headers_store.get_header(tx_receipt.posterity_hash) else {
            return false;
        };
        tx_receipt.verify_receipt(post_posterity_header.accepted_id_merkle_root)
    }

    fn block_context_hash(&self, block_header: &Header) -> Result<Hash, ReceiptsErrors> {
        // Parallel to the context construction used in seq-commit processing:
        // mergeset_context_hash(selected_parent.timestamp, block.daa_score, block.blue_score).
        let selected_parent_hash = self.reachability_service.get_chain_parent(block_header.hash);
        let selected_parent_header = self.headers_store.get_header(selected_parent_hash)?;
        Ok(mergeset_context_hash(&MergesetContext {
            timestamp: selected_parent_header.timestamp,
            daa_score: block_header.daa_score,
            blue_score: block_header.blue_score,
        }))
    }

    fn inactivity_shortcut_hash(&self, inactivity_shortcut_block: Hash) -> Result<Hash, ReceiptsErrors> {
        // Parallel to virtual_processor::inactivity_shortcut.
        let shortcut_header = self.headers_store.get_header(inactivity_shortcut_block)?;
            Ok(shortcut_header.accepted_id_merkle_root)

    }

    fn build_accepting_blk_context(
        &self,
        accepting_block_header: &Arc<Header>,
        accepting_entries: &[AcceptanceEntry],
        tracked_entry: AcceptanceEntry,
        lane_key_hash: Hash,
        selected_parent: Hash,
        selected_parent_header: &Arc<Header>,
    ) -> Result<AcceptingBlkContext, ReceiptsErrors> {
        let lane_leaves = accepting_entries
            .iter()
            .filter(|e| e.lane_id == tracked_entry.lane_id)
            .map(|e| activity_leaf(&e.tx_id, e.version, e.merge_idx))
            .collect::<Vec<_>>();
        let tracked_leaf = activity_leaf(&tracked_entry.tx_id, tracked_entry.version, tracked_entry.merge_idx);
        let tx_acceptance_proof = create_merkle_witness(lane_leaves.iter().copied(), tracked_leaf, false)?;

        let accepting_context_hash = self.block_context_hash(accepting_block_header)?;
        let parent_bounds = SmtReadBounds::for_pov(selected_parent_header.blue_score, self.posterity_depth);
        let parent_ref = self
            .smt_stores
            .get_lane(lane_key_hash, parent_bounds, |bh| self.reachability_service.is_chain_ancestor_of(bh, selected_parent))
            .map(|v| *v.data())
            .unwrap_or(selected_parent_header.accepted_id_merkle_root);

        Ok(AcceptingBlkContext { tx_acceptance_proof, context_hash: accepting_context_hash, parent_ref })
    }

    fn build_lane_tip_updates_to_posterity(
        &self,
        accepting_block_hash: Hash,
        posterity_block: Hash,
        lane_id: &[u8; 20],
    ) -> Result<Vec<LaneTipUpdateContext>, ReceiptsErrors> {
        let mut updates = Vec::new();

        for chain_block in self.reachability_service.forward_chain_iterator(accepting_block_hash, posterity_block, true).skip(1) {
            let chain_header = self.headers_store.get_header(chain_block)?;
            let lane_activity = self
                .collect_acceptance_entries(chain_block)?
                .into_iter()
                .filter(|e| &e.lane_id == lane_id)
                .map(|e| activity_leaf(&e.tx_id, e.version, e.merge_idx))
                .collect::<Vec<_>>();
            if lane_activity.is_empty() {
                continue;
            }

            updates.push(LaneTipUpdateContext {
                activity_digest: activity_digest_lane(lane_activity.into_iter()),
                context_hash: self.block_context_hash(&chain_header)?,
            });
        }

        Ok(updates)
    }

    fn build_posterity_materials(
        &self,
        posterity_block: Hash,
        posterity_blue_score: u64,
        lane_key_hash: Hash,
    ) -> Result<PosterityReceiptContext, ReceiptsErrors> {
        // Parallel to consensus::get_seq_commit_lane_proof lane proof retrieval.
        let posterity_bounds = SmtReadBounds::for_pov(posterity_blue_score, self.posterity_depth);
        let lane_in_active_lanes_proof = self
            .smt_stores
            .prove_lane(&lane_key_hash, posterity_bounds, |bh| self.reachability_service.is_chain_ancestor_of(bh, posterity_block))?;
        let Some(posterity_lane) = self
            .smt_stores
            .get_lane(lane_key_hash, posterity_bounds, |bh| self.reachability_service.is_chain_ancestor_of(bh, posterity_block))
        else {
            return Err(ReceiptsErrors::LaneNotActiveAtPosterity { lane_key: lane_key_hash, posterity_hash: posterity_block });
        };

        let posterity_metadata = self.smt_metadata_store.get(posterity_block)?;
        let inactivity_shortcut = self.inactivity_shortcut_hash(posterity_metadata.inactivity_shortcut_block())?;
        let parent_sqc =
            self.headers_store.get_header(self.reachability_service.get_chain_parent(posterity_block))?.accepted_id_merkle_root;

        Ok(PosterityReceiptContext {
            lane_in_active_lanes_proof,
            lane_blue_score: posterity_lane.blue_score(),
            inactivity_shortcut,
            payload_and_ctx_digest: posterity_metadata.payload_and_ctx_digest(),
            parent_sqc,
        })
    }

    fn find_tracked_entry(
        &self,
        accepting_entries: &[AcceptanceEntry],
        tracked_tx_id: Hash,
        accepting_block_hash: Hash,
    ) -> Result<AcceptanceEntry, ReceiptsErrors> {
        accepting_entries
            .iter()
            .find(|e| e.tx_id == tracked_tx_id)
            .copied()
            .ok_or(ReceiptsErrors::TrackedTxNotFoundInAcceptanceData(tracked_tx_id, accepting_block_hash))
    }

    fn collect_acceptance_entries(&self, block_hash: Hash) -> Result<Vec<AcceptanceEntry>, ReceiptsErrors> {
        let acceptance_data = self.acceptance_data_store.get(block_hash)?;
        let mut merge_idx: u32 = 0;
        let mut out = Vec::new();

        for mergeset_block in acceptance_data.iter() {
            let txs = self.block_transactions_store.get(mergeset_block.block_hash)?;
            for accepted in mergeset_block.accepted_transactions.iter() {
                let idx = accepted.index_within_block as usize;
                let Some(tx) = txs.get(idx) else {
                    return Err(ReceiptsErrors::InvalidAcceptedTxIndex {
                        block_hash: mergeset_block.block_hash,
                        index: accepted.index_within_block,
                    });
                };
                out.push(AcceptanceEntry {
                    tx_id: accepted.transaction_id,
                    version: tx.version,
                    merge_idx,
                    lane_id: *tx.subnetwork_id.as_ref(),
                });
                merge_idx = merge_idx.saturating_add(1);
            }
        }

        Ok(out)
    }

    // The function assumes that the path from block_hash up to its post posterity if it exits is intact and has not been pruned
    // it will panic if not;
    // An error is returned if post_posterity does not yet exist;
    // The function  assumes block_hash is a chain block.
    // The get_post_posterity_block on a posterity block, will not return the block itself rather the posterity after it.
    pub fn get_post_posterity_block(&self, block_hash: Hash) -> Result<Hash, ReceiptsErrors> {
        // try and reach the first proceeding selected chain block,
        // in the majority of cases, a very short distance is covered before reaching a chain block.
        let candidate_block = block_hash;
        let candidate_bscore = self.headers_store.get_blue_score(candidate_block)?;

        let head_hash = self.selected_chain_store.read().get_tip()?.1;
        let head_bscore = self.headers_store.get_blue_score(head_hash).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth + self.posterity_depth;
        if cutoff_bscore > head_bscore {
            // Posterity block not yet available.
            Err(ReceiptsErrors::PosterityDoesNotExistYet(block_hash))
        } else {
            Ok(self.get_chain_block_by_cutoff_bscore(candidate_block, cutoff_bscore, self.posterity_depth))
        }
    }
    pub fn get_pre_posterity_block_by_hash(&self, block_hash: Hash) -> Hash {
        if block_hash == self.genesis.hash {
            return block_hash;
        }
        let parent_hash = self.reachability_service.get_chain_parent(block_hash);
        self.get_pre_posterity_block_by_parent(parent_hash)
    }

    // The function assumes that the path from block_parent_hash down to its posterity is intact and has not been pruned
    // (which should be the same as assuming block_hash has not been pruned)
    // it will panic if not.
    // The function does not assume block_hash is a chain block, however
    // the known aplications of posterity blocks appear nonsensical when it is not
    // The pre posterity of a posterity block, is not the block itself rather the posterity before it
    // The posterity of genesis is defined to be genesis
    pub fn get_pre_posterity_block_by_parent(&self, block_parent_hash: Hash) -> Hash {
        let block_bscore = self.headers_store.get_blue_score(block_parent_hash).unwrap();

        let tentative_cutoff_bscore = block_bscore - block_bscore % self.posterity_depth;
        if tentative_cutoff_bscore == 0
        // Genesis block edge case
        {
            return self.genesis.hash;
        }
        // try and reach the first preceding selected chain block,
        // while checking if pre_posterity of queried block is of the rare case
        // where it is encountered before arriving at a chain block
        // in the majority of cases, a very short distance is covered before reaching a chain block
        let candidate_block = self
            .reachability_service
            .default_backward_chain_iterator(block_parent_hash)
            .find(|&block| {
                self.headers_store.get_blue_score(block).unwrap() < tentative_cutoff_bscore
                    || self.selected_chain_store.read().get_by_hash(block).is_ok()
            })
            .unwrap();
        // In case cutoff_bscore was crossed prior to reaching a chain block
        if self.headers_store.get_blue_score(candidate_block).unwrap() < tentative_cutoff_bscore {
            let posterity_parent = candidate_block;
            return self.reachability_service.get_next_chain_ancestor(block_parent_hash, posterity_parent);
        }
        // Othrewise, recalculate the cutoff score in accordance to the candiate
        let candidate_bscore = self.headers_store.get_blue_score(candidate_block).unwrap();
        let cutoff_bscore = candidate_bscore - candidate_bscore % self.posterity_depth;

        self.get_chain_block_by_cutoff_bscore(candidate_block, cutoff_bscore, self.posterity_depth)
    }

    // Returns the first chain block with bscore larger or equal to the cutoff bscore;
    // Assumes data is available, will panic if not;
    // Reference_block is assumed to be a chain block;
    // Strongly assumes the required chain block is no more than max_distance away from refernce,
    // and that the tip has higher bscore than the cutoff - the caller is responsible for this guarantee.
    fn get_chain_block_by_cutoff_bscore(&self, reference_block: Hash, cutoff_bscore: u64, max_distance: u64) -> Hash {
        if cutoff_bscore == 0
        // Edge case
        {
            return self.genesis.hash;
        }
        let reference_header = self.headers_store.get_header(reference_block).unwrap();
        let reference_index = self.selected_chain_store.read().get_by_hash(reference_block).unwrap();

        let pruning_read = self.pruning_point_store.read(); // Should I keep this lock till the end?
        let pruning_point_index = self.selected_chain_store.read().get_by_hash(pruning_read.pruning_point().unwrap()).unwrap();
        drop(pruning_read);

        let low = std::cmp::max(reference_index.saturating_sub(max_distance), pruning_point_index);
        let high = min(self.selected_chain_store.read().get_tip().unwrap().0, reference_index + max_distance);
        let estimated_width = self.estimate_dag_width(None); // Rough initial estimation

        assert!(reference_index <= high);

        self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, reference_header, estimated_width)
    }

    // A binary search 'style' recursive function
    // with special checks in place to avoid getting stuck in a back and forth
    // notice the function gets a header and not a hash
    // candidate_header is assumed to be a chain block
    fn get_chain_block_by_cutoff_bscore_rec(
        &self,
        cutoff_bscore: u64,
        mut low: u64,
        mut high: u64,
        candidate_header: Arc<Header>,
        estimated_width: u64,
    ) -> Hash {
        let candidate_index = self.selected_chain_store.read().get_by_hash(candidate_header.hash).unwrap();
        let next_candidate_index;
        // Special attention is taken to prevent a 0 value from occuring
        // and causing divide by 0 or lack of progress
        // estimated width is hence guaranteed a non zero value
        let mut index_step = (candidate_header.blue_score.abs_diff(cutoff_bscore)) / estimated_width;
        index_step = if index_step == 0 { 1 } else { index_step };
        if candidate_header.blue_score < cutoff_bscore {
            // In this case index should move forward
            if low < candidate_index
            // Rescale bound and update
            {
                low = candidate_index; // Rescale bound
                next_candidate_index = candidate_index + index_step;
            } else {
                // If candidate slipped outside the already known bounds,
                // we risk making no progress and  getting stuck inside a back and forth loop,
                // so just iterate forwards on route to high_block till cutoff is found.
                return self.get_chain_block_by_cutoff_bscore_linearly_forwards(candidate_header, high, cutoff_bscore);
            }
        } else {
            // In this case index will move backward
            if high > candidate_index {
                let candidate_parent = self.reachability_service.get_chain_parent(candidate_header.hash);
                let candidate_parent_bscore = self.headers_store.get_blue_score(candidate_parent).unwrap();
                if candidate_parent_bscore < cutoff_bscore
                // First, check if candidate actually is the cutoff block
                {
                    return candidate_header.hash;
                } else {
                    // If not, update candidate indices and bounds
                    high = candidate_index; //  rescale bound
                    next_candidate_index = candidate_index.saturating_sub(index_step);

                    // Shouldn't overflow in natural conditions but does in testing
                }
            } else {
                // Again, avoid getting stuck in a back and forth loop
                // by iterating backwards down to low
                return self.get_chain_block_by_cutoff_bscore_linearly_backwards(candidate_header, low, cutoff_bscore);
            }
        }
        let next_candidate_hash = self.selected_chain_store.read().get_by_index(next_candidate_index);
        if next_candidate_hash.is_err() {
            // Forced repetition if index out of bounds.
            return self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, candidate_header, estimated_width);
        }
        let next_candidate_hash = next_candidate_hash.unwrap();
        let next_candidate_header = self.headers_store.get_header(next_candidate_hash).unwrap();

        // Update the estimated width based on the latest result;
        // Notice a 0 value can never occur:
        // A) because index_step!=0, meaning next_candidate_bscore and candidate_bscore are strictly different
        // B) because |next_candidate_bscore-candidate_bscore| is by definition the minimal possible value index_step can get
        // divide by 0 doesn't occur since index_step!=0;
        // Should reconsider whether this logic is even worth calculating iteratively compared to just the initial guess:
        // This is probably a classic premature optimization is the root of all evil.
        // The scenario where I believe this iterative guessing is of use is when an archival node
        // attempts to find a deep posterity block where thhe width may have flactuated a lot
        let next_estimated_width = (candidate_header.blue_score.abs_diff(next_candidate_header.blue_score)) / index_step;
        assert_ne!(next_estimated_width, 0);

        self.get_chain_block_by_cutoff_bscore_rec(cutoff_bscore, low, high, next_candidate_header, next_estimated_width)
    }

    fn get_chain_block_by_cutoff_bscore_linearly_backwards(&self, initial: Arc<Header>, low: u64, cutoff_bscore: u64) -> Hash {
        // Initial has blue score higher than cutoff score

        let low_block = self.selected_chain_store.read().get_by_index(low).unwrap();
        // Iterate back until a block is found with blue score lower than the cutoff
        let candidate_parent = self
            .reachability_service
            .backward_chain_iterator(initial.hash, low_block, true)
            .find(|&block| self.headers_store.get_blue_score(block).unwrap() < cutoff_bscore);

        if let Some(candidate_parent) = candidate_parent {
            // and then return its 'selected' son
            self.reachability_service.get_next_chain_ancestor(initial.hash, candidate_parent)
        } else {
            // If no block is found, then the block must be low_block itself.
            // We cannot explicitely check this as low_block may not have parents stored
            low_block
        }
    }
    fn get_chain_block_by_cutoff_bscore_linearly_forwards(&self, initial: Arc<Header>, high: u64, cutoff_bscore: u64) -> Hash {
        // Initial has blue score less than cutoff score
        let high_block = self.selected_chain_store.read().get_by_index(high).unwrap();
        self
            .reachability_service
            .forward_chain_iterator(initial.hash, high_block, true)
            .skip(1)// We already know initial has a lower score
            .find(|&block| self.headers_store.get_blue_score(block).unwrap() >= cutoff_bscore)
            .unwrap()
    }

    // A rough estimation of the dag width
    // reference_wrapped is assumed to be a block on the selected chain, or None
    // will panic if not
    pub fn estimate_dag_width(&self, reference_wrapped: Option<Hash>) -> u64 {
        let pruning_read: parking_lot::lock_api::RwLockReadGuard<'_, parking_lot::RawRwLock, Y> = self.pruning_point_store.read();
        let pruning_point_index = self.selected_chain_store.read().get_by_hash(pruning_read.pruning_point().unwrap()).unwrap();
        drop(pruning_read);
        let (reference_index, reference) = match reference_wrapped {
            Some(reference) => {
                let reference_index = self.selected_chain_store.read().get_by_hash(reference).unwrap();
                (reference_index, reference)
            }
            None => {
                // If no refernce provided, take the sink as reference
                self.selected_chain_store.read().get_tip().unwrap()
            }
        };
        let reference_bscore = self.headers_store.get_blue_score(reference).unwrap();
        let past_dist_cover = std::cmp::min(100, self.posterity_depth); // Edge case relevant for testing mostly

        let past_bscore = self
            .headers_store
            .get_blue_score(
                self.selected_chain_store
                    .read()
                    .get_by_index(std::cmp::max(reference_index.saturating_sub(past_dist_cover), pruning_point_index))
                    .unwrap(),
            )
            .unwrap();
        std::cmp::max(reference_bscore.saturating_sub(past_bscore) / past_dist_cover, 1)
        // Avoiding a harmful 0 value
    }

    pub fn verify_is_posterity(&self, alleged_posterity: Hash) -> bool {
        if self.block_transactions_store.get(alleged_posterity).is_ok() {
            // Alleged_posterity is above the retention root, confirm directly that it is a post-posterity of its chain parent.
            let alleged_posterity_parent = self.reachability_service.get_chain_parent(alleged_posterity);
            if !self.verify_post_posterity_block(alleged_posterity_parent, alleged_posterity) {
                return false;
            }
        } else {
            // If the alleged_posterity is already below the retention root, its header not being pruned asserts it is a posterity block
            if self.headers_store.get_header(alleged_posterity).is_err() {
                return false;
            }
        }
        true
    }

    // The verification consists of 3 parts:
    // 1) verify the block queried is an ancesstor of the candidate
    // 2)verify the candidate is on the selected chain
    // 3) verify the selected parent of the candidate has blue score score smaller than the posterity designated blue score
    // function hence assumes the selected parent has not been pruned;
    pub fn verify_post_posterity_block(&self, block_hash: Hash, post_posterity_candidate_hash: Hash) -> bool {
        if !self.reachability_service.is_dag_ancestor_of(block_hash, post_posterity_candidate_hash) {
            return false;
        }
        if self.selected_chain_store.read().get_by_hash(post_posterity_candidate_hash).is_err() {
            return false;
        }
        let bscore = self.headers_store.get_blue_score(block_hash).unwrap();

        let cutoff_bscore = bscore - bscore % self.posterity_depth + self.posterity_depth;
        let candidate_sel_parent_hash = self.reachability_service.get_chain_parent(post_posterity_candidate_hash);
        let candidate_sel_parent_bscore = self.headers_store.get_blue_score(candidate_sel_parent_hash).unwrap();
        candidate_sel_parent_bscore <= cutoff_bscore
    }
}

#[derive(Clone, Copy)]
struct AcceptanceEntry {
    tx_id: Hash,
    version: u16,
    merge_idx: u32,
    lane_id: [u8; 20],
}
