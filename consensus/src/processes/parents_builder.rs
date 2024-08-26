use indexmap::IndexSet;
use itertools::Itertools;
use kaspa_consensus_core::{blockhash::ORIGIN, header::Header, BlockHashMap, BlockHasher, BlockLevel};
use kaspa_hashes::Hash;
use smallvec::{smallvec, SmallVec};
use std::sync::Arc;

use crate::model::{
    services::reachability::{MTReachabilityService, ReachabilityService},
    stores::{headers::HeaderStoreReader, reachability::ReachabilityStoreReader, relations::RelationsStoreReader},
};

#[derive(Clone)]
pub struct ParentsManager<T: HeaderStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> {
    max_block_level: BlockLevel,
    genesis_hash: Hash,

    headers_store: Arc<T>,
    reachability_service: MTReachabilityService<U>,
    relations_service: V,
}

impl<T: HeaderStoreReader, U: ReachabilityStoreReader, V: RelationsStoreReader> ParentsManager<T, U, V> {
    pub fn new(
        max_block_level: BlockLevel,
        genesis_hash: Hash,
        headers_store: Arc<T>,
        reachability_service: MTReachabilityService<U>,
        relations_service: V,
    ) -> Self {
        Self { max_block_level, genesis_hash, headers_store, reachability_service, relations_service }
    }

