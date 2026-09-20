use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    ops::DerefMut,
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher, KType,
    blockhash::{BlockHashes, ORIGIN},
    header::Header,
    pruning::PruningPointProof,
};
use kaspa_core::{debug, trace};
use kaspa_database::prelude::*;
use kaspa_hashes::Hash;
use kaspa_utils::binary_heap::TopK;
use parking_lot::RwLock;

use crate::{
    model::{
        services::reachability::ReachabilityService,
        stores::{
            ghostdag::{DbGhostdagStore, GhostdagStore, GhostdagStoreReader},
            headers::{HeaderStoreReader, HeaderWithBlockLevel},
            pruning::{PruningProofDescriptor, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader},
            relations::{DbRelationsStore, RelationsStoreReader},
        },
    },
    processes::{
        ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
        pruning_proof::{GhostdagReaderExt, ProofInternalError},
        reachability::inquirer as reachability,
        relations::RelationsStoreExtensions,
    },
};

use super::{ProofInternalResult, PruningProofManager};
use crate::model::services::reachability::MTReachabilityService;

#[derive(Clone)]
struct LevelProofContext {
    ghostdag_store: Arc<DbGhostdagStore>,
    tip: Hash,
    root: Hash,
    count: u64,
}

/// A relations-store reader restricted to the future of a fixed root block (including the root).
///
/// Only parents and children that lie within the root’s future are exposed.
/// This provides a consistent, root-relative view of relations when operating on
/// proofs or subgraphs confined to that region of the DAG.
#[derive(Clone)]
struct FutureIntersectRelations<T: RelationsStoreReader, U: ReachabilityService> {
    relations_store: T,
    reachability_service: U,
    root: Hash,
}

impl<T: RelationsStoreReader, U: ReachabilityService> FutureIntersectRelations<T, U> {
    fn new(relations_store: T, reachability_service: U, root: Hash) -> Self {
        Self { relations_store, reachability_service, root }
    }
}

