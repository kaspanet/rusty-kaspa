use std::collections::HashMap;
use std::{ops::Range, sync::Arc};

use kaspa_consensus_core::{BlueWorkType, KType};
use kaspa_core::debug;
use kaspa_database::prelude::StoreError;
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use num_traits::Zero;

use crate::model::services::reachability::{MTReachabilityService, ReachabilityService};
use crate::model::stores::headers::HeaderStoreReader;
use crate::model::stores::reachability::ReachabilityStoreReader;
use crate::processes::dagknight::umc_cascade_persistence::{
    ChainLeafEntry, Mergeset, UmcCascadeKey, UmcCascadePersistedState, UmcCascadeStore,
};
use crate::processes::dagknight::umc_voting::{CascadeResult, ColoringReader, SignedWork, UmcVoter, UmcVotingContext};
use crate::processes::dagknight::{AppendableSegmentTree, Bucket, bucket_for_score};
use crate::processes::difficulty::calc_work;

// ============================================================================
// Cascade Maintainer
// ============================================================================

/// Maintains exact cascade scores for one fixed k using chain decomposition
/// and lazy segment trees with event-driven bucket-transition propagation.
pub struct CascadeMaintainer {
    blues_chains_decomposition: Vec<Vec<Hash>>,
    chains_score_trees: Vec<AppendableSegmentTree<BlockWithWork, SignedWork>>,
    blk_mapping_to_chains: HashMap<Hash, usize>,
    deficit_work: BlueWorkType,
    blue_work: BlueWorkType,
    red_work: BlueWorkType,
    negative_blue_work: BlueWorkType,
    /// Total bucket flips observed during cascade stabilization
    flip_count: u64,
    bound_depth: u64,
    depth_limit_ancestor: Hash,
    next_chain_ancestor: Hash,
}

/// A block identifier paired with that block's own proof-of-work contribution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockWithWork {
    pub hash: Hash,
    pub work: BlueWorkType,
}

impl BlockWithWork {
    pub fn new(hash: Hash, work: BlueWorkType) -> Self {
        Self { hash, work }
    }
}

fn work_delta(work: BlueWorkType, bucket: Bucket) -> SignedWork {
    let magnitude = SignedWork::from(work);
    match bucket {
        Bucket::Positive => magnitude,
        Bucket::Negative => SignedWork::zero() - magnitude,
    }
}

impl CascadeMaintainer {
    /// Initializes the cascade with `floor(sqrt(k))` conflict-genesis work as its voting deficit.
    pub fn new(conflict_genesis: BlockWithWork, k: KType, next_chain_ancestor: Hash) -> Self {
        let deficit_work = conflict_genesis.work * u64::from(k.isqrt());
        Self {
            blues_chains_decomposition: Vec::new(),
            chains_score_trees: Vec::new(),
            blk_mapping_to_chains: HashMap::new(),
            deficit_work,
            blue_work: BlueWorkType::ZERO,
            red_work: BlueWorkType::ZERO,
            negative_blue_work: BlueWorkType::ZERO,
            flip_count: 0,
            bound_depth: u64::from(k).pow(4) + 1,
            depth_limit_ancestor: conflict_genesis.hash,
            next_chain_ancestor,
        }
    }

    /// Insert a new blue block into the chain decomposition
    fn add_blue(&mut self, block: BlockWithWork, reachability: &impl ReachabilityService, events: &mut Vec<(Hash, SignedWork)>) {
        let initial_score = SignedWork::from(self.deficit_work);
        let initial_bucket = bucket_for_score(initial_score);

        let chain_id = self.find_extendable_chain(block.hash, reachability).unwrap_or_else(|| {
            let id = self.blues_chains_decomposition.len();
            self.blues_chains_decomposition.push(Vec::new());
            self.chains_score_trees.push(AppendableSegmentTree::new());
            id
        });

        self.blue_work = self.blue_work + block.work;
        self.append_to_chain(chain_id, block, initial_score, initial_bucket);

        // A new blue block contributes according to its initial bucket.
        let initial_contribution = work_delta(block.work, initial_bucket);
        events.push((block.hash, initial_contribution));
    }

    /// Add a new red block and propagate its negative work to ancestor blues.
    fn add_red(&mut self, block: BlockWithWork, events: &mut Vec<(Hash, SignedWork)>) {
        self.red_work = self.red_work + block.work;
        events.push((block.hash, work_delta(block.work, Bucket::Negative)));
    }

    /// Returns the aggregate score of the virtual block.
    pub fn cascade_score(&self) -> SignedWork {
        SignedWork::from(self.blue_work) + SignedWork::from(self.deficit_work)
            - SignedWork::from(self.red_work)
            - SignedWork::from(self.negative_blue_work * 2)
    }

    /// Check if the virtual block's aggregate cascade score is non-negative.
    pub fn virtual_accepts(&self) -> bool {
        self.cascade_score() >= SignedWork::zero()
    }

