use std::{collections::HashMap, sync::Arc};

use misc::uint256::Uint256;

use crate::{
    model::{
        api::hash::{Hash, HashArray},
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{GhostdagData, GhostdagStoreReader, HashKTypeMap, KType},
            relations::RelationsStoreReader,
        },
        ORIGIN,
    },
    pipeline::HeaderProcessingContext,
};

use super::ordering::*;

pub struct GhostdagManager<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService> {
    genesis_hash: Hash,
    pub(super) k: KType,
    pub(super) ghostdag_store: Arc<T>,
    pub(super) relations_store: Arc<S>,
    pub(super) reachability_service: Arc<U>,
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService> GhostdagManager<T, S, U> {
    pub fn new(
        genesis_hash: Hash, k: KType, ghostdag_store: Arc<T>, relations_store: Arc<S>, reachability_service: Arc<U>,
    ) -> Self {
        Self { genesis_hash, k, ghostdag_store, relations_store, reachability_service }
    }

    pub fn init(&self, ctx: &mut HeaderProcessingContext) {
        if !self
            .ghostdag_store
            .has(self.genesis_hash, false)
            .unwrap()
        {
            ctx.cache_mergeset(HashArray::new(Vec::new()));
            ctx.stage_ghostdag_data(Arc::new(GhostdagData::new(
                0,
                Uint256::from_u64(0),
                ORIGIN,
                HashArray::new(Vec::new()),
                HashArray::new(Vec::new()),
                HashKTypeMap::new(HashMap::new()),
            )));
        }
    }

    fn find_selected_parent(&self, parents: &HashArray) -> Hash {
        parents
            .iter()
            .map(|parent| SortableBlock {
                hash: *parent,
                blue_work: self
                    .ghostdag_store
                    .get_blue_work(*parent, false)
                    .unwrap(),
            })
            .max()
            .unwrap()
            .hash
    }

    pub fn add_block(&self, ctx: &mut HeaderProcessingContext, block: Hash) {
        let parents = self.relations_store.get_parents(block).unwrap();
        assert!(parents.len() > 0, "genesis must be added via a call to init");

        // Run the GHOSTDAG parent selection algorithm
        let selected_parent = self.find_selected_parent(&parents);
        // Initialize new GHOSTDAG block data with the selected parent
        let mut new_block_data = Arc::new(GhostdagData::new_with_selected_parent(selected_parent, self.k));
        // Get the mergeset in consensus-agreed topological order (topological here means forward in time from blocks to children)
        let ordered_mergeset = self.ordered_mergeset_without_selected_parent(selected_parent, &parents);

        for blue_candidate in ordered_mergeset.iter().cloned() {
            let (is_blue, candidate_blue_anticone_size, candidate_blues_anticone_sizes) =
                self.check_blue_candidate(&new_block_data, blue_candidate);

            if is_blue {
                // No k-cluster violation found, we can now set the candidate block as blue
                new_block_data.add_blue(blue_candidate, candidate_blue_anticone_size, &candidate_blues_anticone_sizes);
            } else {
                new_block_data.add_red(blue_candidate);
            }
        }

        let blue_score = self
            .ghostdag_store
            .get_blue_score(selected_parent, false)
            .unwrap()
            + new_block_data.mergeset_blues.len() as u64;
        // TODO: This is just a placeholder until calc_work is implemented.
        let blue_work = Uint256::from_u64(blue_score);
        new_block_data.finalize_score_and_work(blue_score, blue_work);

        // Cache mergeset in context
        ctx.cache_mergeset(Arc::new(ordered_mergeset));
        // Stage new block data
        ctx.stage_ghostdag_data(new_block_data);
    }

    fn check_blue_candidate_with_chain_block(
        &self, new_block_data: &GhostdagData, chain_block: &ChainBlockData, blue_candidate: Hash,
        candidate_blues_anticone_sizes: &mut HashMap<Hash, KType>, candidate_blue_anticone_size: &mut KType,
    ) -> (bool, bool) {
        // If blue_candidate is in the future of chain_block, it means
        // that all remaining blues are in the past of chain_block and thus
        // in the past of blue_candidate. In this case we know for sure that
        // the anticone of blue_candidate will not exceed K, and we can mark
        // it as blue.
        //
        // The new block is always in the future of blue_candidate, so there's
        // no point in checking it.

        // We check if chain_block is not the new block by checking if it has a hash.
        if let Some(hash) = chain_block.hash {
            if self
                .reachability_service
                .is_dag_ancestor_of(hash, blue_candidate)
            {
                return (true, false);
            }
        }

        for block in chain_block.data.mergeset_blues.iter().cloned() {
            // Skip blocks that exist in the past of blue_candidate.
            if self
                .reachability_service
                .is_dag_ancestor_of(block, blue_candidate)
            {
                continue;
            }

            candidate_blues_anticone_sizes.insert(block, self.blue_anticone_size(block, new_block_data));

            *candidate_blue_anticone_size += 1;
            if *candidate_blue_anticone_size > self.k {
                // k-cluster violation: The candidate's blue anticone exceeded k
                return (false, true);
            }

            if *candidate_blues_anticone_sizes
                .get(&block)
                .unwrap()
                == self.k
            {
                // k-cluster violation: A block in candidate's blue anticone already
                // has k blue blocks in its own anticone
                return (false, true);
            }

            // This is a sanity check that validates that a blue
            // block's blue anticone is not already larger than K.
            assert!(
                *candidate_blues_anticone_sizes
                    .get(&block)
                    .unwrap()
                    <= self.k,
                "found blue anticone larger than K"
            );
        }

        (false, false)
    }

    /// Returns the blue anticone size of `block` from the worldview of `context`.
    /// Expects `block` to be in the blue set of `context`
    fn blue_anticone_size(&self, block: Hash, context: &GhostdagData) -> KType {
        let mut is_trusted_data = false;
        let mut current_blues_anticone_sizes = HashKTypeMap::clone(&context.blues_anticone_sizes);
        let mut current_selected_parent = context.selected_parent;
        loop {
            if let Some(size) = current_blues_anticone_sizes.get(&block) {
                return *size;
            }

            if current_selected_parent == self.genesis_hash {
                panic!("block {} is not in blue set of the given context", block);
            }

            current_blues_anticone_sizes = self
                .ghostdag_store
                .get_blues_anticone_sizes(current_selected_parent, is_trusted_data)
                .unwrap();

            current_selected_parent = self
                .ghostdag_store
                .get_selected_parent(current_selected_parent, is_trusted_data)
                .unwrap();

            if current_selected_parent == ORIGIN {
                is_trusted_data = true;
                current_blues_anticone_sizes = self
                    .ghostdag_store
                    .get_blues_anticone_sizes(current_selected_parent, is_trusted_data)
                    .unwrap();

                current_selected_parent = self
                    .ghostdag_store
                    .get_selected_parent(current_selected_parent, is_trusted_data)
                    .unwrap();
            }
        }
    }

    fn check_blue_candidate(
        &self, new_block_data: &Arc<GhostdagData>, blue_candidate: Hash,
    ) -> (bool, KType, HashMap<Hash, KType>) {
        // The maximum length of new_block_data.mergeset_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.mergeset_blues.len() as KType == self.k + 1 {
            return (false, 0, HashMap::new());
        }

        let mut candidate_blues_anticone_sizes: HashMap<Hash, KType> = HashMap::with_capacity(self.k as usize);

        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlockData { hash: None, data: Arc::clone(new_block_data) };

        let mut candidate_blue_anticone_size: KType = 0;

        loop {
            let (is_blue, is_red) = self.check_blue_candidate_with_chain_block(
                new_block_data,
                &chain_block,
                blue_candidate,
                &mut candidate_blues_anticone_sizes,
                &mut candidate_blue_anticone_size,
            );

            if is_blue {
                break;
            }

            if is_red {
                return (false, 0, HashMap::new());
            }

            chain_block = ChainBlockData {
                hash: Some(chain_block.data.selected_parent),
                data: self
                    .ghostdag_store
                    .get_data(chain_block.data.selected_parent, false)
                    .unwrap(),
            }
        }

        (true, candidate_blue_anticone_size, candidate_blues_anticone_sizes)
    }
}

struct ChainBlockData {
    hash: Option<Hash>,
    data: Arc<GhostdagData>,
}