impl<T: RelationsStoreReader, U: ReachabilityService> RelationsStoreReader for FutureIntersectRelations<T, U> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.relations_store.get_parents(hash).map(|hashes| {
            hashes
                .iter()
                .copied()
                .filter(|&h| self.reachability_service.try_is_dag_ancestor_of(self.root, h).optional().unwrap().unwrap_or(false))
                .collect_vec()
                .into()
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

/// Utility for creating retry-indexed temporary ghostdag stores.
///
/// Each call to `new_store` returns a fresh temporary `DbGhostdagStore` for the
/// given level, using an incrementing retry index to avoid namespace collisions.
/// This is used when multiple ghostdag attempts may be required for the same level.
struct GhostdagStoreFactory {
    db: Arc<DB>,
    cache_policy: CachePolicy,
    level: BlockLevel,
    retries: u8,
}

impl GhostdagStoreFactory {
    fn new(db: Arc<DB>, cache_policy: CachePolicy, level: BlockLevel) -> Self {
        Self { db, cache_policy, level, retries: 0 }
    }

    /// Creates a fresh temporary ghostdag store for the next retry attempt
    fn new_store(&mut self) -> Arc<DbGhostdagStore> {
        self.retries += 1;
        Arc::new(DbGhostdagStore::new_temp(self.db.clone(), self.level, self.cache_policy, self.cache_policy, self.retries - 1))
    }
}

/// Utility for creating retry-indexed temporary reachability stores.
struct ReachabilityStoreFactory {
    db: Arc<DB>,
    cache_policy: CachePolicy,
    level: BlockLevel,
    retries: u8,
}

impl ReachabilityStoreFactory {
    fn new(db: Arc<DB>, cache_policy: CachePolicy, level: BlockLevel) -> Self {
        Self { db, cache_policy, level, retries: 0 }
    }

    fn new_store(&mut self) -> Arc<RwLock<DbReachabilityStore>> {
        self.retries += 1;
        Arc::new(RwLock::new(DbReachabilityStore::with_block_level_retry(
            self.db.clone(),
            self.cache_policy,
            self.cache_policy,
            self.level,
            self.retries - 1,
        )))
    }
}

impl PruningProofManager {
    /// Builds a pruning-point proof for `pp` by computing per-level MLS proof contexts and
    /// collecting the headers in `future(root) ∩ past(tip)` for each level.
    /// Temporary stores are used during construction, and headers are shared (via arcs)
    /// across levels in the final proof.
    pub(crate) fn build_pruning_point_proof(&self, pp: Hash) -> PruningPointProof {
        let descriptor = self.pruning_point_store.read().pruning_proof_descriptor().optional().unwrap();
        if let Some(descriptor) = descriptor.as_ref() {
            // Use a locally built descriptor (when it matches the current pruning point) for fast reconstruction.
            // Otherwise, recalculate the descriptor to optimize proof size.
            if descriptor.pruning_point == pp && !descriptor.external {
                return self.proof_from_descriptor(descriptor);
            }
        }

        let new_descriptor = match pp == self.genesis_hash {
            true => {
                // Genesis case - create a proof where all levels hold genesis only
                let (tips, roots, counts) = (0..=self.max_block_level).map(|_| (self.genesis_hash, self.genesis_hash, 1)).multiunzip();
                PruningProofDescriptor::new(self.genesis_hash, tips, roots, counts)
            }
            false => {
                // General case
                self.calc_new_proof(pp, descriptor.as_deref())
            }
        };

        let proof = self.proof_from_descriptor(&new_descriptor);
        self.pruning_point_store.write().set_pruning_proof_descriptor(new_descriptor).unwrap();

        proof
    }

    /// Reconstructs the pruning proof described by `descriptor` by loading headers from storage
    /// and collecting, per level, the blocks in `future(root) ∩ past(tip)`.
    ///
    /// Uses a local header-arc cache to deduplicate headers shared across levels.
    fn proof_from_descriptor(&self, descriptor: &PruningProofDescriptor) -> PruningPointProof {
        // The pruning proof can contain many duplicate headers (across levels), so we use a local cache in order
        // to make sure we hold a single Arc per header
        let mut cache: BlockHashMap<Arc<Header>> = BlockHashMap::with_capacity(4 * self.pruning_proof_m as usize);
        let mut get_header = |hash| cache.entry(hash).or_insert_with_key(|&hash| self.headers_store.get_header(hash).unwrap()).clone();

        let (_db_lifetime, temp_db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);

        (0..=self.max_block_level)
            .map(|level| {
                let level_idx = level as usize;
                let tip = descriptor.tips[level_idx];
                let root = descriptor.roots[level_idx];
                let expected_count = descriptor.counts[level_idx];

                let mut headers = VecDeque::with_capacity(2 * self.pruning_proof_m as usize);
                let mut relations_store = DbRelationsStore::new_temp(temp_db.clone(), level, 0, cache_policy, cache_policy);

                let mut queue = BinaryHeap::<SortableBlock>::new();
                let mut visited = BlockHashSet::new();
                queue.push(SortableBlock::new(tip, get_header(tip).blue_work));

                while let Some(SortableBlock { hash: current, .. }) = queue.pop() {
                    if !visited.insert(current) {
                        continue;
                    }

                    // We are only interested in the exact diamond future(root) ⋂ past(tip)
                    if !self.reachability_service.is_dag_ancestor_of(root, current) {
                        continue;
                    }

                    let header = get_header(current);
                    let parents: BlockHashes = self.reachable_parents_at_level(level, &header).collect::<Vec<_>>().into();
                    for parent in parents.iter().copied() {
                        queue.push(SortableBlock::new(parent, get_header(parent).blue_work));
                    }
                    relations_store.insert(current, parents).unwrap();
                }

                // Bottom-up traversal from root using the relations collected above
                let mut bottom_up_queue: BinaryHeap<_> = Default::default();
                let mut bottom_up_visited = BlockHashSet::new();
                bottom_up_queue.push(Reverse(SortableBlock::new(root, get_header(root).blue_work)));

                while let Some(Reverse(SortableBlock { hash: current, .. })) = bottom_up_queue.pop() {
                    if !bottom_up_visited.insert(current) {
                        continue;
                    }

                    if !self.reachability_service.is_dag_ancestor_of(current, tip) {
                        continue;
                    }

                    headers.push_back(get_header(current));

                    for &child in relations_store.get_children(current).unwrap().read().iter() {
                        bottom_up_queue.push(Reverse(SortableBlock::new(child, get_header(child).blue_work)));
                    }
                }

                assert_eq!(
                    expected_count,
                    headers.len() as u64,
                    "rebuilt proof level {} count {} does not match the expected descriptor count {}",
                    level,
                    headers.len(),
                    expected_count
                );
                headers.into()
            })
            .collect()
    }

    /// Computes level-proof contexts for all levels, processing levels from high to low to satisfy
    /// MLS inter-level constraints, and aggregates the results into a pruning-proof descriptor.
    fn calc_new_proof(&self, pp: Hash, prev_descriptor: Option<&PruningProofDescriptor>) -> PruningProofDescriptor {
        let (_db_lifetime, temp_db) = kaspa_database::create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let pp_header = self.headers_store.get_header_with_block_level(pp).unwrap();

        let mut level_proof_ctxs: Vec<Option<LevelProofContext>> = vec![None; (self.max_block_level + 1).into()];

        for level in (0..=self.max_block_level).rev() {
            let level_idx = level as usize;
            let required_block = if level != self.max_block_level {
                let LevelProofContext { ghostdag_store: next_level_gd_store, tip: next_level_tip, .. } =
                    level_proof_ctxs[level_idx + 1].as_ref().unwrap();

                let block_at_depth_m_at_next_level = next_level_gd_store
                    .block_at_depth(*next_level_tip, self.pruning_proof_m)
                    .map_err(|err| format!("next level: {}, err: {}", level + 1, err))
                    .unwrap();
                Some(block_at_depth_m_at_next_level)
            } else {
                None
            };
            level_proof_ctxs[level_idx] = Some(
                self.calc_level_proof_context(
                    &pp_header,
                    level,
                    required_block,
                    prev_descriptor.as_ref().map(|d| d.tips[level_idx]),
                    prev_descriptor.as_ref().map(|d| d.roots[level_idx]),
                    temp_db.clone(),
                )
                .unwrap_or_else(|e| panic!("calc_level_proof_context failed for level {level}: {e}")),
            );
        }

        let (tips, roots, counts) = level_proof_ctxs.into_iter().map(Option::unwrap).map(|l| (l.tip, l.root, l.count)).multiunzip();

        PruningProofDescriptor::new(pp, tips, roots, counts)
    }

    /// Computes a level-proof context by incrementally expanding the level relations subgraph and
    /// periodically attempting to anchor a proof between a candidate `root` and the selected `tip`.
    ///
    /// High-level flow:
    /// 1. Determine the selected `tip` at `level` (either the pruning point itself, or its approximate
    ///    selected parent at that level).
    /// 2. Traverse *backward* from the selected `tip` while populating a temporary relations store.
    ///    Traversal is performed in reverse-topological order so that all derived data
    ///    (e.g. future-size tracking, ghostdag inputs) is observed consistently.
    /// 3. Whenever the current block is a viable root candidate (sufficient base-level depth,
    ///    sufficient future size, and inclusion constraints), populate a temporary ghostdag store
    ///    for the region `future(root) ∩ past(tip)` and test whether it satisfies the
    ///    proof-level requirements.
    /// 4. If a candidate fails due to insufficient realized blue depth (due to reds),
    ///    increase the future-size threshold and continue searching further into the past.
    ///
    /// If `required_block` is provided, the chosen root must lie in its past.
    /// Typically, this block is the one at depth `M` from the *next* level, as mandated by the
    /// MLS (mining-in-log-space) protocol. Since level proofs are computed from higher levels
    /// to lower ones, the caller already has this block available and supplies it here to
    /// constrain root selection and ensure inter-level consistency.
    fn calc_level_proof_context(
        &self,
        pp_header: &HeaderWithBlockLevel,
        level: BlockLevel,
        required_block: Option<Hash>,
        prev_tip: Option<Hash>,
        prev_root: Option<Hash>,
        db: Arc<DB>,
    ) -> ProofInternalResult<LevelProofContext> {
        // Select the tip at this level:
        // - If the pruning point level >= level, use it.
        // - Otherwise, use the approximate selected parent at level.
        let tip = if pp_header.block_level >= level {
            pp_header.header.hash
        } else {
            // When advancing from a previous descriptor, require `prev_tip` to lie in the past of the new selected tip.
            // This preserves monotonicity across successive proofs (see `prev_root` rationale below).
            //
            // Note: such a parent always exists because the new pruning point is in the future of previous pruning points.
            self.reachable_parents_at_level(level, &pp_header.header)
                .filter(|&p| prev_tip.is_none_or(|prev_tip| self.reachability_service.is_dag_ancestor_of(prev_tip, p)))
                .map(|p| self.headers_store.get_header(p).expect("reachable"))
                .max_by_key(|h| SortableBlock::new(h.hash, h.blue_work))
                .ok_or_else(|| ProofInternalError::NotEnoughHeadersToBuildProof("no reachable parents".to_string()))?
                .hash
        };

        // Base-level blue score of the selected tip, taken directly from the header.
        // This is distinct from the *locally computed* blue score later derived from
        // the temporary ghostdag instance at this level.
        let tip_header_score = self.headers_store.get_blue_score(tip).unwrap();

        // Proof thresholds:
        // - required_future_size gates root candidacy based on how much future mass a root covers.
        // - required_base_level_depth is a base-level (header) blue-depth guard: if level 0
        //   lacks sufficient blues here, we avoid premature ghostdag attempts.
        let mut required_future_size = 2 * self.pruning_proof_m;
        let required_base_level_depth = (self.pruning_proof_m as f64 * 2.1) as u64; // ~= 2100 for M=1000

        // If no explicit required block is provided, default to `tip`.
        // Typically, `required_block` is the block at depth `M` from the *next* level, per the MLS protocol
        let required = required_block.unwrap_or(tip);

        // Backward traversal from `tip` in reverse-topological order
        // to maintain consistency for all derived computations.
        let mut queue = BinaryHeap::<SortableBlock>::new();
        let mut visited = BlockHashSet::new();
        queue.push(SortableBlock { hash: tip, blue_work: self.headers_store.get_header(tip).unwrap().blue_work });

        let cache_policy = CachePolicy::Count(2 * self.pruning_proof_m as usize);

        // A single shared relations store is maintained for the entire search of this level.
        let mut relations_store = DbRelationsStore::new_temp(db.clone(), level, 0, cache_policy, cache_policy);

        // For each visited block, store the size of its (known) future up to `tip`.
        let mut future_sizes_map = BlockHashMap::<u64>::new();

        // Each ghostdag attempt uses a fresh temp store namespace (indexed internally by `retries`).
        let mut ghostdag_factory = GhostdagStoreFactory::new(db.clone(), cache_policy, level);
        let mut reachability_factory = ReachabilityStoreFactory::new(db.clone(), cache_policy, level);

        // Track a few high-future-size candidates for a final fallback pass
        let mut best_future_roots = TopK::<(u64, Hash), 8>::new();

        // Try to realize a level-proof from a candidate root
        let mut try_root = |relations_store: &DbRelationsStore, root: Hash, future_size: u64| -> Option<LevelProofContext> {
            // Populate ghostdag for `future(root) ∩ past(tip)` and test depth requirements.
            let (ghostdag_store, has_required_block, count) = self.populate_level_proof_ghostdag_data(
                relations_store,
                &mut ghostdag_factory,
                &mut reachability_factory,
                root,
                tip,
                required,
                level,
                self.ghostdag_k,
            );

            // Realized blue depth for this root, computed from the level-specific ghostdag
            let current_level_score = ghostdag_store.get_blue_score(tip).unwrap();

            // Log all non-trivial cases
            if tip != self.genesis_hash {
                debug!(
                    "level: {}, future: {}, blue score: {}, count: {}, retries: {}",
                    level, future_size, current_level_score, count, ghostdag_factory.retries
                );
            }

            // Success:
            // - Must include the required block (the block at depth M from the next level)
            // - If root is genesis, required-block inclusion is sufficient
            // - Otherwise require at least `2M` blue depth at this level
            if has_required_block && (root == self.genesis_hash || current_level_score >= 2 * self.pruning_proof_m) {
                Some(LevelProofContext { ghostdag_store, tip, root, count })
            } else {
                None
            }
        };

        while let Some(SortableBlock { hash: current, .. }) = queue.pop() {
            if !visited.insert(current) {
                continue;
            }

            if let Some(prev_root) = prev_root {
                // When advancing from a previous descriptor, use `prev_root` as a boundary for root selection.
                if !self.reachability_service.is_dag_ancestor_of(prev_root, current) {
                    continue;
                }
            }

            let header = self.headers_store.get_header(current).unwrap();

            // Collect reachable parents at this level
            let parents: BlockHashes = self.reachable_parents_at_level(level, &header).collect::<Vec<_>>().into();

            // Persist relations for `current`
            relations_store.insert(current, parents.clone()).unwrap();

            trace!("Level: {} | Counting future size of {}", level, current);
            let future_size = self.count_future_size(&relations_store, current, &future_sizes_map);
            future_sizes_map.insert(current, future_size);
            trace!("Level: {} | Hash: {} | Future Size: {}", level, current, future_size);

            // Base-level depth from `tip`, measured using *header* blue scores.
            let base_level_depth = tip_header_score.saturating_sub(header.blue_score);

            // Root candidacy conditions:
            // - Must be in the past of `required`
            // - And one of:
            //   (a) genesis
            //   (b) sufficiently large future and sufficiently deep base-level distance
            if self.reachability_service.is_dag_ancestor_of(current, required) {
                // If the root appears immediately viable, attempt ghostdag now.
                // A successful attempt requires ≥ 2M realized blues at this level.
                if current == self.genesis_hash
                    || (future_size >= required_future_size && base_level_depth >= required_base_level_depth)
                {
                    let root = current;
                    if let Some(level_ctx) = try_root(&relations_store, root, future_size) {
                        return Ok(level_ctx);
                    }

                    // Large enough future with insufficient blue depth implies reds; increase the
                    // future-size threshold and retry further in the past.
                    required_future_size = (required_future_size as f64 * 1.1) as u64;
                } else if future_size >= 2 * self.pruning_proof_m {
                    // Minimum precondition for reaching ≥ 2M blues is future_size ≥ 2M.
                    // Defer ghostdag and keep as a fallback candidate.
                    best_future_roots.push((future_size, current));
                }
            }

            // Continue expanding the backward traversal.
            for &p in parents.iter() {
                queue.push(SortableBlock { hash: p, blue_work: self.headers_store.get_header(p).unwrap().blue_work });
            }
        }

        // Use the previous proof's root as the fallback anchor when progressing proofs.
        // With a fixed root, ghostdag selection is deterministic, and if the new tip is in the future of the
        // previous tip then blue score/work can only increase — so once the 2M (or genesis) invariant holds,
        // it continues to hold for all future progressions.
        if let Some(root) = prev_root {
            let future_size = *future_sizes_map.get(&root).expect("exhausted traversal");
            if let Some(level_ctx) = try_root(&relations_store, root, future_size) {
                return Ok(level_ctx);
            }
        }

        // Final fallback: give a last chance to a few high-future-size roots.
        // This is only needed for migrating nodes without a stored descriptor yet, and can be removed
        // once all nodes persist descriptors (along with the whole top-k fallback path).
        for (future_size, root) in best_future_roots.into_sorted_iter_ascending().collect_vec().into_iter().rev() {
            if let Some(level_ctx) = try_root(&relations_store, root, future_size) {
                return Ok(level_ctx);
            }
        }

        panic!("Failed to find sufficient root for level {level} after exhausting all known headers.");
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

    /// Forward-traverses from `root` toward `tip`, and inserts ghostdag data for each visited block.
    ///
    /// Traversal is restricted to `future(root) ∩ past(tip)` (i.e., blocks in the antipast of `tip` are ignored).
    /// Returns `true` iff `required_block` was encountered during traversal.
    fn populate_level_proof_ghostdag_data(
        &self,
        relations_store: &DbRelationsStore,
        ghostdag_factory: &mut GhostdagStoreFactory,
        reachability_factory: &mut ReachabilityStoreFactory,
        root: Hash,
        tip: Hash,
        required_block: Hash,
        level: BlockLevel,
        ghostdag_k: KType,
    ) -> (Arc<DbGhostdagStore>, bool, u64) {
        debug!("Populating GD for root {} at level {} (retry {})", root, level, ghostdag_factory.retries.saturating_sub(1));

        let ghostdag_store = ghostdag_factory.new_store();
        let reachability_store = reachability_factory.new_store();
        let reachability_service = MTReachabilityService::new(reachability_store.clone());

        // Init reachability with ORIGIN and add root as its only child
        reachability::init(reachability_store.write().deref_mut()).unwrap();
        reachability::add_block(reachability_store.write().deref_mut(), root, ORIGIN, &mut [].into_iter()).unwrap();

        // Restrict relations to `future(root)` via level reachability
        let relations_view = FutureIntersectRelations::new(relations_store, reachability_service.clone(), root);

        // Create a ghostdag manager over the restricted relations view
        let ghostdag_manager = GhostdagManager::with_level(
            root,
            ghostdag_k,
            ghostdag_store.clone(),
            &relations_view,
            self.headers_store.clone(),
            reachability_service.clone(),
            level,
            self.max_block_level,
        );

        // No need to initialize origin since we have a single root
        ghostdag_store.insert(root, Arc::new(ghostdag_manager.genesis_ghostdag_data())).unwrap();

        // Bottom-up topological traversal from `root` toward `tip`
        let mut queue: BinaryHeap<_> = Default::default();
        let mut visited = BlockHashSet::new();
        let mut count = 1; // counting root
        for child in relations_view.get_children(root).unwrap().read().iter().copied() {
            queue.push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
        }

        let mut has_required_block = root == required_block;
        let mut reachability_tip = root;

        while let Some(Reverse(SortableBlock { hash: current, .. })) = queue.pop() {
            if !visited.insert(current) {
                continue;
            }

            // We only care about `future(root) ∩ past(tip)`
            if !self.reachability_service.is_dag_ancestor_of(current, tip) {
                continue;
            }

            has_required_block |= current == required_block;
            count += 1;

            let parents = relations_view.get_parents(current).unwrap();
            assert!(!parents.is_empty(), "non-root blocks must have parents");

            let ghostdag_data = Arc::new(ghostdag_manager.ghostdag(parents.as_slice()));
            ghostdag_store.insert(current, ghostdag_data.clone()).unwrap();

            reachability_tip = ghostdag_manager.find_selected_parent([reachability_tip, current]);

            let mut level_reachability = reachability_store.write();
            let mut reachability_mergeset = ghostdag_data
                .unordered_mergeset_without_selected_parent()
                .filter(|hash| level_reachability.has(*hash).unwrap())
                .collect_vec()
                .into_iter();

            reachability::add_block(
                level_reachability.deref_mut(),
                current,
                ghostdag_data.selected_parent,
                &mut reachability_mergeset,
            )
            .unwrap();

            if current == reachability_tip {
                reachability::hint_virtual_selected_parent(level_reachability.deref_mut(), current).unwrap();
            }
            drop(level_reachability);

            for child in relations_view.get_children(current).unwrap().read().iter().copied() {
                queue.push(Reverse(SortableBlock { hash: child, blue_work: self.headers_store.get_header(child).unwrap().blue_work }));
            }
        }

        // Returned for sanity testing by the caller
        (ghostdag_store, has_required_block, count)
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
}
