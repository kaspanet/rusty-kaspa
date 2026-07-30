use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

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
    /// Mapping from block hash to work, used for debug bucket collection
    block_work_map: HashMap<Hash, BlueWorkType>,
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
            block_work_map: HashMap::new(),
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
        self.block_work_map.insert(block.hash, block.work);

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
            let p = last_ancestor_index(chain, source, reachability);
            if p > 0 {
                tree.prefix_add(p, delta);
            }
        }
    }

    /// Collect per-blue bucket assignments from all segment trees for debug comparison.
    /// TODO[DK]: Remove when debugging against baseline is done
    pub fn collect_blue_buckets(&mut self) -> CascadeDebugInfo {
        let mut per_blue_buckets = HashMap::new();
        let mut blue_hashes = HashSet::new();

        for (chain_id, chain) in self.blues_chains_decomposition.iter().enumerate() {
            let tree = &mut self.chains_score_trees[chain_id];
            for &hash in chain.iter() {
                blue_hashes.insert(hash);
                // Reconstruct BlockWithWork from the hash and stored work
                let work = self.block_work_map[&hash];
                let block = BlockWithWork::new(hash, work);
                let score = tree.score(block);
                per_blue_buckets.insert(hash, bucket_for_score(score));
            }
        }

        CascadeDebugInfo { per_blue_buckets, blue_hashes }
    }
}

/// Binary search for the prefix of the chain where all elements are chain ancestors of source.
/// Returns the prefix length p such that chain[0..p] are all ancestors of source.
fn last_ancestor_index(chain: &[Hash], source: Hash, reachability: &impl ReachabilityService) -> usize {
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
    lo
}

/// Per-blue debug information for comparing baseline vs cascade bucket assignments.
#[derive(Debug, Clone)]
pub struct CascadeDebugInfo {
    /// Map from blue hash to its final bucket (positive/negative) in the cascade
    pub per_blue_buckets: HashMap<Hash, Bucket>,
    /// Set of all blue hashes (for easy iteration in debug logs)
    pub blue_hashes: HashSet<Hash>,
}

/// Cascade result including flip statistics for performance monitoring.
#[derive(Debug, Clone)]
pub struct CascadeResult {
    pub accepted: bool,
    pub flips: u64,
    pub voting_blocks: u64,
    /// Debug information with per-blue bucket assignments (populated during comparison mode)
    pub debug_info: Option<CascadeDebugInfo>,
}

/// Run cascade voting on a set of blues, reds, and grays.
/// Two-phase processing: (1) all blues in topological order, (2) all reds in topological order.
/// Each block's events propagate backward to already-processed ancestors.
pub fn run_cascade(
    mut topological_heap: BinaryHeap<Reverse<SortableBlock>>,
    block_map: BlockHashMap<(BlockWithWork, BlockColor)>,
    conflict_genesis: BlockWithWork,
    k: KType,
    reachability: &impl ReachabilityService,
) -> CascadeResult {
    let mut maintainer = CascadeMaintainer::new(conflict_genesis, k);
    let mut voting_blocks = 0;

    // FIXME: Topological order should work just as well as processing blues first then reds first.
    // However, this is not the case, indicating that there is some sensitivity related to how reds
    // propagate and how flips work. If you uncomment this and comment out the 2 phase processing below
    // simpa will panic due to the strict baseline comparison.

    // while !topological_heap.is_empty() {
    //     let Reverse(SortableBlock { hash, .. }) = topological_heap.pop().unwrap();
    //     let (block_with_work, color) = &block_map[&hash];

    //     if *color == BlockColor::BLUE {
    //         maintainer.add_blue(*block_with_work, reachability);
    //     } else {
    //         maintainer.add_red(*block_with_work, reachability);
    //     }

    //     voting_blocks += 1;
    // }

    // Phase 1: Process all blues first (in topological order)
    let mut reds: Vec<SortableBlock> = Vec::new();
    while let Some(Reverse(sb)) = topological_heap.pop() {
        if let Some((block_with_work, color)) = block_map.get(&sb.hash) {
            if *color == BlockColor::BLUE {
                maintainer.add_blue(*block_with_work, reachability);
                voting_blocks += 1;
            } else {
                reds.push(sb);
            }
        }
    }

    // Phase 2: Process all reds (in topological order)
    for sb in reds {
        if let Some((block_with_work, color)) = block_map.get(&sb.hash) {
            if *color == BlockColor::RED {
                maintainer.add_red(*block_with_work, reachability);
                voting_blocks += 1;
            }
        }
    }

    let accepted = maintainer.virtual_accepts();
    let debug_info = maintainer.collect_blue_buckets();

    CascadeResult { accepted, flips: maintainer.flip_count, voting_blocks, debug_info: Some(debug_info) }
}

// Cascade maintainer testing is done through protocol integration tests.
// The reachability service requires proper store setup which is handled
// by DagBuilder in protocol tests.
