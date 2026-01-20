use std::sync::Arc;

use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, BlueWorkType, HashKTypeMap, HashMapCustomHasher, KType, blockhash::BlockHashExtensions,
};
use kaspa_database::prelude::StoreError;
use kaspa_hashes::Hash;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            dagknight::{DagknightKey, DagknightStore, DagknightStoreReader},
            ghostdag::GhostdagData,
            headers::HeaderStoreReader,
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
        },
    },
    processes::{
        difficulty::calc_work,
        ghostdag::{
            mergeset::unordered_mergeset_without_selected_parent,
            ordering::SortableBlock,
            protocol::{ChainBlock, ColoringOutput, ColoringState},
        },
        reachability::relations::FutureIntersectRelations,
    },
};

// START Copied from GD Manager
// NOTE: This is a copy from GD Manager right now, but the idea here is that it will update k_colouring to
// be more in line with what the paper needs
// Renamed from ghostdag_customized to k_colouring
pub struct ConflictZoneManager<
    S: DagknightStore + DagknightStoreReader,
    Q: RelationsStoreReader,
    R: ReachabilityStoreReader + Clone,
    T: HeaderStoreReader,
> {
    k: KType,
    root: Hash,
    dagknight_store: Arc<S>,
    relations_store: FutureIntersectRelations<Q, MTReachabilityService<R>>,
    reachability_service: MTReachabilityService<R>,
    headers_store: Arc<T>,
}

