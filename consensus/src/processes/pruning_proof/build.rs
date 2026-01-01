use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    header::Header,
    pruning::PruningPointProof,
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
};
use kaspa_core::debug;
use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreError, StoreResult, StoreResultExt, StoreResultUnitExt, DB};
use kaspa_hashes::Hash;
use kaspa_utils::vec::VecExtensions;
use parking_lot::RwLock;

use crate::{
    model::{
        services::{reachability::ReachabilityService, relations::MTRelationsService},
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{HeaderStoreReader, HeaderWithBlockLevel},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
        pruning_proof::{GhostdagReaderExt, PruningProofManagerInternalError},
        relations::RelationsStoreExtensions,
    },
};

use super::{PruningProofManager, PruningProofManagerInternalResult};
type LevelProofContext = (Arc<DbGhostdagStore>, Arc<RwLock<DbRelationsStore>>, Hash, Hash);

struct MultiLevelProofContext {
    transient_ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    transient_relations_stores: Vec<Arc<RwLock<DbRelationsStore>>>,
    selected_tip_by_level: Vec<Hash>,
    roots_by_level: Vec<Hash>,
}

#[derive(Clone)]
struct RelationsStoreInFutureOfRoot<T: RelationsStoreReader, U: ReachabilityService> {
    relations_store: T,
    reachability_service: U,
    root: Hash,
}

impl<T: RelationsStoreReader, U: ReachabilityService> RelationsStoreReader for RelationsStoreInFutureOfRoot<T, U> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, kaspa_database::prelude::StoreError> {
        self.relations_store.get_parents(hash).map(|hashes| {
            Arc::new(
                hashes
                    .iter()
                    .copied()
                    // TODO(relaxed): consider removing this filtering altogether - in the context this is currently called
                    // blocks should not be inserted if they do not fulfill this condition
                    .filter(|h| self.reachability_service.is_dag_ancestor_of_result(self.root, *h).optional().unwrap().unwrap_or(false))
                    .collect_vec(),
            )
        })
    }

    fn get_children(&self, hash: Hash) -> StoreResult<kaspa_database::prelude::ReadLock<BlockHashSet>> {
        // We assume hash is in future of root
        assert!(self.reachability_service.is_dag_ancestor_of(self.root, hash));
        self.relations_store.get_children(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        if self.reachability_service.is_dag_ancestor_of(self.root, hash) {
            Ok(false)
        } else {
            self.relations_store.has(hash)
        }
    }

    fn counts(&self) -> Result<(usize, usize), kaspa_database::prelude::StoreError> {
        unimplemented!()
    }
}