    /// finds the first chain ancestor of the merging block, that is more than k^4 blue score away from its selected parent
    fn update_depth_limit_ancestor<C: ColoringReader + ?Sized>(
        &mut self,
        merger_selected_parent: Hash,
        merger_blue_score: u64,
        coloring_reader: &C,
        reachability: &impl ReachabilityService,
    ) {
        let mut curr_bound = self.depth_limit_ancestor;

        // The selected-parent path may have changed branches since the last
        // mergeset. Walk the old bound back until it reaches the new path; the
        // first ancestor shared by both paths is their lowest common point.
        while !reachability.is_chain_ancestor_of(curr_bound, merger_selected_parent) {
            curr_bound = reachability.get_chain_parent(curr_bound);
        }

        while curr_bound != merger_selected_parent {
            let next_bound = reachability.get_next_chain_ancestor(merger_selected_parent, curr_bound);
            let next_distance = merger_blue_score.saturating_sub(coloring_reader.get_coloring_data(next_bound).blue_score);
            if next_distance < self.bound_depth {
                break;
            }
            curr_bound = next_bound;
        }

        self.depth_limit_ancestor = curr_bound;
    }

    fn violates_depth_restriction(&mut self, reachability: &impl ReachabilityService) -> bool {
        for (chain, tree) in self.blues_chains_decomposition.iter().zip(self.chains_score_trees.iter_mut()) {
            let prefix_length = strict_ancestor_index(chain, self.depth_limit_ancestor, reachability).unwrap_or(0);
            if tree.has_negative_score_in_prefix(prefix_length) {
                return true;
            }
        }
        false
    }

    /// Returns the total number of bucket flips observed during cascade stabilization.
    pub fn flip_count(&self) -> u64 {
        self.flip_count
    }

    // ----- Persistence -----

    /// Serialize the current cascade state for checkpoint persistence.
    pub fn to_persisted_state(&mut self, voting_blocks: u64) -> UmcCascadePersistedState {
        let mut chains_leaves: Vec<Vec<ChainLeafEntry>> = Vec::new();

        for (chain_id, _chain) in self.blues_chains_decomposition.iter().enumerate() {
            let tree = &mut self.chains_score_trees[chain_id];
            let leaves = tree.leaves();
            let mut chain_leaves = Vec::new();
            for block_with_work in leaves {
                let score = tree.score(block_with_work);
                let is_negative = score.negative();
                let abs_score: Uint192 = score.abs();
                let bucket_positive = tree.bucket(block_with_work) == Bucket::Positive;
                chain_leaves.push(ChainLeafEntry {
                    hash: block_with_work.hash,
                    work: block_with_work.work,
                    score_abs: abs_score,
                    score_negative: is_negative,
                    bucket_positive,
                });
            }
            chains_leaves.push(chain_leaves);
        }

        UmcCascadePersistedState {
            blues_chains_decomposition: self.blues_chains_decomposition.clone(),
            chains_leaves,
            blk_mapping_to_chains: self.blk_mapping_to_chains.clone(),
            depth_limit_ancestor: self.depth_limit_ancestor,
            deficit_work: self.deficit_work,
            blue_work: self.blue_work,
            red_work: self.red_work,
            negative_blue_work: self.negative_blue_work,
            voting_blocks,
            flip_count: self.flip_count,
        }
    }

    /// Restore cascade state from a persisted checkpoint.
    pub fn from_persisted_state(
        persisted: &UmcCascadePersistedState,
        conflict_genesis: BlockWithWork,
        k: KType,
        next_chain_ancestor: Hash,
    ) -> Self {
        let mut maintainer = Self::new(conflict_genesis, k, next_chain_ancestor);

        // Override counters from persisted state
        maintainer.deficit_work = persisted.deficit_work;
        maintainer.blue_work = persisted.blue_work;
        maintainer.red_work = persisted.red_work;
        maintainer.negative_blue_work = persisted.negative_blue_work;
        maintainer.flip_count = persisted.flip_count;
        maintainer.depth_limit_ancestor = persisted.depth_limit_ancestor;

        // Restore chains and trees
        maintainer.blues_chains_decomposition = persisted.blues_chains_decomposition.clone();
        maintainer.blk_mapping_to_chains = persisted.blk_mapping_to_chains.clone();

        for (chain_id, _chain) in maintainer.blues_chains_decomposition.iter().enumerate() {
            let leaves = &persisted.chains_leaves[chain_id];

            // Rebuild tree from checkpoint
            let mut temp_tree: AppendableSegmentTree<BlockWithWork, SignedWork> = AppendableSegmentTree::new();
            for leaf_entry in leaves {
                let block = BlockWithWork::new(leaf_entry.hash, leaf_entry.work);
                let score: SignedWork = if leaf_entry.score_negative {
                    SignedWork::zero() - SignedWork::from(leaf_entry.score_abs)
                } else {
                    SignedWork::from(leaf_entry.score_abs)
                };
                let bucket = if leaf_entry.bucket_positive { Bucket::Positive } else { Bucket::Negative };
                temp_tree.append_leaf_with_bucket(block, score, bucket);
            }

            maintainer.chains_score_trees.push(temp_tree);
        }

        maintainer
    }

