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
use kaspa_core::{debug, trace};
use kaspa_database::prelude::*;
use kaspa_hashes::Hash;

use crate::{
    model::{
        services::reachability::ReachabilityService,
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
type LevelProofContext = (Arc<DbGhostdagStore>, Arc<DbRelationsStore>, Hash, Hash);

struct MultiLevelProofContext {
    transient_ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    transient_relations_stores: Vec<Arc<DbRelationsStore>>,
    selected_tip_by_level: Vec<Hash>,
    roots_by_level: Vec<Hash>,
}

/// A relations-store reader restricted to the future cone of a fixed root block (including the root).
///
/// Only parents and children that lie within the root’s future cone are exposed.
/// This provides a consistent, root-relative view of relations when operating on
/// proofs or subgraphs confined to that region of the DAG.
#[derive(Clone)]
struct FutureConeRelations<T: RelationsStoreReader, U: ReachabilityService> {
    relations_store: T,
    reachability_service: U,
    root: Hash,
}

impl<T: RelationsStoreReader, U: ReachabilityService> FutureConeRelations<T, U> {
    fn new(relations_store: T, reachability_service: U, root: Hash) -> Self {
        Self { relations_store, reachability_service, root }
    }
}

impl<T: RelationsStoreReader, U: ReachabilityService> RelationsStoreReader for FutureConeRelations<T, U> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.relations_store.get_parents(hash).map(|hashes| {
            // Reachability queries are safe here, since in this context all blocks are reached via `reachable_parents_at_level`
            hashes.iter().copied().filter(|&h| self.reachability_service.is_dag_ancestor_of(self.root, h)).collect_vec().into()
        })
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        assert!(self.reachability_service.is_dag_ancestor_of(self.root, hash), "future(root) invariant violated");
        self.relations_store.get_children(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.relations_store.has(hash)? && self.reachability_service.is_dag_ancestor_of(self.root, hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        unreachable!("not expected to be called in this context")
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
                    for child in transient_relations_stores[level].get_children(current).unwrap().read().iter().copied() {
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

    /// Given a current hash, count the blocks in its future.
    ///
    /// The algorithm works as follows:
    /// 1. Identify the dominant child (the one with the largest future) to minimize traversal,
    ///    since most of the future is expected to be covered by it.
    /// 2. Perform a BFS over all other children and their futures, skipping blocks that are
    ///    already in the future of the dominant child.
    ///
    /// This is conceptually similar to mergeset calculation logic
    /// (effectively a traversal over the reversed mergeset).
    ///
    /// Assumes `future_sizes` is populated for all children of `current` (caller is expected to be doing a topological BFS).
    fn count_future_size(&self, relations: &DbRelationsStore, current: Hash, future_sizes: &BlockHashMap<u64>) -> u64 {
        // Seed the BFS queue with all children of the current hash
        let mut queue: VecDeque<_> = relations.get_children(current).unwrap().read().iter().copied().collect();
        let mut visited = BlockHashSet::new();

        struct Entry {
            child: Hash,
            fut_size: u64,
        }

        // Future sizes are guaranteed to exist due to the topological BFS invariant
        let dominant_entry = queue
            .iter()
            .copied()
            .map(|child| Entry { child, fut_size: *future_sizes.get(&child).expect("topological bfs") })
            .max_by_key(|e| e.fut_size);

        let mut count = 0;

        if let Some(Entry { child: dominant_child, fut_size }) = dominant_entry {
            // Fully account for the dominant child future (+1 for itself) and exclude it from the traversal
            count += fut_size + 1;
            visited.insert(dominant_child);

            while let Some(hash) = queue.pop_front() {
                if !visited.insert(hash) {
                    continue;
                }

                // Skip blocks that are already in the future of the dominant child
                if self.reachability_service.is_dag_ancestor_of(dominant_child, hash) {
                    continue;
                }

                count += 1;
                for &child in relations.get_children(hash).unwrap().read().iter() {
                    queue.push_back(child);
                }
            }
        }

        trace!("Counted future size of {} as {}", current, count);
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
            self.approx_selected_parent_header_at_level(&pp_header.header, level)?.hash
        };

        let tip_header_bs = self.headers_store.get_blue_score(selected_tip).unwrap();
        let mut required_future_size = 2 * self.pruning_proof_m;
        let required_base_level_depth = (self.pruning_proof_m as f64 * 2.1) as u64; // =2100 for M=1000
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

        while let Some(SortableBlock { hash: current, .. }) = queue.pop() {
            if !visited.insert(current) {
                continue;
            }

            let header = self.headers_store.get_header(current).unwrap();
            let parents: BlockHashes = self.reachable_parents_at_level(level, &header).collect::<Vec<_>>().into();

            // A level may not contain enough headers to satisfy the safety margin.
            // This is intended to give the last header a chance, since it may still be deep enough
            // for a level proof.
            let is_last_header = parents.is_empty() && queue.is_empty();

            // Write parents to the relations store
            level_relation_store.insert(current, parents.clone()).unwrap();

            trace!("Level: {} | Counting future size of {}", level, current);
            let future_size = self.count_future_size(&level_relation_store, current, &future_sizes_map);
            future_sizes_map.insert(current, future_size);
            trace!("Level: {} | Hash: {} | Future Size: {}", level, current, future_size);

            let base_level_depth = tip_header_bs.saturating_sub(header.blue_score);

            // If the current hash is valid root candidate, fill the GD store and see if it passes as a level proof
            // Valid root candidates are:
            // 1. The genesis block
            // 2. The last header in the headers store
            // 3. Any known block that is in the selected chain from tip, has sufficient future size and depth
            if self.reachability_service.is_dag_ancestor_of(current, block_at_depth_m_at_next_level)
                && (current == self.genesis_hash
                    || is_last_header
                    || (future_size >= required_future_size && base_level_depth >= required_base_level_depth))
            {
                let root = current;
                let ghostdag_store = Arc::new(DbGhostdagStore::new_temp(temp_db.clone(), level, cache_policy, cache_policy, gd_tries));
                let has_required_block = self.populate_level_proof_ghostdag_data(
                    &level_relation_store,
                    &ghostdag_store,
                    root,
                    selected_tip,
                    block_at_depth_m_at_next_level,
                    level,
                    self.ghostdag_k,
                );
                assert!(has_required_block, "we verified that current ∈ past(required)");

                // Step 4 - Check if we actually have enough depth.
                // Need to ensure this does the same 2M+1 depth that block_at_depth does
                let curr_tip_bs = ghostdag_store.get_blue_score(selected_tip).unwrap();

                // Log all non-trivial cases
                if selected_tip != self.genesis_hash {
                    debug!("level: {}, future size: {}, blue_score: {}, retries: {}", level, future_size, curr_tip_bs, gd_tries);
                }

                if root == self.genesis_hash || curr_tip_bs >= 2 * self.pruning_proof_m {
                    return Ok((ghostdag_store, level_relation_store.into(), selected_tip, root));
                }

                // Large enough future with less than 2M blues means we have reds and thus need a gradual future size increase
                required_future_size = (required_future_size as f64 * 1.1) as u64;
                gd_tries += 1;
            }

            // Enqueue parents to fill full upper chain
            for &p in parents.iter() {
                queue.push(SortableBlock { hash: p, blue_work: self.headers_store.get_header(p).unwrap().blue_work });
            }
        }

        panic!("Failed to find sufficient root for level {level} after exhausting all known headers.");
    }

    /// Forward-traverses from `root` toward `tip`, and inserts Ghostdag data for each visited block.
    ///
    /// Traversal is restricted to `future(root) ∩ past(tip)` (i.e., blocks in the antipast of `tip` are ignored).
    /// Returns `true` iff `required_block` was encountered during traversal.
    fn populate_level_proof_ghostdag_data(
        &self,
        level_relations_store: &DbRelationsStore,
        ghostdag_store: &Arc<DbGhostdagStore>,
        root: Hash,
        tip: Hash,
        required_block: Hash,
        level: BlockLevel,
        ghostdag_k: KType,
    ) -> bool {
        // Restrict relations to `future(root)`
        let relations_view = FutureConeRelations::new(level_relations_store, self.reachability_service.clone(), root);

        // Create a Ghostdag manager over the restricted relations view
        let ghostdag_manager = GhostdagManager::with_level(
            root,
            ghostdag_k,
            ghostdag_store.clone(),
            &relations_view,
            self.headers_store.clone(),
            self.reachability_service.clone(),
            level,
            self.max_block_level,
        );

        // No need to initialize origin since we have a single root
        ghostdag_store.insert(root, Arc::new(ghostdag_manager.genesis_ghostdag_data())).unwrap();

        // Bottom-up topological traversal from `root` toward `tip`
        let mut queue: BinaryHeap<_> = Default::default();
        let mut visited = BlockHashSet::new();
        for child in relations_view.get_children(root).unwrap().read().iter().copied() {
            queue.push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
        }

        let mut has_required_block = root == required_block;

        while let Some(Reverse(SortableBlock { hash: current, .. })) = queue.pop() {
            if !visited.insert(current) {
                continue;
            }

            // We only care about `future(root) ∩ past(tip)`
            if !self.reachability_service.is_dag_ancestor_of(current, tip) {
                continue;
            }

            has_required_block |= current == required_block;

            ghostdag_store
                .insert(current, Arc::new(ghostdag_manager.ghostdag(&relations_view.get_parents(current).unwrap())))
                .unwrap();

            for child in relations_view.get_children(current).unwrap().read().iter().copied() {
                queue.push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
            }
        }

        // Returned for sanity testing by the caller
        has_required_block
    }

    /// Returns the header's parents at `level` that are reachable according to the reachability service,
    /// i.e., parents for which reachability data exists in the database.
    ///
    /// This function enforces the reachability / storage invariants described in the
    /// [crate-level documentation](crate): only parents with reachability data are returned.
    /// By convention, the returned hashes are therefore also guaranteed to have a header
    /// entry in the database.
    fn reachable_parents_at_level<'a>(&'a self, level: u8, header: &'a Header) -> impl Iterator<Item = Hash> + 'a {
        // `parents_at_level` may include candidates that are not currently in the database.
        // This is fine here: we only need *some* sufficiently-deep reachable root for a proof at this level,
        // not necessarily the "best" / most complete set of candidates.
        self.parents_manager
            .parents_at_level(header, level)
            .iter()
            .copied()
            // Filtering by header existence alone is not enough: we may store headers of past pruning points,
            // but those are not part of the reachable DAG for proof purposes.
            .filter(|&p| self.reachability_service.has_reachability_data(p))
    }

    /// Approximates the selected parent at `level` as the reachable parent whose header has the highest `blue_work`.
    fn approx_selected_parent_header_at_level(&self, header: &Header, level: BlockLevel) -> PpmInternalResult<Arc<Header>> {
        self.reachable_parents_at_level(level, header)
            .map(|p| self.headers_store.get_header(p).expect("reachable"))
            .max_by_key(|h| SortableBlock::new(h.hash, h.blue_work))
            .ok_or_else(|| PpmInternalError::NotEnoughHeadersToBuildProof("no reachable parents".to_string()))
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
