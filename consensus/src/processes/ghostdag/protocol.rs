use crate::processes::reachability::inquirer::{self, is_dag_ancestor_of};
use crate::{
    misc::uint256::Uint256,
    model::{
        api::hash::{Hash, HashArray},
        stores::{ghostdag::GhostdagStore, reachability::ReachabilityStore, relations::RelationsStore},
        ORIGIN,
    },
};

use std::{collections::HashMap, rc::Rc};

use super::ordering::*;

pub trait StoreAccess<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore> {
    fn ghostdag_store(&self) -> &T;
    fn ghostdag_store_as_mut(&mut self) -> &mut T;
    fn relations_store(&self) -> &S;
    fn reachability_store(&self) -> &U;
    fn reachability_store_as_mut(&mut self) -> &mut U;
}

#[derive(Clone)]
struct BlockData {
    blue_score: u64,
    blue_work: Uint256,
    selected_parent: Hash,
    merge_set_blues: HashArray,
    blues_anticone_sizes: Rc<HashMap<Hash, u8>>,
}

pub struct GhostdagManager {
    pub genesis_hash: Hash,
    pub k: u8,
}

impl GhostdagManager {
    pub fn add_block<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore>(
        &self, sa: &mut impl StoreAccess<T, S, U>, block: Hash,
    ) {
        let parents = sa.relations_store().get_parents(&block).unwrap();
        let is_genesis = parents.len() == 0;
        if is_genesis {
            sa.ghostdag_store_as_mut()
                .set_blue_score(block, 0)
                .unwrap();
            sa.ghostdag_store_as_mut()
                .set_blue_work(block, Uint256::new([0; 4]))
                .unwrap();
            sa.ghostdag_store_as_mut()
                .set_merge_set_blues(block, HashArray::new(Vec::new()))
                .unwrap();
            sa.ghostdag_store_as_mut()
                .set_merge_set_reds(block, HashArray::new(Vec::new()))
                .unwrap();
            sa.ghostdag_store_as_mut()
                .set_blues_anticone_sizes(block, Rc::new(HashMap::new()))
                .unwrap();
        }

        let selected_parent = find_selected_parent(sa, &parents);
        let mut merge_set_blues = Vec::with_capacity((self.k + 1) as usize);
        merge_set_blues.push(selected_parent);

        let mut blues_anticone_sizes: HashMap<Hash, u8> = HashMap::with_capacity(self.k as usize);
        blues_anticone_sizes.insert(selected_parent, 0);
        let merge_set = self.merge_set_without_selected_parent(sa, &selected_parent, &parents);

        let mut new_block_data = Rc::new(BlockData {
            blue_score: 0,
            blue_work: Default::default(),
            selected_parent,
            merge_set_blues: Rc::new(merge_set_blues),
            blues_anticone_sizes: Rc::new(blues_anticone_sizes),
        });

        let mut mergeset_reds: Vec<Hash> = Vec::new();
        for blue_candidate in merge_set.iter().cloned() {
            let (is_blue, candidate_blue_anticone_size, candidate_blues_anticone_sizes) =
                self.check_blue_candidate(sa, Rc::clone(&new_block_data), blue_candidate);

            if is_blue {
                // No k-cluster violation found, we can now set the candidate block as blue
                let new_block_data_mut = Rc::make_mut(&mut new_block_data);
                Rc::make_mut(&mut new_block_data_mut.merge_set_blues).push(blue_candidate);
                Rc::make_mut(&mut new_block_data_mut.blues_anticone_sizes)
                    .insert(blue_candidate, candidate_blue_anticone_size);
                for (blue, size) in candidate_blues_anticone_sizes {
                    Rc::make_mut(&mut new_block_data_mut.blues_anticone_sizes).insert(blue, size + 1);
                }
            } else {
                mergeset_reds.push(blue_candidate);
            }
        }

        let blue_score = sa
            .ghostdag_store()
            .get_blue_score(selected_parent, false)
            .unwrap()
            + new_block_data.merge_set_blues.len() as u64;

        // TODO: This is just a placeholder until calc_work is implemented.
        let blue_work = Uint256::from_u64(blue_score);

        sa.ghostdag_store_as_mut()
            .set_blue_score(block, blue_score)
            .unwrap();

        sa.ghostdag_store_as_mut()
            .set_blue_work(block, blue_work)
            .unwrap();

        sa.ghostdag_store_as_mut()
            .set_selected_parent(block, new_block_data.selected_parent)
            .unwrap();

        sa.ghostdag_store_as_mut()
            .set_merge_set_blues(block, Rc::clone(&new_block_data.merge_set_blues))
            .unwrap();

        sa.ghostdag_store_as_mut()
            .set_merge_set_reds(block, Rc::new(mergeset_reds))
            .unwrap();

        sa.ghostdag_store_as_mut()
            .set_blues_anticone_sizes(block, Rc::clone(&new_block_data.blues_anticone_sizes))
            .unwrap();

        inquirer::add_block(sa.reachability_store_as_mut(), block, selected_parent, &mut merge_set.iter().cloned())
            .unwrap();
    }