    // ----- Chain operations -----

    fn find_extendable_chain(&self, block: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
        for (chain_id, chain) in self.blues_chains_decomposition.iter().enumerate() {
            if let Some(&head) = chain.last()
                && reachability.is_dag_ancestor_of(head, block)
            {
                return Some(chain_id);
            }
        }
        None
    }

    fn append_to_chain(&mut self, chain_id: usize, block: BlockWithWork, initial_score: SignedWork, initial_bucket: Bucket) {
        self.blues_chains_decomposition[chain_id].push(block.hash);
        self.chains_score_trees[chain_id].append_leaf(block, initial_score);
        self.blk_mapping_to_chains.insert(block.hash, chain_id);

        if initial_bucket == Bucket::Negative {
            self.negative_blue_work = self.negative_blue_work + block.work;
        }
    }

    // ----- Event processing -----

    /// # Amortized complexity
    ///
    /// Let the `c = O(k^2)` chains contain `n_1, ..., n_c` blocks, with
    /// `sum(n_i) = n`. Propagating one event performs one logarithmic prefix
    /// update on every affected chain. By concavity of the logarithm, its
    /// worst-case cost is
    /// `O(sum_i log(1 + n_i)) = O(c log(1 + n/c))`, hence
    /// `O(k^2 log(1 + n/k^2))` (usually written `O(k^2 log(n/k^2))`).
    ///
    /// The depth restriction gives an `O(k^4)` worst-case bound on negative
    /// flips in one mergeset round, but the same bound does not hold pointwise
    /// for positive flips: one round may restore arbitrarily many old negative
    /// blocks. These flips are nevertheless bounded amortized over the (linear) lifetime
    /// of the maintainer:
    ///
    /// 1. During one mergeset round, a block inside the depth boundary flips
    ///    at most once to negative and at most once to positive. The
    ///    `O(k^4)` bound on negative flips therefore also bounds all flips
    ///    inside the boundary by `O(k^4)` for that round.
    /// 2. A positive flip strictly beyond the boundary can occur at most once.
    ///    If that block later obtains a negative score, `violates_depth_restriction`
    ///    stops processing before `process_negative_phase` changes its bucket
    ///    back to negative. Its score may subsequently change, but it cannot
    ///    cause positive flip processing.
    ///
    /// Thus each round has `O(k^4)` flips inside the boundary, plus at most one
    /// beyond-boundary positive flip per block over the complete history
    /// (including history restored from a checkpoint). Since a mergeset adds at
    /// most `O(k^2)` blocks, those one-time flips contribute only `O(k^2)` per
    /// round amortized. The total is therefore `O(k^4)` flip events per round
    /// amortized. Multiplying by the cost of propagating an event gives
    /// `O(k^6 log(n / k^2))` amortized time per mergeset round. Direct events
    /// from the `O(k^2)` newly processed blocks are lower order. This accounting
    /// intentionally does not use the range-update batching optimization,
    /// which provides de facto speedup, but is hard to analyze.
    fn process_positive_phase(&mut self, reachability: &impl ReachabilityService, events: &mut Vec<(Hash, SignedWork)>) {
        while !events.is_empty() {
            self.apply_events_batch(events.drain(..), reachability);

            for chain_id in 0..self.chains_score_trees.len() {
                let crossing_blocks = self.chains_score_trees[chain_id].extract_negative_at_least_zero_batch();
                for block in crossing_blocks {
                    self.chains_score_trees[chain_id].flip_to_positive(block);
                    self.negative_blue_work = self.negative_blue_work - block.work;
                    self.flip_count += 1;
                    events.push((block.hash, work_delta(block.work * 2u64, Bucket::Positive)));
                }
            }
        }
    }

    /// Processes queued effects and settles negative bucket crossings.
    ///
    /// The depth restriction bounds the number of negative flips processed here.
    /// The blue future of the bound contains at most `k^4 + k^2` blocks (the
    /// selected-chain interval up to the merger and its mergeset blues), while
    /// the blue anticone of the bound contributes at most another `k^2` blocks.
    /// Hence at most `(k^4 + k^2) + k^2` negative flips are processed.
    fn process_negative_phase(&mut self, reachability: &impl ReachabilityService, events: &mut Vec<(Hash, SignedWork)>) {
        debug_assert!(events.iter().all(|(_, delta)| *delta < SignedWork::zero()), "red processing must start with negative events");

        loop {
            self.apply_events_batch(events.drain(..), reachability);

            if self.violates_depth_restriction(reachability) {
                return;
            }

            for chain_id in 0..self.chains_score_trees.len() {
                let crossing_blocks = self.chains_score_trees[chain_id].extract_positive_below_zero_batch();
                for block in crossing_blocks {
                    self.chains_score_trees[chain_id].flip_to_negative(block);
                    self.negative_blue_work = self.negative_blue_work + block.work;
                    self.flip_count += 1;
                    events.push((block.hash, work_delta(block.work * 2u64, Bucket::Negative)));
                }
            }

            if events.is_empty() {
                break;
            }

            // Every queued event belongs to a flip already committed in this
            // round, so propagate the complete batch before checking again.
        }
    }

