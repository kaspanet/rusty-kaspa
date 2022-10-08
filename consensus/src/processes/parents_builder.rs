use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use consensus_core::{blockhash::ORIGIN, header::Header};
use hashes::Hash;
use itertools::Itertools;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            errors::StoreError, headers::HeaderStoreReader, reachability::ReachabilityStoreReader, relations::RelationsStoreReader,
        },
    },
    processes::reachability::ReachabilityError,
};

#[derive(Clone)]
pub struct ParentsManager<T: HeaderStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> {
    max_block_level: u8,
    genesis_hash: Hash,

    headers_store: Arc<T>,
    reachability_service: MTReachabilityService<U>,
    relations_store: Arc<RwLock<V>>,
}

impl<T: HeaderStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> ParentsManager<T, U, V> {
    pub fn new(
        max_block_level: u8,
        genesis_hash: Hash,
        headers_store: Arc<T>,
        reachability_service: MTReachabilityService<U>,
        relations_store: Arc<RwLock<V>>,
    ) -> Self {
        Self { max_block_level, genesis_hash, headers_store, reachability_service, relations_store }
    }

    pub fn calc_block_parents(&self, pruning_point: Hash, direct_parents: &Vec<Hash>) -> Vec<Vec<Hash>> {
        let mut direct_parent_headers =
            direct_parents.iter().copied().map(|parent| self.headers_store.get_header_with_block_level(parent).unwrap()).collect_vec();

        // The first candidates to be added should be from a parent in the future of the pruning
        // point, so later on we'll know that every block that doesn't have reachability data
        // (i.e. pruned) is necessarily in the past of the current candidates and cannot be
        // considered as a valid candidate.
        // This is why we sort the direct parent headers in a way that the first one will be
        // in the future of the pruning point.
        let first_parent_in_future_of_pruning_point_index = direct_parents
            .iter()
            .copied()
            .position(|parent| self.reachability_service.is_dag_ancestor_of(pruning_point, parent))
            .expect("at least one of the parents is expected to be in the future of the pruning point");
        direct_parent_headers.swap(0, first_parent_in_future_of_pruning_point_index);
        drop(direct_parents); // Since `direct_parents` and `direct_parent_headers` are now sorted differently, we drop direct_parents to avoid mistakes.

        let mut candidates_by_level_to_reference_blocks_map = (0..self.max_block_level + 1).map(|level| HashMap::new()).collect_vec();
        // Direct parents are guaranteed to be in one other's anticones so add them all to
        // all the block levels they occupy.
        for direct_parent_header in direct_parent_headers.iter() {
            for level in 0..direct_parent_header.block_level + 1 {
                candidates_by_level_to_reference_blocks_map[level as usize]
                    .insert(direct_parent_header.header.hash, vec![direct_parent_header.header.hash]);
            }
        }

        let origin_children = self.relations_store.read().get_children(ORIGIN).unwrap();
        let origin_children_headers =
            origin_children.iter().copied().map(|parent| self.headers_store.get_header(parent).unwrap()).collect_vec();

        for direct_parent_header in direct_parent_headers {
            for (block_level, direct_parent_level_parents) in self.parents(&direct_parent_header.header).enumerate() {
                let is_empty_level = candidates_by_level_to_reference_blocks_map[block_level].is_empty();

                for parent in direct_parent_level_parents.iter().copied() {
                    let mut is_in_future_origin_children = false;
                    for child in origin_children.iter().copied() {
                        match self.reachability_service.is_dag_ancestor_of_result(child, parent) {
                            Ok(is_in_future_of_child) => {
                                if is_in_future_of_child {
                                    is_in_future_origin_children = true;
                                    break;
                                }
                            }
                            Err(ReachabilityError::StoreError(e)) => {
                                if let StoreError::KeyNotFound(_) = e {
                                    break;
                                } else {
                                    panic!("Unexpected store error: {:?}", e)
                                }
                            }
                            Err(err) => panic!("Unexpected reachability error: {:?}", err),
                        }
                    }

                    // Reference blocks are the blocks that are used in reachability queries to check if
                    // a candidate is in the future of another candidate. In most cases this is just the
                    // block itself, but in the case where a block doesn't have reachability data we need
                    // to use some blocks in its future as reference instead.
                    // If we make sure to add a parent in the future of the pruning point first, we can
                    // know that any pruned candidate that is in the past of some blocks in the pruning
                    // point anticone should have should be a parent (in the relevant level) of one of
                    // the virtual genesis children in the pruning point anticone. So we can check which
                    // virtual genesis children have this block as parent and use those block as
                    // reference blocks.
                    let reference_blocks = if is_in_future_origin_children {
                        vec![parent]
                    } else {
                        let mut reference_blocks = Vec::with_capacity(origin_children.len());
                        for child_header in origin_children_headers.iter() {
                            if self.parents_at_level(&child_header, block_level as u8).contains(&parent) {
                                reference_blocks.push(child_header.hash);
                            }
                        }
                        reference_blocks
                    };

                    if is_empty_level {
                        candidates_by_level_to_reference_blocks_map[block_level].insert(parent, reference_blocks);
                        continue;
                    }

                    if !is_in_future_origin_children {
                        continue;
                    }

                    let mut to_remove = HashSet::new();
                    for (candidate, candidate_references) in candidates_by_level_to_reference_blocks_map[block_level].iter() {
                        if self.reachability_service.is_any_dag_ancestor(&mut candidate_references.iter().copied(), parent) {
                            to_remove.insert(*candidate);
                            continue;
                        }
                    }

                    for hash in to_remove.iter() {
                        candidates_by_level_to_reference_blocks_map[block_level].remove(hash);
                    }

                    let is_ancestor_of_any_candidate =
                        candidates_by_level_to_reference_blocks_map[block_level].iter().any(|(_, candidate_references)| {
                            self.reachability_service.is_dag_ancestor_of_any(parent, &mut candidate_references.iter().copied())
                        });

                    // We should add the block as a candidate if it's in the future of another candidate
                    // or in the anticone of all candidates.
                    if !is_ancestor_of_any_candidate || !to_remove.is_empty() {
                        candidates_by_level_to_reference_blocks_map[block_level].insert(parent, reference_blocks);
                    }
                }
            }
        }

        let mut parents = Vec::with_capacity(self.max_block_level as usize);
        for (block_level, reference_blocks_map) in candidates_by_level_to_reference_blocks_map.iter().enumerate() {
            if block_level > 0 && reference_blocks_map.contains_key(&self.genesis_hash) && reference_blocks_map.len() == 1 {
                break;
            }

            let level_blocks = reference_blocks_map.keys().copied().collect_vec();
            parents.push(reference_blocks_map.keys().copied().collect_vec());
        }

        parents
    }

    pub fn parents<'a>(&'a self, header: &'a Header) -> impl ExactSizeIterator<Item = &'a [Hash]> {
        (0..self.max_block_level).map(|level| self.parents_at_level(header, level))
    }

    pub fn parents_at_level<'a>(&'a self, header: &'a Header, level: u8) -> &'a [Hash] {
        if header.direct_parents().is_empty() {
            // If is genesis
            &[]
        } else if header.parents_by_level.len() > level as usize {
            &header.parents_by_level[level as usize][..]
        } else {
            std::slice::from_ref(&self.genesis_hash)
        }
    }
}