impl PruningProofManager {
    pub(crate) fn build_pruning_point_proof(&self, pp: Hash) -> PruningPointProof {
        if pp == self.genesis_hash {
            return vec![];
        }

        let (_db_lifetime, temp_db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let pp_header = self.headers_store.get_header_with_block_level(pp).unwrap();
        let MultiLevelProofContext { transient_ghostdag_stores, transient_relations_stores, selected_tip_by_level, roots_by_level } =
            self.calc_all_level_proof_stores(&pp_header, temp_db);

        // The pruning proof can contain many duplicate headers (across levels), so we use a local cache in order
        // to make sure we hold a single Arc per header
        let mut cache: BlockHashMap<Arc<Header>> = BlockHashMap::with_capacity(4 * self.pruning_proof_m as usize);
        let mut get_header = |hash| cache.entry(hash).or_insert_with_key(|&hash| self.headers_store.get_header(hash).unwrap()).clone();

        (0..=self.max_block_level)
            .map(|level| {
                let level = level as usize;
                let selected_tip = selected_tip_by_level[level];
                let block_at_depth_2m = transient_ghostdag_stores[level]
                    .block_at_depth(selected_tip, 2 * self.pruning_proof_m)
                    .map_err(|err| format!("level: {}, err: {}", level, err))
                    .unwrap();

                // TODO (relaxed): remove the assertion below
                // (New Logic) This is the root we calculated by going through block relations
                let root = roots_by_level[level];
                // (Old Logic) This is the root we can calculate given that the GD records are already filled
                // The root calc logic below is the original logic before the on-demand higher level GD calculation
                // We only need old_root to sanity check the new logic
                let old_root = if level != self.max_block_level as usize {
                    let block_at_depth_m_at_next_level = transient_ghostdag_stores[level + 1]
                        .block_at_depth(selected_tip_by_level[level + 1], self.pruning_proof_m)
                        .map_err(|err| format!("level + 1: {}, err: {}", level + 1, err))
                        .unwrap();
                    if self.reachability_service.is_dag_ancestor_of(block_at_depth_m_at_next_level, block_at_depth_2m) {
                        block_at_depth_m_at_next_level
                    } else if self.reachability_service.is_dag_ancestor_of(block_at_depth_2m, block_at_depth_m_at_next_level) {
                        block_at_depth_2m
                    } else {
                        self.find_common_ancestor_in_chain_of_a(
                            &*transient_ghostdag_stores[level],
                            block_at_depth_m_at_next_level,
                            block_at_depth_2m,
                        )
                        .map_err(|err| format!("level: {}, err: {}", level, err))
                        .unwrap()
                    }
                } else {
                    block_at_depth_2m
                };

                // new root is expected to be always an ancestor of old_root because new root takes a safety margin
                assert!(self.reachability_service.is_dag_ancestor_of(root, old_root));

                let mut headers = Vec::with_capacity(2 * self.pruning_proof_m as usize);
                let mut queue = BinaryHeap::<Reverse<SortableBlock>>::new();
                let mut visited = BlockHashSet::new();
                queue.push(Reverse(SortableBlock::new(root, get_header(root).blue_work)));
                while let Some(current) = queue.pop() {
                    let current = current.0.hash;
                    if !visited.insert(current) {
                        continue;
                    }

                    // The second condition is always expected to be true (ghostdag store will have the entry)
                    // because we are traversing the exact diamond (future(root) ⋂ past(tip)) for which we calculated
                    // GD for (see fill_level_proof_ghostdag_data). TODO (relaxed): remove the condition or turn into assertion
                    if !self.reachability_service.is_dag_ancestor_of(current, selected_tip)
                        || !transient_ghostdag_stores[level].has(current).is_ok_and(|found| found)
                    {
                        continue;
                    }

                    headers.push(get_header(current));
                    for child in transient_relations_stores[level].read().get_children(current).unwrap().read().iter().copied() {
                        queue.push(Reverse(SortableBlock::new(child, get_header(child).blue_work)));
                    }
                }

                // TODO (relaxed): remove the assertion below
                // Temp assertion for verifying a bug fix: assert that the full 2M chain is actually contained in the composed level proof
                let set = BlockHashSet::from_iter(headers.iter().map(|h| h.hash));
                let chain_2m = self
                    .chain_up_to_depth(&*transient_ghostdag_stores[level], selected_tip, 2 * self.pruning_proof_m)
                    .map_err(|err| {
                        dbg!(level, selected_tip, block_at_depth_2m, root);
                        format!("Assert 2M chain -- level: {}, err: {}", level, err)
                    })
                    .unwrap();
                let chain_2m_len = chain_2m.len();
                for (i, chain_hash) in chain_2m.into_iter().enumerate() {
                    if !set.contains(&chain_hash) {
                        let next_level_tip = selected_tip_by_level[level + 1];
                        let next_level_chain_m = self
                            .chain_up_to_depth(&*transient_ghostdag_stores[level + 1], next_level_tip, self.pruning_proof_m)
                            .unwrap();
                        let next_level_block_m = next_level_chain_m.last().copied().unwrap();
                        dbg!(next_level_chain_m.len());
                        dbg!(transient_ghostdag_stores[level + 1].get_compact_data(next_level_tip).unwrap().blue_score);
                        dbg!(transient_ghostdag_stores[level + 1].get_compact_data(next_level_block_m).unwrap().blue_score);
                        dbg!(transient_ghostdag_stores[level].get_compact_data(selected_tip).unwrap().blue_score);
                        dbg!(transient_ghostdag_stores[level].get_compact_data(block_at_depth_2m).unwrap().blue_score);
                        dbg!(level, selected_tip, block_at_depth_2m, root);
                        panic!("Assert 2M chain -- missing block {} at index {} out of {} chain blocks", chain_hash, i, chain_2m_len);
                    }
                }

                headers
            })
            .collect_vec()
    }

    fn calc_all_level_proof_stores(&self, pp_header: &HeaderWithBlockLevel, temp_db: Arc<DB>) -> MultiLevelProofContext {
        // TODO: Uncomment line and send as argument to find_sufficiently_deep_level_root
        // once full fix to minimize proof sizes comes
        // let current_dag_level = self.find_current_dag_level(&pp_header.header);
        let mut level_proof_stores_vec: Vec<Option<LevelProofContext>> = vec![None; (self.max_block_level + 1).into()];
        for level in (0..=self.max_block_level).rev() {
            let level_usize = level as usize;
            let required_block = if level != self.max_block_level {
                let (next_level_gd_store, _relation_store_at_next_level, selected_tip_at_next_level, _root_at_next_level) =
                    level_proof_stores_vec[level_usize + 1].as_ref().unwrap();

                let block_at_depth_m_at_next_level = next_level_gd_store
                    .block_at_depth(*selected_tip_at_next_level, self.pruning_proof_m)
                    .map_err(|err| format!("level + 1: {}, err: {}", level + 1, err))
                    .unwrap();
                Some(block_at_depth_m_at_next_level)
            } else {
                None
            };
            level_proof_stores_vec[level_usize] = Some(
                self.find_sufficiently_deep_level_root(pp_header, level, required_block, temp_db.clone())
                    .unwrap_or_else(|_| panic!("find_sufficient_root failed for level {level}")),
            );
        }

        let (transient_ghostdag_stores, transient_relations_stores, selected_tip_by_level, roots_by_level) =
            level_proof_stores_vec.into_iter().map(Option::unwrap).multiunzip();

        MultiLevelProofContext { transient_ghostdag_stores, transient_relations_stores, selected_tip_by_level, roots_by_level }
    }

    /// Builds and returns a store of parents-children relations at a given level
    /// to facilitate forward traversal of a snippet of the Dag at that level.
    /// This relation store contains all reachable blocks
    /// that belong in the future of root and the past of tip.
    /// Non reachable blocks are filtered out as they are not conceptually a part of the Dag at that level
    /// but simply remain in node storage for other reasons.
    fn populate_relation_store_at_level(
        &self,
        temp_db: Arc<DB>,
        tip: Hash,
        root: Hash,
        level: BlockLevel,
        try_number: u8,
    ) -> DbRelationsStore {
        //TODO(relaxed): currently we rebuild the relation store in its entirety for each try.
        // A more efficient implementation could instead extend the store constructed in the previous try
        //TODO: Remove sanity checks when convinced code is stable and correct
        let mut queue = VecDeque::new();
        let mut visited = BlockHashSet::new();
        queue.push_back(tip);
        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);
        let mut level_relation_store = DbRelationsStore::new_temp(temp_db.clone(), level, try_number, cache_policy, cache_policy);

        level_relation_store.insert(ORIGIN, BlockHashes::new(vec![])).unwrap();

        // sanity check
        assert!(self.reachability_service.is_dag_ancestor_of(root, tip));
        while let Some(hash) = queue.pop_front() {
            if !visited.insert(hash) {
                continue;
            }
            if hash == ORIGIN {
                continue;
            }
            let header = self.headers_store.get_header(hash).unwrap();
            let parents = Arc::new(
                self.parents_manager
                    .parents_at_level(&header, level)
                    .iter()
                    .copied()
                    .filter(|&p| self.reachability_service.is_dag_ancestor_of_result(root, p).optional().unwrap().unwrap_or(false))
                    .collect::<Vec<_>>()
                    .push_if_empty(ORIGIN),
            );
            // sanity check
            if hash == root {
                assert_eq!(parents.len(), 1);
                assert_eq!(parents[0], ORIGIN);
            }
            // Write parents to the relations store
            level_relation_store.insert(hash, parents.clone()).unwrap();

            // Enqueue parents to fill full upper chain
            for &p in parents.iter() {
                queue.push_back(p);
            }
        }
        //sanity checks
        for hash in visited {
            assert!(level_relation_store.has(hash).unwrap());
        }
        let children_read = level_relation_store.get_children(ORIGIN).unwrap();
        let origin_children = children_read.read();
        assert!(origin_children.len() == 1);
        assert!(origin_children.contains(&root));

        level_relation_store
    }

