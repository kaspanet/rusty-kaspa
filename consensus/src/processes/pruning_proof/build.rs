use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, BlockHashes},
    header::Header,
    pruning::PruningPointProof,
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
};
use kaspa_core::debug;
use kaspa_database::prelude::{CachePolicy, ConnBuilder, StoreError, StoreResult, StoreResultExt, StoreResultUnitExt, DB};
use kaspa_hashes::Hash;
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
        pruning_proof::{GhostdagReaderExt, PpmInternalError},
        relations::RelationsStoreExtensions,
    },
};

use super::{PpmInternalResult, PruningProofManager};
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
                    .filter(|h| {
                        self.reachability_service.is_dag_ancestor_of_result(self.root, *h).optional().unwrap().unwrap_or(false)
                    })
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
                self.calc_level_proof_context(pp_header, level, required_block, temp_db.clone())
                    .unwrap_or_else(|_| panic!("find_sufficient_root failed for level {level}")),
            );
        }

        let (transient_ghostdag_stores, transient_relations_stores, selected_tip_by_level, roots_by_level) =
            level_proof_stores_vec.into_iter().map(Option::unwrap).multiunzip();

        MultiLevelProofContext { transient_ghostdag_stores, transient_relations_stores, selected_tip_by_level, roots_by_level }
    }

    /// Given a current hash, count the blocks in its future
    /// Do this by:
    /// 1. Getting the largest child in terms of future size (this ensures that the rest of the iteration is minimized since most of the future is covered)
    /// 2. BFS traversal of all other children and their futures, skipping blocks that are in the future of the largest child.
    ///    This is similar to the mergeset calculation logic. Add each block seen to the count.
    fn count_future_size(&self, relation_store: &DbRelationsStore, current: Hash, future_sizes_map: &BlockHashMap<u64>) -> u64 {
        let mut queue = VecDeque::new();
        let mut visited = BlockHashSet::new();

        let children_lock = relation_store.get_children(current).unwrap();
        let children_read = children_lock.read();
        let largest_child = children_read
            .iter()
            .map(|c| {
                // Initialize the queue with all the children of current hash
                queue.push_back(*c);

                (c, future_sizes_map.get(c).copied().unwrap_or(0))
            })
            .max_by_key(|(_, depth)| *depth);

        let mut count = 0;

        if let Some(largest_child) = largest_child {
            // Add all the count of future of the largest child
            let largest_child_hash = *largest_child.0;
            count += largest_child.1 + 1; // +1 to include the largest child itself

            while let Some(hash) = queue.pop_front() {
                if !visited.insert(hash) {
                    continue;
                }

                // Skip blocks in the future of the largest child
                if self.reachability_service.is_dag_ancestor_of(largest_child_hash, hash) {
                    continue;
                }

                count += 1;
                let children_read = relation_store.get_children(hash).unwrap();
                for &child in children_read.read().iter() {
                    queue.push_back(child);
                }
            }
        }

        debug!("Counted future size of {} as {}", current, count);

        count
    }

    /// Calculate the level proof context by:
    /// 1. Traversing backwards from the selected tip, filling the relations store along the way
    /// 2. If a candidate root is found (sufficient future size and depth), fill GD data for this root to the tip:
    ///    - If the GD data satisfies the requirements of a level proof, return it
    ///    - Otherwise, keep continue traversing backwards
    fn calc_level_proof_context(
        &self,
        pp_header: &HeaderWithBlockLevel,
        level: BlockLevel,
        required_block: Option<Hash>,
        temp_db: Arc<DB>,
    ) -> PpmInternalResult<LevelProofContext> {
        let selected_tip = if pp_header.block_level >= level {
            pp_header.header.hash
        } else {
            self.find_approximate_selected_parent_header_at_level(&pp_header.header, level)?.hash
        };

        let tip_bs = self.headers_store.get_blue_score(selected_tip).unwrap();

        let required_level_depth = 2 * self.pruning_proof_m;
        // Add a "safety margin"
        let required_base_level_depth = required_level_depth + 100;
        let block_at_depth_m_at_next_level = required_block.unwrap_or(selected_tip);

        // BFS backward from the tip for relations.
        // We need to traverse in backward topological order to ensure that the GD attempts have all needed relations
        let mut queue = BinaryHeap::<SortableBlock>::new();
        let mut visited = BlockHashSet::new();
        queue.push(SortableBlock { hash: selected_tip, blue_work: self.headers_store.get_header(selected_tip).unwrap().blue_work });

        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);

        // Only a single try is needed for this since we will maintain this single relations store
        let mut level_relation_store = DbRelationsStore::new_temp(temp_db.clone(), level, 0, cache_policy, cache_policy);

        // Maps the each known block to their future sizes (up to selected tip)
        let mut future_sizes_map = BlockHashMap::<u64>::new();
        let mut gd_tries = 0;

        // A level may not contain enough headers to satisfy the safety margin.
        // This is intended to give the last header a chance, since it may still be deep enough
        // for a level proof.
        let mut is_last_header = false;

        while let Some(sb) = queue.pop() {
            let current_hash = sb.hash;
            if !visited.insert(current_hash) {
                continue;
            }

            let header = self.headers_store.get_header(current_hash).unwrap();

            let parents: BlockHashes = self
                .parents_manager
                .parents_at_level(&header, level)
                .iter()
                .copied()
                .filter(|&p| self.reachability_service.has_reachability_data(p))
                .collect::<Vec<_>>()
                .into();

            is_last_header = is_last_header || parents.is_empty();

            // Write parents to the relations store
            level_relation_store.insert(current_hash, parents.clone()).unwrap();

            debug!("Level: {} | Counting future size of {}", level, current_hash);
            let future_size = self.count_future_size(&level_relation_store, current_hash, &future_sizes_map);
            future_sizes_map.insert(current_hash, future_size);
            debug!("Level: {} | Hash: {} | Future Size: {}", level, current_hash, future_size);

            // If the current hash is valid root candidate, fill the GD store and see if it passes as a level proof
            // Valid root candidates are:
            // 1. The genesis block
            // 2. The last header in the headers store
            // 3. Any known block that is in the selected chain from tip, has sufficient future size and depth
            if current_hash == self.genesis_hash
                || is_last_header
                || (future_size >= 2 * self.pruning_proof_m - 1 // -1 because future size does not include the current block
                    && tip_bs.saturating_sub(header.blue_score) >= required_base_level_depth)
            {
                let root = if self.reachability_service.is_dag_ancestor_of(current_hash, block_at_depth_m_at_next_level) {
                    current_hash
                } else if self.reachability_service.is_dag_ancestor_of(block_at_depth_m_at_next_level, current_hash) {
                    block_at_depth_m_at_next_level
                } else {
                    // find common ancestor of block_at_depth_m_at_next_level and block_at_depth_2m in chain of block_at_depth_m_at_next_level
                    let mut common_ancestor = self.headers_store.get_header(block_at_depth_m_at_next_level).unwrap();
                    while !self.reachability_service.is_dag_ancestor_of(common_ancestor.hash, current_hash) {
                        common_ancestor = match self.find_approximate_selected_parent_header_at_level(&common_ancestor, level) {
                            Ok(header) => header,
                            // Try to give this last header a chance at being root
                            Err(PpmInternalError::NotEnoughHeadersToBuildProof(_)) => break,
                            Err(e) => return Err(e),
                        };
                    }

                    common_ancestor.hash
                };

                // If level relation store does not have the needed root, it means we need to continue the outer BFS and fill the
                // relation store
                if level_relation_store.has(root).unwrap() {
                    let transient_relation_store = Arc::new(RwLock::new(level_relation_store.clone()));

                    let transient_ghostdag_store =
                        Arc::new(DbGhostdagStore::new_temp(temp_db.clone(), level, cache_policy, cache_policy, gd_tries));
                    let has_required_block = self.fill_level_proof_ghostdag_data(
                        root,
                        selected_tip,
                        &transient_ghostdag_store,
                        Some(required_block.unwrap_or(selected_tip)),
                        level,
                        &transient_relation_store,
                        self.ghostdag_k,
                    );

                    // Step 4 - Check if we actually have enough depth.
                    // Need to ensure this does the same 2M+1 depth that block_at_depth does
                    let curr_tip_bs = transient_ghostdag_store.get_blue_score(selected_tip).unwrap();
                    if has_required_block
                        && (root == self.genesis_hash
                            || curr_tip_bs >= required_level_depth
                            || (is_last_header && curr_tip_bs > 2 * self.pruning_proof_m))
                    {
                        return Ok((transient_ghostdag_store, transient_relation_store, selected_tip, root));
                    }

                    gd_tries += 1;
                }
            }

            // Enqueue parents to fill full upper chain
            for &p in parents.iter() {
                queue.push(SortableBlock { hash: p, blue_work: self.headers_store.get_header(p).unwrap().blue_work });
            }
        }

        panic!("Failed to find sufficient root for level {level} after exhausting all known headers.");
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

    /// selected parent at level = the parent of the header at the level
    /// with the highest blue_work
    fn find_approximate_selected_parent_header_at_level(&self, header: &Header, level: BlockLevel) -> PpmInternalResult<Arc<Header>> {
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
            .ok_or(PpmInternalError::NotEnoughHeadersToBuildProof("no parents with header".to_string()))?;
        Ok(self.headers_store.get_header(sp.hash).expect("unwrapped above"))
    }

    /// Copy of `block_at_depth` which returns the full chain up to depth. Temporarily used for assertion purposes.
    fn chain_up_to_depth(
        &self,
        transient_ghostdag_store: &impl GhostdagStoreReader,
        high: Hash,
        depth: u64,
    ) -> Result<Vec<Hash>, PpmInternalError> {
        let high_gd = transient_ghostdag_store
            .get_compact_data(high)
            .map_err(|err| PpmInternalError::BlockAtDepth(format!("high: {high}, depth: {depth}, {err}")))?;
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
                PpmInternalError::BlockAtDepth(format!(
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
    ) -> Result<Hash, PpmInternalError> {
        let a_gd = transient_ghostdag_store
            .get_compact_data(a)
            .map_err(|err| PpmInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        let mut current_gd = a_gd;
        let mut current;
        let mut loop_counter = 0;
        loop {
            current = current_gd.selected_parent;
            loop_counter += 1;
            if current.is_origin() {
                break Err(PpmInternalError::NoCommonAncestor(format!("a: {a}, b: {b} ({loop_counter} loop steps)")));
            }
            if self.reachability_service.is_dag_ancestor_of(current, b) {
                break Ok(current);
            }
            current_gd = transient_ghostdag_store
                .get_compact_data(current)
                .map_err(|err| PpmInternalError::FindCommonAncestor(format!("a: {a}, b: {b}, {err}")))?;
        }
    }
}
