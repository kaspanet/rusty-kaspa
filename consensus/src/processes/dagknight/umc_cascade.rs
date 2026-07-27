use std::collections::HashMap;

use kaspa_hashes::Hash;

use crate::model::services::reachability::ReachabilityService;
use crate::processes::dagknight::{AppendableSegmentTree, Bucket, bucket_for_score};

// ============================================================================
// Cascade Maintainer
// ============================================================================

/// Maintains exact cascade scores for one fixed k using chain decomposition
/// and lazy segment trees with event-driven bucket-transition propagation.
pub struct CascadeMaintainer {
    chains: Vec<Vec<Hash>>,
    trees: Vec<AppendableSegmentTree<Hash>>,
    chain_id: HashMap<Hash, usize>,
    deficit: i64,
    blue_count: i64,
    red_count: i64,
    negative_count: i64,
}

impl CascadeMaintainer {
    pub fn new(deficit: i64) -> Self {
        Self {
            chains: Vec::new(),
            trees: Vec::new(),
            chain_id: HashMap::new(),
            deficit,
            blue_count: 0,
            red_count: 0,
            negative_count: 0,
        }
    }

    /// Insert a new blue block into the chain decomposition and stabilize cascade.
    pub fn add_blue(&mut self, block: Hash, reachability: &impl ReachabilityService) {
        let initial_score = self.deficit;
        let initial_bucket = bucket_for_score(initial_score);

        let chain_id = self.find_extendable_chain(block, reachability).unwrap_or_else(|| {
            let id = self.chains.len();
            self.chains.push(Vec::new());
            self.trees.push(AppendableSegmentTree::new());
            id
        });

        self.append_to_chain(chain_id, block, initial_score, initial_bucket);

        // A new blue block contributes according to its initial bucket.
        let initial_contribution = match initial_bucket {
            Bucket::Positive => 1,
            Bucket::Negative => -1,
        };
        self.process_event(block, initial_contribution, reachability);
    }

    /// Add a new red block and emit -1 event to ancestors.
    pub fn add_red(&mut self, block: Hash, reachability: &impl ReachabilityService) {
        self.red_count += 1;
        self.process_event(block, -1, reachability);
    }

    /// Add a gray block (red block that agrees with the current side).
    /// Grays don't vote - they don't emit events and don't affect the cascade.
    pub fn add_gray(&mut self, _block: Hash) {
        // Gray blocks are tracked but don't participate in cascade voting.
        // They don't emit events, don't affect scores, and don't count toward red_count.
    }

    /// Compute virtual score: |U| - |R| - 2*negative_count + deficit
    pub fn virtual_score(&self) -> i64 {
        self.blue_count - self.red_count - 2 * self.negative_count + self.deficit
    }

    pub fn virtual_accepts(&self) -> bool {
        self.virtual_score() >= 0
    }

    // ----- Chain operations -----

    fn find_extendable_chain(&self, block: Hash, reachability: &impl ReachabilityService) -> Option<usize> {
        for (chain_id, chain) in self.chains.iter().enumerate() {
            if let Some(&head) = chain.last() {
                if reachability.is_chain_ancestor_of(head, block) {
                    return Some(chain_id);
                }
            }
        }
        None
    }

    fn append_to_chain(&mut self, chain_id: usize, block: Hash, initial_score: i64, initial_bucket: Bucket) {
        self.chains[chain_id].push(block);
        self.trees[chain_id].append_leaf(block, initial_score);
        self.chain_id.insert(block, chain_id);
        self.blue_count += 1;

        if initial_bucket == Bucket::Negative {
            self.negative_count += 1;
        }
    }

    // ----- Event processing -----

    fn process_event(&mut self, source: Hash, delta: i64, reachability: &impl ReachabilityService) {
        let mut queue = Vec::new();

        self.apply_event(source, delta, reachability);

        // Extract crossings and propagate
        let mut changed = true;
        while changed {
            changed = false;

            for tree in self.trees.iter_mut() {
                while let Some(v) = tree.extract_positive_below_zero() {
                    tree.flip_to_negative(v);
                    self.negative_count += 1;
                    queue.push((v, -2));
                    changed = true;
                }

                while let Some(v) = tree.extract_negative_at_least_zero() {
                    tree.flip_to_positive(v);
                    self.negative_count -= 1;
                    queue.push((v, 2));
                    changed = true;
                }
            }

            for (source, delta) in queue.drain(..) {
                self.apply_event(source, delta, reachability);
            }
        }
    }

    fn apply_event(&mut self, source: Hash, delta: i64, reachability: &impl ReachabilityService) {
        for (chain, tree) in self.chains.iter().zip(self.trees.iter_mut()) {
            let p = last_ancestor_index(chain, source, reachability);
            if p > 0 {
                tree.prefix_add(p, delta);
            }
        }
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
        if reachability.is_chain_ancestor_of(v_mid, source) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Run cascade voting on a set of blues, reds, and grays.
/// Grays are red blocks that agree with the current side and don't vote.
/// Returns true if the virtual score is >= 0 (UMC accepted).
pub fn run_cascade(blues: &[Hash], reds: &[Hash], grays: &[Hash], deficit: i64, reachability: &impl ReachabilityService) -> bool {
    let mut maintainer = CascadeMaintainer::new(deficit);

    for &blue in blues {
        maintainer.add_blue(blue, reachability);
    }

    for &red in reds {
        maintainer.add_red(red, reachability);
    }

    // Grays are recorded but don't vote - they don't emit events
    for &gray in grays {
        maintainer.add_gray(gray);
    }

    maintainer.virtual_accepts()
}

// Cascade maintainer testing is done through protocol integration tests.
// The reachability service requires proper store setup which is handled
// by DagBuilder in protocol tests.