    fn apply_events_batch<I>(&mut self, events: I, reachability: &impl ReachabilityService)
    where
        I: IntoIterator<Item = (Hash, SignedWork)>,
    {
        let mut updates: Vec<Vec<(Range<usize>, SignedWork)>> = (0..self.chains_score_trees.len()).map(|_| Vec::new()).collect();
        for (source, delta) in events {
            for (chain_id, chain) in self.blues_chains_decomposition.iter().enumerate() {
                if let Some(ancestor_index) = strict_ancestor_index(chain, source, reachability) {
                    updates[chain_id].push((0..ancestor_index, delta));
                }
            }
        }
        for (tree, chain_updates) in self.chains_score_trees.iter_mut().zip(updates) {
            tree.range_add_batch(&chain_updates);
        }
    }

    /// Serialize checkpoint state at the given chain block for persistence.
    /// and save it to the persistence store
    pub fn save_state(
        &mut self,
        key: UmcCascadeKey,
        voting_blocks: u64,
        cascade_store: Arc<dyn UmcCascadeStore>,
    ) -> Result<(), StoreError> {
        let persisted_state = self.to_persisted_state(voting_blocks);
        cascade_store.insert_checkpoint(key, persisted_state)
    }
}

/// Returns the exclusive end index of the strict-ancestor prefix, or `None` if it is empty.
fn strict_ancestor_index(chain: &[Hash], source: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = chain.len();

    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let v_mid = chain[mid - 1];
        if reachability.is_dag_ancestor_of(v_mid, source) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }

    if lo == 0 {
        return None;
    }

    // `lo` is exclusive, so `lo - 1` is the last inclusive ancestor.
    // Reachability includes `source` itself; strict ancestry excludes it.
    if chain[lo - 1] == source {
        lo = lo.saturating_sub(1);
    }

    if lo == 0 {
        return None;
    }

    Some(lo)
}

// ============================================================================
// Checkpoint-Based Cascade Runner
// ============================================================================

/// Run cascade voting on mergesets from the virtual GD chain.
/// Each block's events propagate backward to already-processed strict ancestors.
///
/// `mergeset_stack` is ordered Virtual-first (top-down), so pop() gives CG first.
///
/// Within each mergeset, blocks are already in topological order:
/// - mergeset_blues[i] < mergeset_blues[i+1]
/// - mergeset_reds[i] < mergeset_reds[i+1]
/// - All blues are earlier than all reds
///
/// `next_chain_ancestor` is used to filter grays: a red block for which
/// `next_chain_ancestor` is a chain ancestor is a gray and is skipped.
///
/// `checkpoint_state` is the persisted cascade state loaded by the caller.
/// When `Some`, the cascade reloads from this state and processes only the
/// remaining mergesets above the checkpoint. When `None`, starts from scratch.
///
/// `from_checkpoint` indicates whether we started from a checkpoint (caller's responsibility).
/// `estimated_effort_saved` is the estimated number of blue blocks skipped by checkpointing
/// (caller's responsibility to calculate from virtual_gd.blue_score - checkpoint_blue_score).
/// `estimated_effort_total` is virtual_gd.blue_score (total blues in the conflict zone).
pub fn run_cascade<C: ColoringReader + ?Sized>(
    mut mergeset_stack: Vec<Mergeset>,
    conflict_genesis: BlockWithWork,
    k: KType,
    next_chain_ancestor: Hash,
    reachability: &impl ReachabilityService,
    cascade_store: Arc<dyn UmcCascadeStore>,
    checkpoint_state: Option<UmcCascadePersistedState>,
    estimated_effort_saved: u64,
    estimated_effort_total: u64,
    coloring_reader: &C,
) -> CascadeResult {
    let mut voting_blocks = 0u64;
    let from_checkpoint = checkpoint_state.is_some();

    // Restore from checkpoint or start fresh
    let mut maintainer = if let Some(persisted) = checkpoint_state {
        voting_blocks = persisted.voting_blocks;
        CascadeMaintainer::from_persisted_state(&persisted, conflict_genesis, k, next_chain_ancestor)
    } else {
        CascadeMaintainer::new(conflict_genesis, k, next_chain_ancestor)
    };

    // Process remaining mergesets (pop from bottom = CG-first, upward)
    while let Some(mergeset) = mergeset_stack.pop() {
        let checkpoint_key = (!mergeset_stack.is_empty())
            .then(|| UmcCascadeKey::new(conflict_genesis.hash, k, next_chain_ancestor, mergeset.checkpoint_hash()));
        let processed_blocks = process_mergeset(&mut maintainer, mergeset, reachability, coloring_reader);
        voting_blocks += processed_blocks;

        // Checkpoint at chain block — persist to store (best-effort)
        if let Some(checkpoint_key) = checkpoint_key {
            let _ = maintainer.save_state(checkpoint_key, voting_blocks, cascade_store.clone());
        }
    }

    let cascade_score = maintainer.cascade_score();
    // TODO: Create a rigorous document explaining why this restriction maintains dagknight's security.
    let accepted = !maintainer.violates_depth_restriction(reachability) && maintainer.virtual_accepts();

    CascadeResult {
        cascade_score,
        accepted,
        flips: maintainer.flip_count,
        voting_blocks,
        from_checkpoint,
        estimated_effort_saved,
        estimated_effort_total,
    }
}

