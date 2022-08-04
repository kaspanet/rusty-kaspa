use crate::model::{
    api::hash::{Hash, HashArray},
    stores::{
        ghostdag::{GhostdagData, GhostdagStore, HashU8Map},
        reachability::ReachabilityStore,
        relations::RelationsStore,
    },
    ORIGIN,
};
use crate::processes::reachability::inquirer::{self, is_dag_ancestor_of};
use core::marker::PhantomData;
use misc::uint256::Uint256;
use std::{collections::HashMap, sync::Arc};

use super::ordering::*;

pub trait StoreAccess<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore> {
    fn ghostdag_store(&self) -> &T;
    fn ghostdag_store_as_mut(&mut self) -> &mut T;
    fn relations_store(&self) -> &S;
    fn reachability_store(&self) -> &U;
    fn reachability_store_as_mut(&mut self) -> &mut U;
}

pub struct GhostdagManager<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore, V: StoreAccess<T, S, U>> {
    genesis_hash: Hash,
    pub(super) k: u8,
    _phantom: PhantomData<(T, S, U, V)>,
}

impl<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore, V: StoreAccess<T, S, U>> GhostdagManager<T, S, U, V> {
    pub fn new(genesis_hash: Hash, k: u8) -> Self {
        Self { genesis_hash, k, _phantom: Default::default() }
    }

    pub fn init(&self, sa: &mut V) {
        sa.ghostdag_store_as_mut()
            .insert(
                self.genesis_hash,
                Arc::new(GhostdagData::new(
                    0,
                    Uint256::from_u64(0),
                    ORIGIN,
                    HashArray::new(Vec::new()),
                    HashArray::new(Vec::new()),
                    HashU8Map::new(HashMap::new()),
                )),
            )
            .unwrap();
    }

    pub fn add_block(&self, sa: &mut V, block: Hash) {
        let parents = sa.relations_store().get_parents(block).unwrap();
        assert!(parents.len() > 0, "genesis must be added via a call to init");

        let selected_parent = Self::find_selected_parent(sa, &parents);
        let mut new_block_data = Arc::new(GhostdagData::with_selected_parent(selected_parent, self.k));

        let mergeset = self.mergeset_without_selected_parent(sa, &selected_parent, &parents);

        for blue_candidate in mergeset.iter().cloned() {
            let (is_blue, candidate_blue_anticone_size, candidate_blues_anticone_sizes) =
                self.check_blue_candidate(sa, &new_block_data, blue_candidate);

            if is_blue {
                // No k-cluster violation found, we can now set the candidate block as blue
                let new_block_data_mut = Arc::make_mut(&mut new_block_data);
                HashArray::make_mut(&mut new_block_data_mut.mergeset_blues).push(blue_candidate);
                HashU8Map::make_mut(&mut new_block_data_mut.blues_anticone_sizes)
                    .insert(blue_candidate, candidate_blue_anticone_size);
                for (blue, size) in candidate_blues_anticone_sizes {
                    HashU8Map::make_mut(&mut new_block_data_mut.blues_anticone_sizes).insert(blue, size + 1);
                }
            } else {
                let new_block_data_mut = Arc::make_mut(&mut new_block_data);
                HashArray::make_mut(&mut new_block_data_mut.mergeset_reds).push(blue_candidate);
            }
        }

        let blue_score = sa
            .ghostdag_store()
            .get_blue_score(selected_parent, false)
            .unwrap()
            + new_block_data.mergeset_blues.len() as u64;

        let new_block_data_mut = Arc::make_mut(&mut new_block_data);
        new_block_data_mut.blue_score = blue_score;

        // TODO: This is just a placeholder until calc_work is implemented.
        new_block_data_mut.blue_work = Uint256::from_u64(blue_score);

        sa.ghostdag_store_as_mut()
            .insert(block, new_block_data)
            .unwrap();

        // TODO: Reachability should be changed somewhere else
        inquirer::add_block(sa.reachability_store_as_mut(), block, selected_parent, &mut mergeset.iter().cloned())
            .unwrap();
    }