    /// Calculates the parents for each level based on the direct parents. Expects the current
    /// global pruning point s.t. at least one of the direct parents is in its inclusive future
    pub fn calc_block_parents(&self, current_pruning_point: Hash, direct_parents: &[Hash]) -> Vec<Vec<Hash>> {
        let mut direct_parent_headers =
            direct_parents.iter().copied().map(|parent| self.headers_store.get_header_with_block_level(parent).unwrap()).collect_vec();

        // The first candidates to be added should be from a parent in the future of the pruning
        // point, so later on we'll know that every block that doesn't have reachability data
        // (i.e. pruned) is necessarily in the past of the current candidates and cannot be
        // considered as a valid candidate.
        // This is why we sort the direct parent headers in a way that the first one will be
        // in the future of the pruning point.
        let first_parent_in_future_of_pruning_point = direct_parents
            .iter()
            .copied()
            .position(|parent| self.reachability_service.is_dag_ancestor_of(current_pruning_point, parent))
            .expect("at least one of the parents is expected to be in the future of the pruning point");
        direct_parent_headers.swap(0, first_parent_in_future_of_pruning_point);

        let mut origin_children_headers = None;
        let mut parents = Vec::with_capacity(self.max_block_level as usize);

        for block_level in 0..=self.max_block_level {
            // Direct parents are guaranteed to be in one another's anticones so add them all to
            // all the block levels they occupy.
            let mut level_candidates_to_reference_blocks = direct_parent_headers
                .iter()
                .filter(|h| block_level <= h.block_level)
                .map(|h| (h.header.hash, smallvec![h.header.hash]))
                // We use smallvec with size 1 in order to optimize for the common case
                // where the block itself is the only reference block
                .collect::<BlockHashMap<SmallVec<[Hash; 1]>>>();

            let mut first_parent_marker = 0;
            let grandparents = if level_candidates_to_reference_blocks.is_empty() {
                // This means no direct parents at the level, hence we must give precedence to first parent's parents
                // which should all be added as candidates in the processing loop below (since we verified that first
                // parent was in the pruning point's future)
                let mut grandparents = self.parents_at_level(&direct_parent_headers[0].header, block_level)
                    .iter()
                    .copied()
                    // We use IndexSet in order to preserve iteration order and make sure the 
                    // processing loop visits the parents of the first parent first
                    .collect::<IndexSet<Hash, BlockHasher>>();
                // Mark the end index of first parent's parents
                first_parent_marker = grandparents.len();
                // Add the remaining level-grandparents
                grandparents.extend(
                    direct_parent_headers[1..].iter().flat_map(|h| self.parents_at_level(&h.header, block_level).iter().copied()),
                );
                grandparents
            } else {
                direct_parent_headers
                    .iter()
                    // We need to iterate parent's parents only if parent is not at block_level
                    .filter(|h| block_level > h.block_level)
                    .flat_map(|h| self.parents_at_level(&h.header, block_level).iter().copied())
                    .collect::<IndexSet<Hash, BlockHasher>>()
            };

            let parents_at_level = if level_candidates_to_reference_blocks.is_empty() && first_parent_marker == grandparents.len() {
                // Optimization: this is a common case for high levels where none of the direct parents is on the level
                // and all direct parents have the same level parents. The condition captures this case because all grandparents
                // will be below the first parent marker and there will be no additional grandparents. Bcs all grandparents come
                // from a single, already validated parent, there's no need to run any additional antichain checks and we can return
                // this set.
                grandparents.into_iter().collect()
            } else {
                //
                // Iterate through grandparents in order to find an antichain
                for (i, parent) in grandparents.into_iter().enumerate() {
                    let has_reachability_data = self.reachability_service.has_reachability_data(parent);

                    // Reference blocks are the blocks that are used in reachability queries to check if
                    // a candidate is in the future of another candidate. In most cases this is just the
                    // block itself, but in the case where a block doesn't have reachability data we need
                    // to use some blocks in its future as reference instead.
                    // If we make sure to add a parent in the future of the pruning point first, we can
                    // know that any pruned candidate that is in the past of some blocks in the pruning
                    // point anticone should be a parent (in the relevant level) of one of
                    // the origin children in the pruning point anticone. So we can check which
                    // origin children have this block as parent and use those block as
                    // reference blocks.
                    let reference_blocks = if has_reachability_data {
                        smallvec![parent]
                    } else {
                        // Here we explicitly declare the type because otherwise Rust would make it mutable.
                        let origin_children_headers: &Vec<_> = origin_children_headers.get_or_insert_with(|| {
                            self.relations_service
                                .get_children(ORIGIN)
                                .unwrap()
                                .read()
                                .iter()
                                .copied()
                                .map(|parent| self.headers_store.get_header(parent).unwrap())
                                .collect_vec()
                        });
                        let mut reference_blocks = SmallVec::with_capacity(origin_children_headers.len());
                        for child_header in origin_children_headers.iter() {
                            if self.parents_at_level(child_header, block_level).contains(&parent) {
                                reference_blocks.push(child_header.hash);
                            }
                        }
                        reference_blocks
                    };

                    // Make sure we process and insert all first parent's parents. See comments above.
                    // Note that as parents of an already validated block, they all form an antichain,
                    // hence no need for reachability queries yet.
                    if i < first_parent_marker {
                        level_candidates_to_reference_blocks.insert(parent, reference_blocks);
                        continue;
                    }

                    if !has_reachability_data {
                        continue;
                    }

                    let len_before_retain = level_candidates_to_reference_blocks.len();
                    level_candidates_to_reference_blocks
                        .retain(|_, refs| !self.reachability_service.is_any_dag_ancestor(&mut refs.iter().copied(), parent));
                    let is_any_candidate_ancestor_of = level_candidates_to_reference_blocks.len() < len_before_retain;

                    // We should add the block as a candidate if it's in the future of another candidate
                    // or in the anticone of all candidates.
                    if is_any_candidate_ancestor_of
                        || !level_candidates_to_reference_blocks.iter().any(|(_, candidate_references)| {
                            self.reachability_service.is_dag_ancestor_of_any(parent, &mut candidate_references.iter().copied())
                        })
                    {
                        level_candidates_to_reference_blocks.insert(parent, reference_blocks);
                    }
                }

                // After processing all grandparents, collect the successful level candidates
                level_candidates_to_reference_blocks.keys().copied().collect_vec()
            };

            if block_level > 0 && parents_at_level.as_slice() == std::slice::from_ref(&self.genesis_hash) {
                break;
            }

            parents.push(parents_at_level);
        }

        parents
    }