/// Process one mergeset in protocol order.
///
/// All blue and red effects are queued before stabilization starts. Applying the
/// positive phase  first and consuming any potential positive flip,  allows us to be
/// certain that if a negative is found beyond the depth restriction later on,
/// than it is final for this mergeset and the run can be safely stopped.
fn process_mergeset<C: ColoringReader + ?Sized>(
    maintainer: &mut CascadeMaintainer,
    mergeset: Mergeset,
    reachability: &impl ReachabilityService,
    coloring_reader: &C,
) -> u64 {
    let selected_parent = mergeset.selected_parent;
    let merger_blue_score = coloring_reader.get_coloring_data(selected_parent).blue_score + mergeset.mergeset_blues.len() as u64;

    // For the virtual mergeset, the bound uses the virtual blue score and its
    // concrete selected parent as the chain endpoint.
    maintainer.update_depth_limit_ancestor(selected_parent, merger_blue_score, coloring_reader, reachability);

    let mut events = Vec::new();
    let mut voting_blocks = 0u64;

    for (hash, work) in mergeset.mergeset_blues {
        maintainer.add_blue(BlockWithWork::new(hash, work), reachability, &mut events);
        voting_blocks += 1;
    }
    for (hash, work) in mergeset.mergeset_reds {
        if !reachability.is_chain_ancestor_of(maintainer.next_chain_ancestor, hash) {
            maintainer.add_red(BlockWithWork::new(hash, work), &mut events);
            voting_blocks += 1;
        }
    }

    maintainer.process_positive_phase(reachability, &mut events);
    maintainer.process_negative_phase(reachability, &mut events);
    voting_blocks
}

// Segment Tree UMC Voter
// ============================================================================

/// UMC cascade voter using chain-based segment trees.
///
/// Dependencies: headers for proof-of-work, reachability for the merging-chain walk and
/// gray filtering, and the UMC cascade checkpoint store.
pub struct SegmentTreeUmcVoter<
    O: HeaderStoreReader + 'static,
    E: UmcCascadeStore + Clone + 'static,
    R: ReachabilityStoreReader + Clone,
> {
    headers_store: Arc<O>,
    reachability_service: MTReachabilityService<R>,
    umc_persistence_store: Arc<E>,
}

impl<O: HeaderStoreReader + 'static, E: UmcCascadeStore + Clone + 'static, R: ReachabilityStoreReader + Clone>
    SegmentTreeUmcVoter<O, E, R>
{
    pub fn new(headers_store: Arc<O>, umc_persistence_store: Arc<E>, reachability_service: MTReachabilityService<R>) -> Self {
        Self { headers_store, umc_persistence_store, reachability_service }
    }
}