    fn check_blue_candidate_with_chain_block(
        &self, sa: &V, new_block_data: &GhostdagData, chain_block: &ChainBlockData, blue_candidate: Hash,
        candidate_blues_anticone_sizes: &mut HashMap<Hash, u8>, candidate_blue_anticone_size: &mut u8,
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
            if is_dag_ancestor_of(sa.reachability_store(), hash, blue_candidate).unwrap() {
                return (true, false);
            }
        }

        for block in chain_block.data.mergeset_blues.iter().cloned() {
            // Skip blocks that exist in the past of blue_candidate.
            if is_dag_ancestor_of(sa.reachability_store(), block, blue_candidate).unwrap() {
                continue;
            }

            candidate_blues_anticone_sizes.insert(block, self.blue_anticone_size(sa, block, new_block_data));

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
            if *candidate_blues_anticone_sizes
                .get(&block)
                .unwrap()
                > self.k
            {
                panic!("found blue anticone larger than K");
            }
        }

        (false, false)
    }

    // blue_anticone_size returns the blue anticone size of 'block' from the worldview of 'context'.
    // Expects 'block' to be in the blue set of 'context'
    fn blue_anticone_size(&self, sa: &V, block: Hash, context: &GhostdagData) -> u8 {
        let mut is_trusted_data = false;
        let mut current_blues_anticone_sizes = HashU8Map::clone(&context.blues_anticone_sizes);
        let mut current_selected_parent = context.selected_parent;
        loop {
            if let Some(size) = current_blues_anticone_sizes.get(&block) {
                return *size;
            }

            if current_selected_parent == self.genesis_hash {
                panic!("block {} is not in blue set of the given context", block);
            }

            current_blues_anticone_sizes = sa
                .ghostdag_store()
                .get_blues_anticone_sizes(current_selected_parent, is_trusted_data)
                .unwrap();

            current_selected_parent = sa
                .ghostdag_store()
                .get_selected_parent(current_selected_parent, is_trusted_data)
                .unwrap();
            if current_selected_parent == ORIGIN {
                is_trusted_data = true;
                current_blues_anticone_sizes = sa
                    .ghostdag_store()
                    .get_blues_anticone_sizes(current_selected_parent, is_trusted_data)
                    .unwrap();

                current_selected_parent = sa
                    .ghostdag_store()
                    .get_selected_parent(current_selected_parent, is_trusted_data)
                    .unwrap();
            }
        }
    }

    fn check_blue_candidate(
        &self, sa: &V, new_block_data: &Arc<GhostdagData>, blue_candidate: Hash,
    ) -> (bool, u8, HashMap<Hash, u8>) {
        // The maximum length of new_block_data.mergeset_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.mergeset_blues.len() as u8 == self.k + 1 {
            return (false, 0, HashMap::new());
        }

        let mut candidate_blues_anticone_sizes: HashMap<Hash, u8> = HashMap::with_capacity(self.k as usize);

        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlockData { hash: None, data: Arc::clone(new_block_data) };

        let mut candidate_blue_anticone_size: u8 = 0;

        loop {
            let (is_blue, is_red) = self.check_blue_candidate_with_chain_block(
                sa,
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
                data: sa
                    .ghostdag_store()
                    .get_data(chain_block.data.selected_parent, false)
                    .unwrap(),
            }
        }

        (true, candidate_blue_anticone_size, candidate_blues_anticone_sizes)
    }

    fn find_selected_parent(sa: &V, parents: &HashArray) -> Hash {
        parents
            .iter()
            .map(|parent| SortableBlock {
                hash: *parent,
                blue_work: sa
                    .ghostdag_store()
                    .get_blue_work(*parent, false)
                    .unwrap(),
            })
            .max()
            .unwrap()
            .hash
    }
}
struct ChainBlockData {
    hash: Option<Hash>,
    data: Arc<GhostdagData>,
}