    /// Find a sufficient root at a given level by going through the headers store and looking
    /// for a deep enough level block
    /// For each root candidate, fill in the ghostdag data to see if it actually is deep enough.
    /// If the root is deep enough, it will satisfy these conditions
    /// 1. block at depth 2m at this level ∈ Future(root)
    /// 2. block at depth m at the next level ∈ Future(root)
    ///
    /// Returns: the filled ghostdag store from root to tip, the selected tip and the root
    fn find_sufficiently_deep_level_root(
        &self,
        pp_header: &HeaderWithBlockLevel,
        level: BlockLevel,
        required_block: Option<Hash>,
        temp_db: Arc<DB>,
    ) -> PruningProofManagerInternalResult<LevelProofContext> {
        // Step 1: Determine which selected tip to use
        let selected_tip = if pp_header.block_level >= level {
            pp_header.header.hash
        } else {
            self.find_approximate_selected_parent_header_at_level(&pp_header.header, level)?.hash
        };

        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);
        let required_level_depth = 2 * self.pruning_proof_m;

        // We only have the headers store (which has level 0 blue_scores) to assemble the proof data from.
        // We need to look deeper at higher levels (2x deeper every level) to find 2M (plus margin) blocks at that level
        // TODO: uncomment when the full fix to minimize proof sizes comes, and add current_dag_level as an argument
        // let mut required_base_level_depth = self.estimated_blue_depth_at_level_0(
        //     level,
        //     required_level_depth + 100, // We take a safety margin
        //     current_dag_level,
        // );
        // NOTE: Starting from required_level_depth (a much lower starting point than normal) will typically require O(N) iterations
        // for level L + N where L is the current dag level. This is fine since the steps per iteration are still exponential
        // and so we will complete each level in not much more than N iterations per level.
        // We start here anyway so we can try to minimize the proof size when the current dag level goes down significantly.
        let mut required_base_level_depth = required_level_depth + 100;