impl<O: HeaderStoreReader + 'static, E: UmcCascadeStore + Clone + 'static, R: ReachabilityStoreReader + Clone> UmcVoter
    for SegmentTreeUmcVoter<O, E, R>
{
    /// UMC Cascade Voting using chain-based segment tree
    ///
    /// inputs: G, U, d
    /// output: does U have a subset U' s.t. U' is d-UMC of G
    ///         where d-UMC means that each block in U' is majority covered by U' (up to d)
    fn vote(&self, ctx: &UmcVotingContext<'_>) -> CascadeResult {
        let conflict_genesis = ctx.conflict_genesis;
        let subgroup = ctx.subgroup;
        let virtual_gd = ctx.virtual_gd;
        let k = ctx.k;
        let coloring_reader = ctx.coloring_reader;
        let next_chain_ancestor_of_subgroup = self.reachability_service.get_next_chain_ancestor(subgroup[0], conflict_genesis);

        // Collect blues and reds by traversing virtual GD chain backward.
        // Build mergesets into a stack: Virtual first, then ChainN, ..., Chain1, CG last.
        let mut mergeset_stack: Vec<Mergeset> = Vec::new();
        let mut checkpoint_merger_gd = None;
        let mut checkpoint_state: Option<UmcCascadePersistedState> = None;

        let virtual_blue_score = virtual_gd.blue_score;
        let mut curr_gd = Arc::new(virtual_gd.clone());
        loop {
            let selected_parent = curr_gd.selected_parent;
            let blues: Vec<(Hash, BlueWorkType)> =
                curr_gd.mergeset_blues.iter().map(|&h| (h, calc_work(self.headers_store.get_bits(h).unwrap()))).collect();

            let reds: Vec<(Hash, BlueWorkType)> =
                curr_gd.mergeset_reds.iter().map(|&h| (h, calc_work(self.headers_store.get_bits(h).unwrap()))).collect();

            mergeset_stack.push(Mergeset { selected_parent, mergeset_blues: blues, mergeset_reds: reds });

            let candidate_merger_gd = coloring_reader.get_coloring_data(selected_parent);
            let state_key = UmcCascadeKey::new(
                conflict_genesis,
                k,
                next_chain_ancestor_of_subgroup,
                Mergeset::checkpoint_hash_from_hashes(
                    candidate_merger_gd.selected_parent,
                    candidate_merger_gd.mergeset_blues.iter().copied(),
                    candidate_merger_gd.mergeset_reds.iter().copied(),
                ),
            );
            // Check if a checkpoint exists for the next chain block.
            // If found, break — run_cascade will reload from that state and skip
            // already-computed mergesets.
            if let Ok(Some(existing_state)) = self.umc_persistence_store.get_checkpoint(state_key) {
                checkpoint_merger_gd = Some(candidate_merger_gd);
                checkpoint_state = Some(existing_state);
                break;
            }

            if selected_parent == conflict_genesis {
                break;
            }

            curr_gd = coloring_reader.get_coloring_data(selected_parent);
        }

        let from_checkpoint = checkpoint_state.is_some();
        let estimated_effort_total = virtual_blue_score;
        let estimated_effort_saved = if from_checkpoint {
            // Estimate effort saved: virtual_blue_score - checkpoint_block.blue_score
            // This represents the blue blocks we didn't need to visit.
            checkpoint_merger_gd.expect("checkpoint state must have merger GHOSTDAG data").blue_score
        } else {
            0
        };

        let cg_work = calc_work(self.headers_store.get_bits(conflict_genesis).unwrap());
        let conflict_genesis_block = BlockWithWork::new(conflict_genesis, cg_work);

        debug!("k = {} | voting_deficit = {} | conflict_genesis_work = {}", k, cg_work * u64::from(k.isqrt()), cg_work);

        run_cascade(
            mergeset_stack,
            conflict_genesis_block,
            k,
            next_chain_ancestor_of_subgroup,
            &self.reachability_service,
            self.umc_persistence_store.clone(),
            checkpoint_state,
            estimated_effort_saved,
            estimated_effort_total,
            coloring_reader,
        )
    }
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;
    use crate::model::services::reachability::MTReachabilityService;
    use crate::model::stores::reachability::MemoryReachabilityStore;
    use crate::processes::dagknight::umc_cascade_persistence::{MemoryUmcCascadeStore, UmcCascadeKey, UmcCascadeStoreReader};
    use crate::processes::dagknight::umc_voting::test_fixtures::{MemoryColoringReader, make_gd};
    use crate::processes::reachability::inquirer;
    use kaspa_consensus_core::blockhash::ORIGIN;

    fn make_reachability()
    -> (MTReachabilityService<MemoryReachabilityStore>, std::sync::Arc<parking_lot::RwLock<MemoryReachabilityStore>>) {
        let mut store = MemoryReachabilityStore::new();
        inquirer::init(&mut store).unwrap();
        let arc = std::sync::Arc::new(parking_lot::RwLock::new(store));
        (MTReachabilityService::new(arc.clone()), arc)
    }

    fn reach_insert(arc: &std::sync::Arc<parking_lot::RwLock<MemoryReachabilityStore>>, hash: Hash, parent: Hash) {
        let mut store = arc.write();
        // Maintain nested intervals and child links required by ancestry queries.
        inquirer::add_block(&mut *store, hash, parent, &mut std::iter::empty()).unwrap();
    }

    fn work() -> BlueWorkType {
        BlueWorkType::from_u64(100)
    }

    fn coloring_reader(entries: &[(u64, u64, u64)]) -> MemoryColoringReader {
        let mut reader = MemoryColoringReader::default();
        for &(hash, selected_parent, blue_score) in entries {
            reader.add(Hash::from_u64_word(hash), make_gd(Hash::from_u64_word(selected_parent), vec![], vec![], blue_score));
        }
        reader
    }

    #[test]
    fn test_checkpoint_reload_produces_identical_result() {
        // Build a simple DAG: 1→2→3→4→5, conflict genesis at 3
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), ORIGIN);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1));
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(2));
        reach_insert(&arc, Hash::from_u64_word(4), Hash::from_u64_word(3));
        reach_insert(&arc, Hash::from_u64_word(5), Hash::from_u64_word(4));

        let store = Arc::new(MemoryUmcCascadeStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(3), work());
        let k: KType = 0;
        let nca = Hash::from_u64_word(2);
        let mut coloring_reader = coloring_reader(&[(3, 2, 3), (4, 3, 4)]);
        coloring_reader.add(Hash::from_u64_word(4), make_gd(Hash::from_u64_word(3), vec![Hash::from_u64_word(4)], vec![], 4));

        // Stack: Virtual → Chain4 → CG
        let stack: Vec<Mergeset> = vec![
            Mergeset {
                selected_parent: Hash::from_u64_word(4),
                mergeset_blues: vec![(Hash::from_u64_word(5), work())],
                mergeset_reds: vec![],
            },
            Mergeset {
                selected_parent: Hash::from_u64_word(3),
                mergeset_blues: vec![(Hash::from_u64_word(4), work())],
                mergeset_reds: vec![],
            },
        ];

        // First run — from scratch
        let result1 = run_cascade(stack.clone(), cg, k, nca, &reachability, store.clone(), None, 0, 5, &coloring_reader);
        assert!(!result1.from_checkpoint);
        assert_eq!(result1.estimated_effort_saved, 0);

        // Second run — same zone, but the caller loads the checkpoint saved by the first run
        let checkpoint_key = UmcCascadeKey::new(cg.hash, k, nca, stack[1].checkpoint_hash());
        let checkpoint_state = store.get_checkpoint(checkpoint_key).unwrap();
        assert!(checkpoint_state.is_some(), "checkpoint should have been saved");

        // Retain only mergesets above checkpoint (Virtual only)
        let stack_above_checkpoint: Vec<Mergeset> = stack[..1].to_vec();

        let result2 = run_cascade(
            stack_above_checkpoint,
            cg,
            k,
            nca,
            &reachability,
            store,
            checkpoint_state,
            1, // estimated_effort_saved estimate
            5, // estimated_effort_total
            &coloring_reader,
        );

        assert!(result2.from_checkpoint);
        assert_eq!(result2.estimated_effort_saved, 1);

        // Both should produce identical cascade results
        assert_eq!(result1.accepted, result2.accepted, "accepted mismatch");
        assert_eq!(result1.cascade_score, result2.cascade_score, "cascade score mismatch");
        assert_eq!(result1.flips, result2.flips, "flips mismatch");
    }

    #[test]
    fn test_checkpoint_with_grays_filtered() {
        // Test that gray filtering works correctly with checkpoint reload
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), ORIGIN);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1));
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(2));
        reach_insert(&arc, Hash::from_u64_word(4), Hash::from_u64_word(3));

        let store = Arc::new(MemoryUmcCascadeStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(2), work());
        let k: KType = 0;
        let nca = Hash::from_u64_word(1);
        let mut coloring_reader = coloring_reader(&[(2, 1, 2), (3, 2, 3)]);
        coloring_reader.add(
            Hash::from_u64_word(3),
            make_gd(Hash::from_u64_word(2), vec![Hash::from_u64_word(3)], vec![Hash::from_u64_word(1)], 3),
        );

        // Stack with gray block
        let stack: Vec<Mergeset> = vec![
            Mergeset {
                selected_parent: Hash::from_u64_word(3),
                mergeset_blues: vec![(Hash::from_u64_word(4), work())],
                mergeset_reds: vec![],
            },
            Mergeset {
                selected_parent: Hash::from_u64_word(2),
                mergeset_blues: vec![(Hash::from_u64_word(3), work())],
                mergeset_reds: vec![(Hash::from_u64_word(1), work())], // Gray: it is the next chain ancestor
            },
        ];

        let result1 = run_cascade(stack.clone(), cg, k, nca, &reachability, store.clone(), None, 0, 4, &coloring_reader);

        // Reload from checkpoint
        let checkpoint_key = UmcCascadeKey::new(cg.hash, k, nca, stack[1].checkpoint_hash());
        let checkpoint_state = store.get_checkpoint(checkpoint_key).unwrap();
        assert!(checkpoint_state.is_some());

        let result2 = run_cascade(
            stack[..1].to_vec(),
            cg,
            k,
            nca,
            &reachability,
            store.clone(),
            checkpoint_state,
            1,
            4, // estimated_effort_total
            &coloring_reader,
        );

        assert_eq!(result1.accepted, result2.accepted);
        assert_eq!(result1.cascade_score, result2.cascade_score);
        assert!(result2.from_checkpoint);
    }

    #[test]
    fn test_checkpoint_different_nca_different_key() {
        // Test that different NCA produces different checkpoint key
        let (reachability, arc) = make_reachability();
        reach_insert(&arc, Hash::from_u64_word(1), ORIGIN);
        reach_insert(&arc, Hash::from_u64_word(2), Hash::from_u64_word(1));
        reach_insert(&arc, Hash::from_u64_word(3), Hash::from_u64_word(1));

        let store = Arc::new(MemoryUmcCascadeStore::new());
        let cg = BlockWithWork::new(Hash::from_u64_word(1), work());
        let k: KType = 0;
        let mut coloring_reader = coloring_reader(&[(1, 0, 1), (2, 1, 2), (3, 1, 2)]);
        coloring_reader.add(
            Hash::from_u64_word(2),
            make_gd(Hash::from_u64_word(1), vec![Hash::from_u64_word(2)], vec![Hash::from_u64_word(3)], 2),
        );
        coloring_reader.add(
            Hash::from_u64_word(3),
            make_gd(Hash::from_u64_word(1), vec![Hash::from_u64_word(3)], vec![Hash::from_u64_word(2)], 2),
        );

        // First subgroup: NCA = 2
        let nca_1 = Hash::from_u64_word(2);
        let stack_1: Vec<Mergeset> = vec![
            Mergeset { selected_parent: Hash::from_u64_word(2), mergeset_blues: vec![], mergeset_reds: vec![] },
            Mergeset {
                selected_parent: Hash::from_u64_word(1),
                mergeset_blues: vec![(Hash::from_u64_word(2), work())],
                mergeset_reds: vec![(Hash::from_u64_word(3), work())],
            },
        ];

        let _result1 = run_cascade(stack_1.clone(), cg, k, nca_1, &reachability, store.clone(), None, 0, 2, &coloring_reader);

        // Second subgroup: NCA = 3 (different key, should not reuse checkpoint)
        let nca_2 = Hash::from_u64_word(3);
        let stack_2: Vec<Mergeset> = vec![
            Mergeset { selected_parent: Hash::from_u64_word(3), mergeset_blues: vec![], mergeset_reds: vec![] },
            Mergeset {
                selected_parent: Hash::from_u64_word(1),
                mergeset_blues: vec![(Hash::from_u64_word(3), work())],
                mergeset_reds: vec![(Hash::from_u64_word(2), work())],
            },
        ];

        let _result2 = run_cascade(stack_2.clone(), cg, k, nca_2, &reachability, store.clone(), None, 0, 2, &coloring_reader);

        // Verify both checkpoints exist with different keys
        let key_1 = UmcCascadeKey::new(cg.hash, k, nca_1, stack_1[1].checkpoint_hash());
        let key_2 = UmcCascadeKey::new(cg.hash, k, nca_2, stack_2[1].checkpoint_hash());

        assert!(store.get_checkpoint(key_1).unwrap().is_some(), "checkpoint for NCA1 should exist");
        assert!(store.get_checkpoint(key_2).unwrap().is_some(), "checkpoint for NCA2 should exist");
    }
}