impl<S: DagknightStore + DagknightStoreReader, Q: RelationsStoreReader, R: ReachabilityStoreReader + Clone, T: HeaderStoreReader>
    ConflictZoneManager<S, Q, R, T>
{
    pub fn new(
        k: KType,
        root: Hash,
        dagknight_store: Arc<S>,
        relations_store: FutureIntersectRelations<Q, MTReachabilityService<R>>,
        reachability_service: MTReachabilityService<R>,
        headers_store: Arc<T>,
    ) -> Self {
        Self { k, root, dagknight_store, headers_store, reachability_service, relations_store }
    }

    pub fn has(&self, pov_hash: Hash) -> bool {
        let key = self.get_key(pov_hash);

        self.dagknight_store.has(key).unwrap()
    }

    pub fn insert(&self, pov_hash: Hash, gd: Arc<GhostdagData>) -> Result<(), StoreError> {
        let key = self.get_key(pov_hash);

        self.dagknight_store.insert(key, gd)
    }

    fn get_key(&self, pov_hash: Hash) -> DagknightKey {
        DagknightKey::new(self.root, pov_hash, self.k, false)
    }

    pub fn get_blue_score(&self, pov_hash: Hash) -> Result<u64, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blue_score)
    }

    pub fn get_blue_work(&self, pov_hash: Hash) -> Result<BlueWorkType, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blue_work)
    }

    pub fn get_selected_parent(&self, pov_hash: Hash) -> Result<Hash, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.selected_parent)
    }

    pub fn get_blues_anticone_sizes(&self, pov_hash: Hash) -> Result<Arc<BlockHashMap<KType>>, StoreError> {
        let key = self.get_key(pov_hash);

        Ok(self.dagknight_store.get_data(key)?.blues_anticone_sizes.clone())
    }

    pub fn get_data(&self, pov_hash: Hash) -> Result<Arc<GhostdagData>, StoreError> {
        let key = self.get_key(pov_hash);

        self.dagknight_store.get_data(key)
    }

    pub fn k_colouring(&self, parents: &[Hash], k: KType, custom_selected_parent: Option<Hash>) -> GhostdagData {
        assert!(!parents.is_empty(), "genesis must be added via a call to init");

        // Run the GHOSTDAG parent selection algorithm
        let selected_parent = custom_selected_parent.unwrap_or(self.find_selected_parent(parents.iter().copied()));
        // Handle the special case of origin children first
        if selected_parent.is_origin() {
            // ORIGIN is always a single parent so both blue score and work should remain zero
            return GhostdagData::new_with_selected_parent(selected_parent, 1); // k is only a capacity hint here
        }
        // Initialize new GHOSTDAG block data with the selected parent
        let mut new_block_data = GhostdagData::new_with_selected_parent(selected_parent, k);
        // Get the mergeset in consensus-agreed topological order (topological here means forward in time from blocks to children)
        let ordered_mergeset = self.ordered_mergeset_without_selected_parent(selected_parent, parents);

        for blue_candidate in ordered_mergeset.iter().cloned() {
            let coloring = self.check_blue_candidate(&new_block_data, blue_candidate, k);

            if let ColoringOutput::Blue(blue_anticone_size, blues_anticone_sizes) = coloring {
                // No k-cluster violation found, we can now set the candidate block as blue
                new_block_data.add_blue(blue_candidate, blue_anticone_size, &blues_anticone_sizes);
            } else {
                new_block_data.add_red(blue_candidate);
            }
        }

        let blue_score = self.get_blue_score(selected_parent).unwrap() + new_block_data.mergeset_blues.len() as u64;

        let added_blue_work: BlueWorkType =
            new_block_data.mergeset_blues.iter().cloned().map(|hash| calc_work(self.headers_store.get_bits(hash).unwrap())).sum();
        let blue_work: BlueWorkType = self.get_blue_work(selected_parent).unwrap() + added_blue_work;

        new_block_data.finalize_score_and_work(blue_score, blue_work);

        new_block_data
    }

    fn check_blue_candidate_with_chain_block(
        &self,
        new_block_data: &GhostdagData,
        chain_block: &ChainBlock,
        blue_candidate: Hash,
        candidate_blues_anticone_sizes: &mut BlockHashMap<KType>,
        candidate_blue_anticone_size: &mut KType,
        k: KType,
    ) -> ColoringState {
        // If blue_candidate is in the future of chain_block, it means
        // that all remaining blues are in the past of chain_block and thus
        // in the past of blue_candidate. In this case we know for sure that
        // the anticone of blue_candidate will not exceed K, and we can mark
        // it as blue.
        //
        // The new block is always in the future of blue_candidate, so there's
        // no point in checking it.

        // We check if chain_block is not the new block by checking if it has a hash.
        if let Some(hash) = chain_block.hash
            && self.reachability_service.is_dag_ancestor_of(hash, blue_candidate)
        {
            return ColoringState::Blue;
        }

        // Iterate over blue peers and check for k-cluster violations
        for &peer in chain_block.data.mergeset_blues.iter() {
            // Skip blocks that are in the past of blue_candidate (since they are not in its anticone)
            if self.reachability_service.is_dag_ancestor_of(peer, blue_candidate) {
                continue;
            }

            // Otherwise, peer must be in the anticone of blue_candidate, so we check for k limits.
            // Note that peer cannot be in the future of blue_candidate because we process the mergeset
            // in past-to-future topological order, so even if chain_block == new_block, an existing blue
            // cannot be in the future of a candidate blue

            let peer_blue_anticone_size = self.blue_anticone_size(peer, new_block_data);
            candidate_blues_anticone_sizes.insert(peer, peer_blue_anticone_size);

            *candidate_blue_anticone_size += 1;
            if *candidate_blue_anticone_size > k {
                // k-cluster violation: The candidate's blue anticone exceeded k
                return ColoringState::Red;
            }

            if peer_blue_anticone_size == k {
                // k-cluster violation: A block in candidate's blue anticone already
                // has k blue blocks in its own anticone
                return ColoringState::Red;
            }

            // This is a sanity check that validates that a blue
            // block's blue anticone is not already larger than K.
            assert!(peer_blue_anticone_size <= k, "found blue anticone larger than K");
            // [Crescendo]: this ^ is a valid assert since we are increasing k. Had we decreased k, this line would
            //              need to be removed and the condition above would need to be changed to >= k
        }

        ColoringState::Pending
    }

    /// Returns the blue anticone size of `block` from the worldview of `context`.
    /// Expects `block` to be in the blue set of `context`
    fn blue_anticone_size(&self, block: Hash, context: &GhostdagData) -> KType {
        let mut current_blues_anticone_sizes = HashKTypeMap::clone(&context.blues_anticone_sizes);
        let mut current_selected_parent = context.selected_parent;
        loop {
            if let Some(size) = current_blues_anticone_sizes.get(&block) {
                return *size;
            }

            // if current_selected_parent == self.genesis_hash || current_selected_parent == blockhash::ORIGIN {
            //     panic!("block {block} is not in blue set of the given context");
            // }

            current_blues_anticone_sizes = self.get_blues_anticone_sizes(current_selected_parent).unwrap();
            current_selected_parent = self.get_selected_parent(current_selected_parent).unwrap();
        }
    }

    fn check_blue_candidate(&self, new_block_data: &GhostdagData, blue_candidate: Hash, k: KType) -> ColoringOutput {
        // The maximum length of new_block_data.mergeset_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.mergeset_blues.len() as KType == k + 1 {
            return ColoringOutput::Red;
        }

        let mut candidate_blues_anticone_sizes: BlockHashMap<KType> = BlockHashMap::with_capacity(k as usize);
        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlock { hash: None, data: new_block_data.into() };
        let mut candidate_blue_anticone_size: KType = 0;

        loop {
            let state = self.check_blue_candidate_with_chain_block(
                new_block_data,
                &chain_block,
                blue_candidate,
                &mut candidate_blues_anticone_sizes,
                &mut candidate_blue_anticone_size,
                k,
            );

            match state {
                ColoringState::Blue => return ColoringOutput::Blue(candidate_blue_anticone_size, candidate_blues_anticone_sizes),
                ColoringState::Red => return ColoringOutput::Red,
                ColoringState::Pending => (), // continue looping
            }

            chain_block = ChainBlock {
                hash: Some(chain_block.data.selected_parent),
                data: self.get_data(chain_block.data.selected_parent).unwrap().into(),
            }
        }
    }

    pub fn sort_blocks(&self, blocks: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = blocks.into_iter().collect();
        sorted_blocks.sort_by_cached_key(|block| SortableBlock { hash: *block, blue_work: self.get_blue_work(*block).unwrap() });
        sorted_blocks
    }

    pub fn ordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &[Hash]) -> Vec<Hash> {
        self.sort_blocks(self.unordered_mergeset_without_selected_parent(selected_parent, parents))
    }

    pub fn unordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &[Hash]) -> BlockHashSet {
        unordered_mergeset_without_selected_parent(&self.relations_store, &self.reachability_service, selected_parent, parents)
    }

    pub fn find_selected_parent(&self, parents: impl IntoIterator<Item = Hash>) -> Hash {
        parents
            .into_iter()
            .map(|parent| SortableBlock { hash: parent, blue_work: self.get_blue_work(parent).unwrap() })
            .max()
            .unwrap()
            .hash
    }
    // END Copied from GD Manager
}
