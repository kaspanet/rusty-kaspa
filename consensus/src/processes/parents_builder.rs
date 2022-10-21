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

    pub fn calc_block_parents(&self, pruning_point: Hash, direct_parents: &[Hash]) -> Vec<Vec<Hash>> {
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
                    // point anticone should be a parent (in the relevant level) of one of
                    // the virtual genesis children in the pruning point anticone. So we can check which
                    // virtual genesis children have this block as parent and use those block as
                    // reference blocks.
                    let reference_blocks = if is_in_future_origin_children {
                        vec![parent]
                    } else {
                        let mut reference_blocks = Vec::with_capacity(origin_children.len());
                        for child_header in origin_children_headers.iter() {
                            if self.parents_at_level(child_header, block_level as u8).contains(&parent) {
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

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        sync::Arc,
    };

    use crate::{
        model::{
            services::reachability::MTReachabilityService,
            stores::{
                errors::StoreError,
                headers::{HeaderStoreReader, HeaderWithBlockLevel},
                reachability::MemoryReachabilityStore,
                relations::RelationsStoreReader,
            },
        },
        processes::reachability::tests::{DagBlock, DagBuilder},
    };

    use super::ParentsManager;
    use consensus_core::{
        blockhash::{BlockHashes, ORIGIN},
        header::Header,
    };
    use hashes::Hash;
    use itertools::Itertools;
    use parking_lot::RwLock;

    struct HeaderStoreMock {
        map: RwLock<HashMap<Hash, Arc<HeaderWithBlockLevel>>>,
    }

    impl HeaderStoreMock {
        fn new() -> Self {
            Self { map: RwLock::new(HashMap::new()) }
        }
    }

    impl HeaderStoreReader for HeaderStoreMock {
        fn get_daa_score(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
            todo!()
        }

        fn get_timestamp(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
            todo!()
        }

        fn get_bits(&self, hash: hashes::Hash) -> Result<u32, StoreError> {
            todo!()
        }

        fn get_header(&self, hash: hashes::Hash) -> Result<Arc<Header>, StoreError> {
            Ok(self.map.read().get(&hash).unwrap().header.clone())
        }

        fn get_compact_header_data(&self, hash: hashes::Hash) -> Result<crate::model::stores::headers::CompactHeaderData, StoreError> {
            todo!()
        }

        fn get_blue_score(&self, hash: hashes::Hash) -> Result<u64, StoreError> {
            todo!()
        }

        fn get_header_with_block_level(&self, hash: hashes::Hash) -> Result<Arc<HeaderWithBlockLevel>, StoreError> {
            Ok(self.map.read().get(&hash).unwrap().clone())
        }
    }

    struct RelationsStoreMock {
        pub children: BlockHashes,
    }

    impl RelationsStoreReader for RelationsStoreMock {
        fn get_parents(&self, hash: Hash) -> Result<consensus_core::blockhash::BlockHashes, StoreError> {
            todo!()
        }

        fn get_children(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
            Ok(self.children.clone())
        }

        fn has(&self, hash: Hash) -> Result<bool, StoreError> {
            todo!()
        }
    }

    #[test]
    fn test_calc_block_parents() {
        let mut reachability_store = MemoryReachabilityStore::new();
        let headers_store = Arc::new(HeaderStoreMock::new());

        let genesis_hash = 3000.into();
        let pruning_point: Hash = 1.into();
        headers_store.map.write().insert(
            pruning_point,
            Arc::new(HeaderWithBlockLevel {
                header: Arc::new(Header {
                    hash: pruning_point,
                    version: 0,
                    parents_by_level: vec![
                        vec![1001.into()],
                        vec![1001.into()],
                        vec![1001.into()],
                        vec![1001.into()],
                        vec![1002.into()],
                    ],
                    hash_merkle_root: 1.into(),
                    accepted_id_merkle_root: 1.into(),
                    utxo_commitment: 1.into(),
                    timestamp: 0,
                    bits: 0,
                    nonce: 0,
                    daa_score: 0,
                    blue_work: 0.into(),
                    blue_score: 0,
                    pruning_point: 1.into(),
                }),
                block_level: 0,
            }),
        );

        let pp_anticone_block: Hash = 3001.into();
        headers_store.map.write().insert(
            pp_anticone_block,
            Arc::new(HeaderWithBlockLevel {
                header: Arc::new(Header {
                    hash: pp_anticone_block,
                    version: 0,
                    parents_by_level: vec![
                        vec![2001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                    ],
                    hash_merkle_root: 1.into(),
                    accepted_id_merkle_root: 1.into(),
                    utxo_commitment: 1.into(),
                    timestamp: 0,
                    bits: 0,
                    nonce: 0,
                    daa_score: 0,
                    blue_work: 0.into(),
                    blue_score: 0,
                    pruning_point: 1.into(),
                }),
                block_level: 0,
            }),
        );

        let pp_anticone_block_child: Hash = 3002.into();
        headers_store.map.write().insert(
            pp_anticone_block_child,
            Arc::new(HeaderWithBlockLevel {
                header: Arc::new(Header {
                    hash: pp_anticone_block_child,
                    version: 0,
                    parents_by_level: vec![
                        vec![3001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                        vec![2001.into()],
                    ],
                    hash_merkle_root: 1.into(),
                    accepted_id_merkle_root: 1.into(),
                    utxo_commitment: 1.into(),
                    timestamp: 0,
                    bits: 0,
                    nonce: 0,
                    daa_score: 0,
                    blue_work: 0.into(),
                    blue_score: 0,
                    pruning_point: 1.into(),
                }),
                block_level: 0,
            }),
        );
        struct TestBlock {
            id: u64,
            block_level: u8,
            direct_parents: Vec<u64>,
            expected_parents: Vec<Vec<u64>>,
        }

        let test_blocks = vec![
            TestBlock {
                id: 2,
                block_level: 0,
                direct_parents: vec![1],
                expected_parents: vec![vec![1], vec![1001], vec![1001], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 3,
                block_level: 1,
                direct_parents: vec![1],
                expected_parents: vec![vec![1], vec![1001], vec![1001], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 4,
                block_level: 0,
                direct_parents: vec![2, 3],
                expected_parents: vec![vec![2, 3], vec![3], vec![1001], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 5,
                block_level: 2,
                direct_parents: vec![4],
                expected_parents: vec![vec![4], vec![3], vec![1001], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 6,
                block_level: 2,
                direct_parents: vec![4],
                expected_parents: vec![vec![4], vec![3], vec![1001], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 7,
                block_level: 0,
                direct_parents: vec![5, 6],
                expected_parents: vec![vec![5, 6], vec![5, 6], vec![5, 6], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 8,
                block_level: 3,
                direct_parents: vec![5],
                expected_parents: vec![vec![5], vec![5], vec![5], vec![1001], vec![1002]],
            },
            TestBlock {
                id: 9,
                block_level: 0,
                direct_parents: vec![7, 8],
                expected_parents: vec![vec![7, 8], vec![6, 8], vec![6, 8], vec![8], vec![1002]],
            },
            TestBlock {
                id: 10,
                block_level: 0,
                direct_parents: vec![3001, 1],
                expected_parents: vec![vec![3001, 1], vec![1001], vec![1001], vec![1001], vec![1002]], // Check that it functions well while one of the parents is in PP anticone
            },
            TestBlock {
                id: 11,
                block_level: 0,
                direct_parents: vec![3002, 1],
                expected_parents: vec![vec![3002, 1], vec![1001], vec![1001], vec![1001], vec![1002]], // Check that it functions well while one of the parents is in PP anticone
            },
        ];

        let mut dag_builder = DagBuilder::new(&mut reachability_store);
        dag_builder
            .init()
            .add_block(DagBlock::new(pruning_point, vec![ORIGIN]))
            .add_block(DagBlock::new(pp_anticone_block, vec![ORIGIN]))
            .add_block(DagBlock::new(pp_anticone_block_child, vec![pp_anticone_block]));

        for test_block in test_blocks.iter() {
            let hash = test_block.id.into();
            let direct_parents = test_block.direct_parents.iter().map(|parent| Hash::from_u64_word(*parent)).collect_vec();
            let expected_parents: Vec<Vec<Hash>> = test_block
                .expected_parents
                .iter()
                .map(|parents| parents.iter().map(|parent| Hash::from_u64_word(*parent)).collect_vec())
                .collect_vec();
            dag_builder.add_block(DagBlock::new(hash, direct_parents));

            headers_store.map.write().insert(
                hash,
                Arc::new(HeaderWithBlockLevel {
                    header: Arc::new(Header {
                        hash,
                        version: 0,
                        parents_by_level: expected_parents,
                        hash_merkle_root: 1.into(),
                        accepted_id_merkle_root: 1.into(),
                        utxo_commitment: 1.into(),
                        timestamp: 0,
                        bits: 0,
                        nonce: 0,
                        daa_score: 0,
                        blue_work: 0.into(),
                        blue_score: 0,
                        pruning_point: 1.into(),
                    }),
                    block_level: test_block.block_level,
                }),
            );
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability_store)));
        let relations_store =
            Arc::new(RwLock::new(RelationsStoreMock { children: BlockHashes::new(vec![pruning_point, pp_anticone_block]) }));
        let parents_manager = ParentsManager::new(250, genesis_hash, headers_store, reachability_service, relations_store);

        for test_block in test_blocks {
            let direct_parents = test_block.direct_parents.iter().map(|parent| Hash::from_u64_word(*parent)).collect_vec();
            let parents = parents_manager.calc_block_parents(pruning_point, &direct_parents[..]);
            let parents_as_u64 = parents
                .iter()
                .map(|parents| HashSet::<u64>::from_iter(parents.iter().map(|parent| hash_to_u64(*parent))))
                .collect_vec();
            let expected_parents = test_block.expected_parents.iter().cloned().map(HashSet::from_iter).collect_vec();
            assert_eq!(expected_parents, parents_as_u64, "failed for block {}", test_block.id);
        }
    }

    fn hash_to_u64(hash: Hash) -> u64 {
        hash.to_le_u64()[0]
    }
}