#[cfg(test)]
mod voter_tests {
    use std::sync::Arc;

    use super::*;
    use crate::processes::dagknight::{
        umc_cascade_persistence::MemoryUmcCascadeStore,
        umc_voting::{UmcVoter, test_fixtures::Fixture},
    };

    #[test]
    fn test_segment_tree_voter_vote() {
        let fixture = Fixture::new();
        let voter =
            SegmentTreeUmcVoter::new(fixture.headers.clone(), Arc::new(MemoryUmcCascadeStore::new()), fixture.reachability.clone());
        let ctx = fixture.context();

        let result = voter.vote(&ctx);

        assert_eq!(result.cascade_score, fixture.expected_score(), "cascade score mismatch");
        assert!(result.accepted, "zone should be accepted");
        assert_eq!(result.voting_blocks, 16, "blues 11, 10, 9, 7, 6, 5, 4, 3, 2 + CG + reds 12..17 (gray red 8 excluded)");
        assert_eq!(result.flips, 0, "segment tree cascade has no flips on this zone");
        assert!(!result.from_checkpoint);
    }

    #[test]
    fn test_segment_tree_voter_checkpoint_reload() {
        let fixture = Fixture::new();
        let store = Arc::new(MemoryUmcCascadeStore::new());
        let voter = SegmentTreeUmcVoter::new(fixture.headers.clone(), store, fixture.reachability.clone());
        let ctx = fixture.context();

        // First vote: computes from scratch and persists checkpoints
        let result1 = voter.vote(&ctx);
        // Second vote: finds the checkpoint saved at chain block 11 and reloads from it
        let result2 = voter.vote(&ctx);

        assert!(!result1.from_checkpoint, "first vote must start from scratch");
        assert!(result2.from_checkpoint, "second vote must reload the persisted checkpoint");

        // The cascade outcome must be identical regardless of the checkpoint
        assert_eq!(result1.cascade_score, result2.cascade_score, "cascade score mismatch");
        assert_eq!(result1.accepted, result2.accepted, "accepted mismatch");
        assert_eq!(result1.flips, result2.flips, "flips mismatch");
        assert_eq!(result1.voting_blocks, result2.voting_blocks, "voting blocks mismatch");
    }
}