    pub fn parents<'a>(&'a self, header: &'a Header) -> impl ExactSizeIterator<Item = &'a [Hash]> {
        (0..=self.max_block_level).map(|level| self.parents_at_level(header, level))
    }

    pub fn parents_at_level<'a>(&'a self, header: &'a Header, level: u8) -> &'a [Hash] {
        if header.parents_by_level.is_empty() {
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
    use super::*;
    use crate::{
        model::{
            services::{reachability::MTReachabilityService, relations::MTRelationsService},
            stores::{
                headers::{HeaderStoreReader, HeaderWithBlockLevel},
                reachability::MemoryReachabilityStore,
                relations::{MemoryRelationsStore, RelationsStoreReader},
            },
        },
        processes::reachability::tests::{DagBlock, DagBuilder},
    };

    use super::ParentsManager;
    use itertools::Itertools;
    use kaspa_consensus_core::{
        blockhash::{BlockHashes, ORIGIN},
        header::Header,
        BlockHashSet, HashMapCustomHasher,
    };
    use kaspa_database::prelude::{ReadLock, StoreError, StoreResult};
    use kaspa_hashes::Hash;
    use parking_lot::RwLock;

    struct HeaderStoreMock {
        map: RwLock<BlockHashMap<HeaderWithBlockLevel>>,
    }

    impl HeaderStoreMock {
        fn new() -> Self {
            Self { map: RwLock::new(BlockHashMap::new()) }
        }
    }

    #[allow(unused_variables)]
    impl HeaderStoreReader for HeaderStoreMock {
        fn get_daa_score(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
            unimplemented!()
        }

        fn get_timestamp(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
            unimplemented!()
        }

        fn get_bits(&self, hash: kaspa_hashes::Hash) -> Result<u32, StoreError> {
            unimplemented!()
        }

        fn get_header(&self, hash: kaspa_hashes::Hash) -> Result<Arc<Header>, StoreError> {
            Ok(self.map.read().get(&hash).unwrap().header.clone())
        }

        fn get_compact_header_data(
            &self,
            hash: kaspa_hashes::Hash,
        ) -> Result<crate::model::stores::headers::CompactHeaderData, StoreError> {
            unimplemented!()
        }

        fn get_blue_score(&self, hash: kaspa_hashes::Hash) -> Result<u64, StoreError> {
            unimplemented!()
        }

        fn get_header_with_block_level(&self, hash: kaspa_hashes::Hash) -> Result<HeaderWithBlockLevel, StoreError> {
            Ok(self.map.read().get(&hash).unwrap().clone())
        }
    }

    struct RelationsStoreMock {
        pub children: BlockHashes,
    }

    #[allow(unused_variables)]
    impl RelationsStoreReader for RelationsStoreMock {
        fn get_parents(&self, hash: Hash) -> Result<kaspa_consensus_core::blockhash::BlockHashes, StoreError> {
            unimplemented!()
        }

        fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
            Ok(BlockHashSet::from_iter(self.children.iter().copied()).into())
        }

        fn has(&self, hash: Hash) -> Result<bool, StoreError> {
            unimplemented!()
        }

        fn counts(&self) -> Result<(usize, usize), StoreError> {
            unimplemented!()
        }
    }

    struct TestBlock {
        id: u64,
        block_level: u8,
        direct_parents: Vec<u64>,
        expected_parents: Vec<Vec<u64>>,
    }

    #[test]
    fn test_calc_block_parents() {
        let mut reachability_store = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();
        let headers_store = Arc::new(HeaderStoreMock::new());

        let genesis_hash = 3000.into();
        let pruning_point: Hash = 1.into();
        headers_store.map.write().insert(
            pruning_point,
            HeaderWithBlockLevel {
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
            },
        );

        let pp_anticone_block: Hash = 3001.into();
        headers_store.map.write().insert(
            pp_anticone_block,
            HeaderWithBlockLevel {
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
            },
        );

        let pp_anticone_block_child: Hash = 3002.into();
        headers_store.map.write().insert(
            pp_anticone_block_child,
            HeaderWithBlockLevel {
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
            },
        );

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

        let mut dag_builder = DagBuilder::new(&mut reachability_store, &mut relations_store);
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
                HeaderWithBlockLevel {
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
                },
            );
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability_store)));
        let relations_store =
            Arc::new(RwLock::new(vec![RelationsStoreMock { children: BlockHashes::new(vec![pruning_point, pp_anticone_block]) }]));
        let relations_service = MTRelationsService::new(relations_store, 0);
        let parents_manager = ParentsManager::new(250, genesis_hash, headers_store, reachability_service, relations_service);

        for test_block in test_blocks {
            let direct_parents = test_block.direct_parents.iter().map(|parent| Hash::from_u64_word(*parent)).collect_vec();
            let parents = parents_manager.calc_block_parents(pruning_point, &direct_parents);
            let actual_parents = parents.iter().map(|parents| BlockHashSet::from_iter(parents.iter().copied())).collect_vec();
            let expected_parents = test_block
                .expected_parents
                .iter()
                .map(|v| BlockHashSet::from_iter(v.iter().copied().map(Hash::from_u64_word)))
                .collect_vec();
            assert_eq!(expected_parents, actual_parents, "failed for block {}", test_block.id);
        }
    }

    #[test]
    fn test_multiple_pruned_parents() {
        /*
        Tests the following special case of multiple parallel high-level parents which are below the pruning point:
               B
             /   \
            0     0
            |     |
            \    /
              PP (level 0)
             /  \
            1    1
        */

        let mut reachability_store = MemoryReachabilityStore::new();
        let mut relations_store = MemoryRelationsStore::new();
        let headers_store = Arc::new(HeaderStoreMock::new());

        let genesis_hash = 3000.into();
        let pruning_point: Hash = 1.into();
        headers_store.map.write().insert(
            pruning_point,
            HeaderWithBlockLevel {
                header: Arc::new(Header {
                    hash: pruning_point,
                    version: 0,
                    parents_by_level: vec![vec![1001.into(), 1002.into()], vec![1001.into(), 1002.into()]],
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
            },
        );

        let test_blocks = vec![
            TestBlock { id: 2, block_level: 0, direct_parents: vec![1], expected_parents: vec![vec![1], vec![1001, 1002]] },
            TestBlock { id: 3, block_level: 0, direct_parents: vec![1], expected_parents: vec![vec![1], vec![1001, 1002]] },
            TestBlock { id: 4, block_level: 0, direct_parents: vec![2, 3], expected_parents: vec![vec![2, 3], vec![1001, 1002]] },
        ];

        let mut dag_builder = DagBuilder::new(&mut reachability_store, &mut relations_store);
        dag_builder.init().add_block(DagBlock::new(pruning_point, vec![ORIGIN]));

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
                HeaderWithBlockLevel {
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
                },
            );
        }

        let reachability_service = MTReachabilityService::new(Arc::new(RwLock::new(reachability_store)));
        let relations_store = Arc::new(RwLock::new(vec![RelationsStoreMock { children: BlockHashes::new(vec![pruning_point]) }]));
        let relations_service = MTRelationsService::new(relations_store, 0);
        let parents_manager = ParentsManager::new(250, genesis_hash, headers_store, reachability_service, relations_service);

        for test_block in test_blocks {
            let direct_parents = test_block.direct_parents.iter().map(|parent| Hash::from_u64_word(*parent)).collect_vec();
            let parents = parents_manager.calc_block_parents(pruning_point, &direct_parents);
            let actual_parents = parents.iter().map(|parents| BlockHashSet::from_iter(parents.iter().copied())).collect_vec();
            let expected_parents = test_block
                .expected_parents
                .iter()
                .map(|v| BlockHashSet::from_iter(v.iter().copied().map(Hash::from_u64_word)))
                .collect_vec();
            assert_eq!(expected_parents, actual_parents, "failed for block {}", test_block.id);
        }
    }
}