    fn check_blue_candidate_with_chain_block<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore>(
        &self, sa: &impl StoreAccess<T, S, U>, new_block_data: &BlockData, chain_block: &ChainBlockData,
        blue_candidate: Hash, candidate_blues_anticone_sizes: &mut HashMap<Hash, u8>,
        candidate_blue_anticone_size: &mut u8,
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

        for block in chain_block.data.merge_set_blues.iter().cloned() {
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
    fn blue_anticone_size<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore>(
        &self, sa: &impl StoreAccess<T, S, U>, block: Hash, context: &BlockData,
    ) -> u8 {
        let mut is_trusted_data = false;
        let mut current_blues_anticone_sizes = Rc::clone(&context.blues_anticone_sizes);
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

    fn check_blue_candidate<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore>(
        &self, sa: &impl StoreAccess<T, S, U>, new_block_data: Rc<BlockData>, blue_candidate: Hash,
    ) -> (bool, u8, HashMap<Hash, u8>) {
        // The maximum length of new_block_data.merge_set_blues can be K+1 because
        // it contains the selected parent.
        if new_block_data.merge_set_blues.len() as u8 == self.k + 1 {
            return (false, 0, HashMap::new());
        }

        let mut candidate_blues_anticone_sizes: HashMap<Hash, u8> = HashMap::with_capacity(self.k as usize);

        // Iterate over all blocks in the blue past of the new block that are not in the past
        // of blue_candidate, and check for each one of them if blue_candidate potentially
        // enlarges their blue anticone to be over K, or that they enlarge the blue anticone
        // of blue_candidate to be over K.
        let mut chain_block = ChainBlockData { hash: None, data: Rc::clone(&new_block_data) };

        let mut candidate_blue_anticone_size: u8 = 0;

        loop {
            let (is_blue, is_red) = self.check_blue_candidate_with_chain_block(
                sa,
                &new_block_data,
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

            let selected_parent_blue_score = sa
                .ghostdag_store()
                .get_blue_score(chain_block.data.selected_parent, false)
                .unwrap();
            let selected_parent_blue_work = sa
                .ghostdag_store()
                .get_blue_work(chain_block.data.selected_parent, false)
                .unwrap();
            let selected_parent_selected_parent = sa
                .ghostdag_store()
                .get_selected_parent(chain_block.data.selected_parent, false)
                .unwrap();
            let selected_parent_merge_set_blues = sa
                .ghostdag_store()
                .get_merge_set_blues(chain_block.data.selected_parent, false)
                .unwrap();
            let selected_parent_blues_anticone_sizes = sa
                .ghostdag_store()
                .get_blues_anticone_sizes(chain_block.data.selected_parent, false)
                .unwrap();

            chain_block = ChainBlockData {
                hash: Some(chain_block.data.selected_parent),
                data: Rc::new(BlockData {
                    blue_score: selected_parent_blue_score,
                    blue_work: selected_parent_blue_work,
                    selected_parent: selected_parent_selected_parent,
                    merge_set_blues: selected_parent_merge_set_blues,
                    blues_anticone_sizes: selected_parent_blues_anticone_sizes,
                }),
            }
        }

        (true, candidate_blue_anticone_size, candidate_blues_anticone_sizes)
    }
}

fn find_selected_parent<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore>(
    sa: &impl StoreAccess<T, S, U>, parents: &HashArray,
) -> Hash {
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

struct ChainBlockData {
    hash: Option<Hash>,
    data: Rc<BlockData>,
}
