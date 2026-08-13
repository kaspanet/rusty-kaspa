use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use kaspa_consensus_core::{BlockHashMap, BlueWorkType, KType};
use kaspa_hashes::Hash;
use kaspa_math::int::SignedInteger;
use num_traits::Zero;

use crate::model::services::reachability::ReachabilityService;
use crate::processes::dagknight::{AppendableSegmentTree, Bucket, bucket_for_score};
use crate::processes::ghostdag::ordering::SortableBlock;

type SignedWork = SignedInteger<BlueWorkType>;

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
}

/// A block identifier paired with that block's own proof-of-work contribution.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockWithWork {
    pub hash: Hash,
    pub work: BlueWorkType,
}

#[derive(Eq, PartialEq)]
pub enum BlockColor {
    BLUE,
    RED,
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
    pub fn new(conflict_genesis: BlockWithWork, k: KType) -> Self {
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
        }
    }

    /// Insert a new blue block into the chain decomposition and stabilize cascade.
    pub fn add_blue(&mut self, block: BlockWithWork, reachability: &impl ReachabilityService) {
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
        self.process_event(block.hash, initial_contribution, reachability);
    }

    /// Add a new red block and propagate its negative work to ancestor blues.
    pub fn add_red(&mut self, block: BlockWithWork, reachability: &impl ReachabilityService) {
        self.red_work = self.red_work + block.work;
        self.process_event(block.hash, work_delta(block.work, Bucket::Negative), reachability);
    }

    /// Add a gray block (red block that agrees with the current side).
    /// Grays don't vote - they don't emit events and don't affect the cascade.
    pub fn add_gray(&mut self, _block: BlockWithWork) {
        // Gray blocks are tracked but don't participate in cascade voting.
        // They don't emit events, don't affect scores, and don't count toward red work.
    }

    /// Returns the aggregate score of the virtual block.
    ///
    /// Each blue contributes its work positively or negatively according to its
    /// final bucket, while red work contributes negatively.
    fn virtual_score(&self) -> SignedWork {
        SignedWork::from(self.blue_work) + SignedWork::from(self.deficit_work)
            - SignedWork::from(self.red_work)
            - SignedWork::from(self.negative_blue_work * 2)
    }

    /// Check if the virtual block's aggregate cascade score is non-negative.
    pub fn virtual_accepts(&self) -> bool {
        self.virtual_score() >= SignedWork::zero()
    }

    // ----- Chain operations -----

    fn find_extendable_chain(&self, block: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
        for (chain_id, chain) in self.blues_chains_decomposition.iter().enumerate() {
            if let Some(&head) = chain.last() {
                if reachability.is_dag_ancestor_of(head, block) {
                    return Some(chain_id);
                }
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

    fn process_event(&mut self, source: Hash, delta: SignedWork, reachability: &impl ReachabilityService) {
        let mut queue = Vec::new();

        self.apply_event(source, delta, reachability);

        // Extract crossings and propagate
        let mut changed = true;
        while changed {
            changed = false;

            for tree in self.chains_score_trees.iter_mut() {
                while let Some(block) = tree.extract_positive_below_zero() {
                    tree.flip_to_negative(block);
                    self.negative_blue_work = self.negative_blue_work + block.work;
                    self.flip_count += 1;
                    queue.push((block.hash, work_delta(block.work * 2u64, Bucket::Negative)));
                    changed = true;
                }

                while let Some(block) = tree.extract_negative_at_least_zero() {
                    tree.flip_to_positive(block);
                    self.negative_blue_work = self.negative_blue_work - block.work;
                    self.flip_count += 1;
                    queue.push((block.hash, work_delta(block.work * 2u64, Bucket::Positive)));
                    changed = true;
                }
            }

            for (source, delta) in queue.drain(..) {
                self.apply_event(source, delta, reachability);
            }
        }
    }

    fn apply_event(&mut self, source: Hash, delta: SignedWork, reachability: &impl ReachabilityService) {
        for (chain, tree) in self.blues_chains_decomposition.iter().zip(self.chains_score_trees.iter_mut()) {
            if let Some(ancestor_index) = strict_ancestor_index(chain, source, reachability) {
                tree.prefix_add(ancestor_index, delta);
            }
        }
    }
}

/// Returns the exclusive end index of the strict-ancestor prefix, or `None` if it is empty.
fn strict_ancestor_index(chain: &[Hash], source: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
    let mut lo = 0usize;
    let mut hi = chain.len();

    while lo < hi {
        let mid = (lo + hi + 1) / 2;
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

/// Cascade result including flip statistics for performance monitoring.
#[derive(Debug, Clone)]
pub struct CascadeResult {
    pub virtual_score: SignedInteger<BlueWorkType>,
    pub accepted: bool,
    pub flips: u64,
    pub voting_blocks: u64,
}

/// Run cascade voting on a set of blues and reds in topological order.
/// Each block's events propagate backward to already-processed strict ancestors.
pub fn run_cascade(
    mut topological_heap: BinaryHeap<Reverse<SortableBlock>>,
    block_map: BlockHashMap<(BlockWithWork, BlockColor)>,
    conflict_genesis: BlockWithWork,
    k: KType,
    reachability: &impl ReachabilityService,
) -> CascadeResult {
    let mut maintainer = CascadeMaintainer::new(conflict_genesis, k);
    let mut voting_blocks = 0;

    while !topological_heap.is_empty() {
        let Reverse(SortableBlock { hash, .. }) = topological_heap.pop().unwrap();
        let (block_with_work, color) = &block_map[&hash];

        if *color == BlockColor::BLUE {
            maintainer.add_blue(*block_with_work, reachability);
        } else {
            maintainer.add_red(*block_with_work, reachability);
        }

        voting_blocks += 1;
    }

    let virtual_score = maintainer.virtual_score();
    let accepted = virtual_score >= SignedWork::zero();

    CascadeResult { virtual_score, accepted, flips: maintainer.flip_count, voting_blocks }
}

// Cascade maintainer testing is done through protocol integration tests.
// The reachability service requires proper store setup which is handled
// by DagBuilder in protocol tests.