        let mut is_last_level_header;
        let mut tries = 0;

        let block_at_depth_m_at_next_level = required_block.unwrap_or(selected_tip);

        loop {
            // Step 2 - Find a deep enough root candidate
            let block_at_depth_2m = match self.level_block_at_base_depth(level, selected_tip, required_base_level_depth) {
                Ok((header, is_last_header)) => {
                    is_last_level_header = is_last_header;
                    header
                }
                Err(e) => return Err(e),
            };

            let root = if self.reachability_service.is_dag_ancestor_of(block_at_depth_2m, block_at_depth_m_at_next_level) {
                block_at_depth_2m
            } else if self.reachability_service.is_dag_ancestor_of(block_at_depth_m_at_next_level, block_at_depth_2m) {
                block_at_depth_m_at_next_level
            } else {
                // find common ancestor of block_at_depth_m_at_next_level and block_at_depth_2m in chain of block_at_depth_m_at_next_level
                let mut common_ancestor = self.headers_store.get_header(block_at_depth_m_at_next_level).unwrap();
                while !self.reachability_service.is_dag_ancestor_of(common_ancestor.hash, block_at_depth_2m) {
                    common_ancestor = match self.find_approximate_selected_parent_header_at_level(&common_ancestor, level) {
                        Ok(header) => header,
                        // Try to give this last header a chance at being root
                        Err(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(_)) => break,
                        Err(e) => return Err(e),
                    };
                }

                common_ancestor.hash
            };

            // Step 3 - Fill the ghostdag data from root to tip.
            //
            // First, derive the relevant relations down to the candidate root.
            let transient_relation_store =
                Arc::new(RwLock::new(self.populate_relation_store_at_level(temp_db.clone(), selected_tip, root, level, tries)));

            let transient_ghostdag_store =
                Arc::new(DbGhostdagStore::new_temp(temp_db.clone(), level, cache_policy, cache_policy, tries));
            let has_required_block = self.fill_level_proof_ghostdag_data(
                root,
                pp_header.header.hash,
                &transient_ghostdag_store,
                Some(block_at_depth_m_at_next_level),
                level,
                &transient_relation_store,
                self.ghostdag_k,
            );

            // Step 4 - Check if we actually have enough depth.
            // Need to ensure this does the same 2M+1 depth that block_at_depth does
            if has_required_block
                && (root == self.genesis_hash
                    || transient_ghostdag_store.get_blue_score(selected_tip).unwrap() >= required_level_depth)
            {
                break Ok((transient_ghostdag_store, transient_relation_store, selected_tip, root));
            }

            tries += 1;
            if is_last_level_header {
                if has_required_block {
                    // Normally this scenario doesn't occur when syncing with nodes that already have the safety margin change in place.
                    // However, when syncing with an older node version that doesn't have a safety margin for the proof, it's possible to
                    // try to find 2500 depth worth of headers at a level, but the proof only contains about 2000 headers. To be able to sync
                    // with such an older node. As long as we found the required block, we can still proceed.
                    debug!("Failed to find sufficient root for level {level} after {tries} tries. Headers below the current depth of {required_base_level_depth} are already pruned. Required block found so trying anyway.");
                    break Ok((transient_ghostdag_store, transient_relation_store, selected_tip, root));
                } else {
                    panic!("Failed to find sufficient root for level {level} after {tries} tries. Headers below the current depth of {required_base_level_depth} are already pruned");
                }
            }

            // If we don't have enough depth now, we need to look deeper
            required_base_level_depth = (required_base_level_depth as f64 * 1.1) as u64;
            debug!("Failed to find sufficient root for level {level} after {tries} tries. Retrying again to find with depth {required_base_level_depth}");
        }
    }

    /// BFS forward iterates from root until selected tip, ignoring blocks in the antipast of selected_tip.
    /// For each block along the way, insert that hash into the ghostdag_store
    /// If we have a required_block to find, this will return true if that block was found along the way
    fn fill_level_proof_ghostdag_data(
        &self,
        root: Hash,
        selected_tip: Hash,
        transient_ghostdag_store: &Arc<DbGhostdagStore>,
        required_block: Option<Hash>,
        level: BlockLevel,
        transient_relations_store: &Arc<RwLock<DbRelationsStore>>,
        ghostdag_k: KType,
    ) -> bool {
        let transient_relations_service = RelationsStoreInFutureOfRoot {
            relations_store: MTRelationsService::new(transient_relations_store.clone()),
            reachability_service: self.reachability_service.clone(),
            root,
        };
        let transient_gd_manager = GhostdagManager::with_level(
            root,
            ghostdag_k,
            transient_ghostdag_store.clone(),
            transient_relations_service.clone(),
            self.headers_store.clone(),
            self.reachability_service.clone(),
            level,
            self.max_block_level,
        );

        // Note there is no need to initialize origin since we have a single root
        transient_ghostdag_store.insert(root, Arc::new(transient_gd_manager.genesis_ghostdag_data())).unwrap();

        let mut topological_heap: BinaryHeap<_> = Default::default();
        let mut visited = BlockHashSet::new();
        for child in transient_relations_service.get_children(root).unwrap().read().iter().copied() {
            topological_heap
                .push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
        }

        let mut has_required_block = required_block.is_some_and(|required_block| root == required_block);
        loop {
            let Some(current) = topological_heap.pop() else {
                break;
            };
            let current_hash = current.0.hash;
            if !visited.insert(current_hash) {
                continue;
            }

            if !self.reachability_service.is_dag_ancestor_of(current_hash, selected_tip) {
                // We don't care about blocks in the antipast of the selected tip
                continue;
            }

            if !has_required_block && required_block.is_some_and(|required_block| current_hash == required_block) {
                has_required_block = true;
            }

            let current_gd = transient_gd_manager.ghostdag(&transient_relations_service.get_parents(current_hash).unwrap());

            transient_ghostdag_store.insert(current_hash, Arc::new(current_gd)).idempotent().unwrap();

            for child in transient_relations_service.get_children(current_hash).unwrap().read().iter().copied() {
                topological_heap
                    .push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
            }
        }

        has_required_block
    }

    // The "current dag level" is the level right before the level whose parents are
    // not the same as our header's direct parents
    //
    // Find the current DAG level by going through all the parents at each level,
    // starting from the bottom level and see which is the first level that has
    // parents that are NOT our current pp_header's direct parents.
    fn find_current_dag_level(&self, pp_header: &Header) -> BlockLevel {
        let direct_parents = BlockHashSet::from_iter(pp_header.direct_parents().iter().copied());
        pp_header
            .parents_by_level
            .expanded_iter()
            .enumerate()
            .skip(1) // skip checking direct parents
            .find_map(|(level, parents)| {
                if BlockHashSet::from_iter(parents.iter().copied()) == direct_parents {
                    None
                } else {
                    Some((level - 1) as BlockLevel)
                }
            })
            .unwrap_or(self.max_block_level)
    }

    fn estimated_blue_depth_at_level_0(&self, level: BlockLevel, level_depth: u64, current_dag_level: BlockLevel) -> u64 {
        level_depth.checked_shl(level.saturating_sub(current_dag_level) as u32).unwrap_or(level_depth)
    }

    /// selected parent at level = the parent of the header at the level
    /// with the highest blue_work
    fn find_approximate_selected_parent_header_at_level(
        &self,
        header: &Header,
        level: BlockLevel,
    ) -> PruningProofManagerInternalResult<Arc<Header>> {
        // Parents manager parents_at_level may return parents that aren't currently in database and those are filtered out.
        // This is ok because this function is called in the context of deriving a root deep enough for a proof at this level,
        // not to find the "best" such proof
        let sp = self
            .parents_manager
            .parents_at_level(header, level)
            .iter()
            .copied()
            // filtering by the existence of headers alone does not suffice because we store the headers of all past pruning points, but these are not conceptually a part of the DAG
            // or the pruning proof and are not reachable under normal means. 
            .filter(|&p| self.reachability_service.has_reachability_data(p))
            .filter_map(|p| self.headers_store.get_header(p).optional().unwrap().map(|h| SortableBlock::new(p, h.blue_work)))
            .max()
            .ok_or(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof("no parents with header".to_string()))?;
        Ok(self.headers_store.get_header(sp.hash).expect("unwrapped above"))
    }

    /// Finds a block on a given level that is at base_depth deep from it.
    /// Also returns if the block was the last one in the level
    /// base_depth = the blue score depth at level 0
    fn level_block_at_base_depth(
        &self,
        level: BlockLevel,
        high: Hash,
        base_depth: u64,
    ) -> PruningProofManagerInternalResult<(Hash, bool)> {
        let high_header = self
            .headers_store
            .get_header(high)
            .map_err(|err| PruningProofManagerInternalError::BlockAtDepth(format!("high: {high}, depth: {base_depth}, {err}")))?;
        let high_header_score = high_header.blue_score;
        let mut current_header = high_header;

        let mut is_last_header = false;

        while current_header.blue_score + base_depth >= high_header_score {
            if current_header.direct_parents().is_empty() {
                // Reached genesis
                break;
            }

            current_header = match self.find_approximate_selected_parent_header_at_level(&current_header, level) {
                Ok(header) => header,
                Err(PruningProofManagerInternalError::NotEnoughHeadersToBuildProof(_)) => {
                    // We want to give this root a shot if all its past is pruned
                    is_last_header = true;
                    break;
                }
                Err(e) => return Err(e),
            };
        }
        Ok((current_header.hash, is_last_header))
    }

    /// Copy of `block_at_depth` which returns the full chain up to depth. Temporarily used for assertion purposes.
    fn chain_up_to_depth(
        &self,
        transient_ghostdag_store: &impl GhostdagStoreReader,
        high: Hash,
        depth: u64,
    ) -> Result<Vec<Hash>, PruningProofManagerInternalError> {
        let high_gd = transient_ghostdag_store
            .get_compact_data(high)
            .map_err(|err| PruningProofManagerInternalError::BlockAtDepth(format!("high: {high}, depth: {depth}, {err}")))?;
        let mut current_gd = high_gd;
        let mut current = high;
        let mut res = vec![current];
        while current_gd.blue_score + depth >= high_gd.blue_score {
            if current_gd.selected_parent.is_origin() {
                break;
            }
            let prev = current;
            current = current_gd.selected_parent;
            res.push(current);
            current_gd = transient_ghostdag_store.get_compact_data(current).map_err(|err| {
                PruningProofManagerInternalError::BlockAtDepth(format!(
                    "high: {}, depth: {}, current: {}, high blue score: {}, current blue score: {}, {}",
                    high, depth, prev, high_gd.blue_score, current_gd.blue_score, err
                ))
            })?;
        }
        Ok(res)
    }

    fn find_common_ancestor_in_chain_of_a(
        &self,
        transient_ghostdag_store: &impl GhostdagStoreReader,
        a: Hash,
        b: Hash,
    ) -> Result<Hash, PruningProofManagerInternalError> {
        let a_gd = transient_ghostdag_store
            .get_compact_data(a)
            .map_err(|err| PruningProofManagerInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        let mut current_gd = a_gd;
        let mut current;
        let mut loop_counter = 0;
        loop {
            current = current_gd.selected_parent;
            loop_counter += 1;
            if current.is_origin() {
                break Err(PruningProofManagerInternalError::NoCommonAncestor(format!("a: {a}, b: {b} ({loop_counter} loop steps)")));
            }
            if self.reachability_service.is_dag_ancestor_of(current, b) {
                break Ok(current);
            }
            current_gd = transient_ghostdag_store
                .get_compact_data(current)
                .map_err(|err| PruningProofManagerInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        }
    }
}
