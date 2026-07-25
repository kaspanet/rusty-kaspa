use std::collections::HashMap;

use kaspa_hashes::Hash;

use crate::model::services::reachability::ReachabilityService;

type Sign = i16;
const POSITIVE: Sign = 1;
const NEGATIVE: Sign = -1;

const INF: i64 = i64::MAX;
const NEG_INF: i64 = i64::MIN;

#[derive(Clone, Debug)]
struct NodeSummary {
    min_pos_score: i64,
    argmin_pos: Option<Hash>,
    max_neg_score: i64,
    argmax_neg: Option<Hash>,
    lazy_add: i64,
}

impl NodeSummary {
    fn empty() -> Self {
        Self {
            min_pos_score: INF,
            argmin_pos: None,
            max_neg_score: NEG_INF,
            argmax_neg: None,
            lazy_add: 0,
        }
    }

    fn leaf(vertex: Hash, score: i64, sign: Sign) -> Self {
        if sign == POSITIVE {
            Self {
                min_pos_score: score,
                argmin_pos: Some(vertex),
                max_neg_score: NEG_INF,
                argmax_neg: None,
                lazy_add: 0,
            }
        } else {
            Self {
                min_pos_score: INF,
                argmin_pos: None,
                max_neg_score: score,
                argmax_neg: Some(vertex),
                lazy_add: 0,
            }
        }
    }
}

fn merge(left: &NodeSummary, right: &NodeSummary) -> NodeSummary {
    let (min_pos_score, argmin_pos) = if left.min_pos_score <= right.min_pos_score {
        (left.min_pos_score, left.argmin_pos)
    } else {
        (right.min_pos_score, right.argmin_pos)
    };

    let (max_neg_score, argmax_neg) = if left.max_neg_score >= right.max_neg_score {
        (left.max_neg_score, left.argmax_neg)
    } else {
        (right.max_neg_score, right.argmax_neg)
    };

    NodeSummary {
        min_pos_score,
        argmin_pos,
        max_neg_score,
        argmax_neg,
        lazy_add: 0,
    }
}

pub struct AppendableChainSegmentTree {
    size: usize,
    capacity: usize,
    vertices: Vec<Hash>,
    index: HashMap<Hash, usize>,
    sign: HashMap<Hash, Sign>,
    tree: Vec<NodeSummary>,
}

impl AppendableChainSegmentTree {
    pub fn new() -> Self {
        let capacity = 1;
        Self {
            size: 0,
            capacity,
            vertices: Vec::new(),
            index: HashMap::new(),
            sign: HashMap::new(),
            tree: vec![NodeSummary::empty(); 2 * capacity],
        }
    }

    pub fn append_leaf(&mut self, vertex: Hash, initial_score: i64, initial_sign: Sign) {
        assert!(!self.index.contains_key(&vertex), "vertex already present");

        if self.size == self.capacity {
            self.grow();
        }

        let leaf_pos = self.size;
        self.size += 1;

        self.vertices.push(vertex);
        self.index.insert(vertex, leaf_pos);
        self.sign.insert(vertex, initial_sign);

        let node = self.capacity + leaf_pos;
        self.tree[node] = NodeSummary::leaf(vertex, initial_score, initial_sign);
        self.pull_path(node);
    }

    pub fn prefix_add(&mut self, p: usize, delta: i64) {
        if p == 0 {
            return;
        }
        self.range_add(1, 0, self.capacity, 0, p, delta);
    }

    pub fn has_positive_below_zero(&self) -> bool {
        self.tree[1].min_pos_score < 0
    }

    pub fn has_negative_at_least_zero(&self) -> bool {
        self.tree[1].max_neg_score >= 0
    }

    pub fn extract_positive_below_zero(&self) -> Option<Hash> {
        if !self.has_positive_below_zero() {
            return None;
        }
        self.tree[1].argmin_pos
    }

    pub fn extract_negative_at_least_zero(&self) -> Option<Hash> {
        if !self.has_negative_at_least_zero() {
            return None;
        }
        self.tree[1].argmax_neg
    }

    pub fn flip_to_negative(&mut self, vertex: Hash) {
        self.set_sign(vertex, NEGATIVE);
    }

    pub fn flip_to_positive(&mut self, vertex: Hash) {
        self.set_sign(vertex, POSITIVE);
    }

    pub fn score(&mut self, vertex: Hash) -> i64 {
        let pos = *self.index.get(&vertex).expect("vertex not in tree");
        self.point_score(1, 0, self.capacity, pos)
    }

    fn point_score(&mut self, node: usize, left: usize, right: usize, target: usize) -> i64 {
        if right - left == 1 {
            let summary = &self.tree[node];
            let vertex = self.vertices[target];
            let sign = self.sign[&vertex];
            if sign == POSITIVE {
                summary.min_pos_score
            } else {
                summary.max_neg_score
            }
        } else {
            self.push(node);
            let mid = (left + right) / 2;
            if target < mid {
                self.point_score(2 * node, left, mid, target)
            } else {
                self.point_score(2 * node + 1, mid, right, target)
            }
        }
    }

    // ----- Sign bucket updates -----

    fn set_sign(&mut self, vertex: Hash, new_sign: Sign) {
        let pos = *self.index.get(&vertex).expect("vertex not in tree");
        let current_score = self.point_score(1, 0, self.capacity, pos);
        self.sign.insert(vertex, new_sign);
        self.point_assign_leaf(1, 0, self.capacity, pos, vertex, current_score, new_sign);
    }

    fn point_assign_leaf(&mut self, node: usize, left: usize, right: usize, target: usize, vertex: Hash, score: i64, sign: Sign) {
        if right - left == 1 {
            self.tree[node] = NodeSummary::leaf(vertex, score, sign);
            return;
        }
        self.push(node);
        let mid = (left + right) / 2;
        if target < mid {
            self.point_assign_leaf(2 * node, left, mid, target, vertex, score, sign);
        } else {
            self.point_assign_leaf(2 * node + 1, mid, right, target, vertex, score, sign);
        }
        self.pull(node);
    }

    // ----- Range and point operations -----

    fn range_add(&mut self, node: usize, left: usize, right: usize, q_left: usize, q_right: usize, delta: i64) {
        if q_right <= left || right <= q_left {
            return;
        }
        if q_left <= left && right <= q_right {
            self.apply_add(node, delta);
            return;
        }
        self.push(node);
        let mid = (left + right) / 2;
        self.range_add(2 * node, left, mid, q_left, q_right, delta);
        self.range_add(2 * node + 1, mid, right, q_left, q_right, delta);
        self.pull(node);
    }

    // ----- Grow support -----

    fn grow(&mut self) {
        let records = self.materialize_records();
        self.capacity *= 2;
        self.tree = vec![NodeSummary::empty(); 2 * self.capacity];

        for (pos, (vertex, score, sign)) in records.into_iter().enumerate() {
            self.tree[self.capacity + pos] = NodeSummary::leaf(vertex, score, sign);
        }

        for node in (1..self.capacity).rev() {
            self.pull(node);
        }
    }

    fn materialize_records(&mut self) -> Vec<(Hash, i64, Sign)> {
        self.push_all(1, 0, self.capacity);
        self.vertices
            .iter()
            .map(|&vertex| {
                let pos = self.index[&vertex];
                let leaf = &self.tree[self.capacity + pos];
                let sign = self.sign[&vertex];
                let score = if sign == POSITIVE {
                    leaf.min_pos_score
                } else {
                    leaf.max_neg_score
                };
                (vertex, score, sign)
            })
            .collect()
    }

    // ----- Lazy mechanics -----

    fn apply_add(&mut self, node: usize, delta: i64) {
        let summary = &mut self.tree[node];
        if summary.argmin_pos.is_some() {
            summary.min_pos_score += delta;
        }
        if summary.argmax_neg.is_some() {
            summary.max_neg_score += delta;
        }
        summary.lazy_add += delta;
    }

    fn push(&mut self, node: usize) {
        let delta = self.tree[node].lazy_add;
        if delta == 0 {
            return;
        }
        self.apply_add(2 * node, delta);
        self.apply_add(2 * node + 1, delta);
        self.tree[node].lazy_add = 0;
    }

    fn push_all(&mut self, node: usize, left: usize, right: usize) {
        if right - left == 1 {
            self.tree[node].lazy_add = 0;
            return;
        }
        self.push(node);
        let mid = (left + right) / 2;
        self.push_all(2 * node, left, mid);
        self.push_all(2 * node + 1, mid, right);
        self.pull(node);
    }

    fn pull(&mut self, node: usize) {
        self.tree[node] = merge(&self.tree[2 * node], &self.tree[2 * node + 1]);
    }

    fn pull_path(&mut self, mut node: usize) {
        node /= 2;
        while node >= 1 {
            self.pull(node);
            node /= 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_score() {
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), 5, POSITIVE);
        tree.append_leaf(2_u64.into(), -3, NEGATIVE);
        tree.append_leaf(3_u64.into(), 0, POSITIVE);

        assert_eq!(tree.score(1_u64.into()), 5);
        assert_eq!(tree.score(2_u64.into()), -3);
        assert_eq!(tree.score(3_u64.into()), 0);
    }

    #[test]
    fn test_prefix_add() {
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), 2, POSITIVE);
        tree.append_leaf(2_u64.into(), 0, POSITIVE);
        tree.append_leaf(3_u64.into(), -1, NEGATIVE);
        tree.append_leaf(4_u64.into(), -3, NEGATIVE);
        tree.append_leaf(5_u64.into(), 4, POSITIVE);

        tree.prefix_add(4, -1);

        assert_eq!(tree.score(1_u64.into()), 1);
        assert_eq!(tree.score(2_u64.into()), -1);
        assert_eq!(tree.score(3_u64.into()), -2);
        assert_eq!(tree.score(4_u64.into()), -4);
        assert_eq!(tree.score(5_u64.into()), 4);
    }

    #[test]
    fn test_crossing_detection_and_flip() {
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), 2, POSITIVE);
        tree.append_leaf(2_u64.into(), 0, POSITIVE);
        tree.append_leaf(3_u64.into(), -1, NEGATIVE);

        tree.prefix_add(4, -1);

        assert!(tree.has_positive_below_zero());
        let bad = tree.extract_positive_below_zero().unwrap();
        assert_eq!(bad, 2_u64.into());

        tree.flip_to_negative(bad);

        assert!(!tree.has_positive_below_zero());
        assert!(!tree.has_negative_at_least_zero());
    }

    #[test]
    fn test_negative_flips_to_positive() {
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), -1, NEGATIVE);
        tree.append_leaf(2_u64.into(), -2, NEGATIVE);

        tree.prefix_add(2, 3);
        // v1: -1+3=2 (negative bucket), v2: -2+3=1 (negative bucket)

        assert!(tree.has_negative_at_least_zero());
        let good = tree.extract_negative_at_least_zero().unwrap();
        assert_eq!(good, 1_u64.into());

        tree.flip_to_positive(good);

        // v1 is now positive(2), v2 is still negative(1)
        // v2 is still >= 0 so has_negative_at_least_zero is still true
        assert!(tree.has_negative_at_least_zero());
        assert!(!tree.has_positive_below_zero());

        // Flip v2 too
        let good2 = tree.extract_negative_at_least_zero().unwrap();
        assert_eq!(good2, 2_u64.into());
        tree.flip_to_positive(good2);

        assert!(!tree.has_negative_at_least_zero());
        assert!(!tree.has_positive_below_zero());
    }

    #[test]
    fn test_grow_capacity() {
        let mut tree = AppendableChainSegmentTree::new();
        for i in 0..5u64 {
            tree.append_leaf(i.into(), i as i64, POSITIVE);
        }

        for i in 0..5u64 {
            assert_eq!(tree.score(i.into()), i as i64);
        }

        tree.prefix_add(3, -2);
        assert_eq!(tree.score(0.into()), -2);
        assert_eq!(tree.score(1.into()), -1);
        assert_eq!(tree.score(2.into()), 0);
        assert_eq!(tree.score(3.into()), 3);
        assert_eq!(tree.score(4.into()), 4);
    }

    #[test]
    fn test_prefix_add_zero() {
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), 5, POSITIVE);
        tree.prefix_add(0, -100);
        assert_eq!(tree.score(1_u64.into()), 5);
    }

    #[test]
    fn test_grow_preserves_lazy_state() {
        // Add 2 items (capacity 1 grows to 2), then add more to grow to 4
        let mut tree = AppendableChainSegmentTree::new();
        tree.append_leaf(1_u64.into(), 10, POSITIVE);
        tree.prefix_add(1, 5); // score = 15
        tree.append_leaf(2_u64.into(), 3, POSITIVE); // triggers grow from 1->2

        // After grow, v1 should still be 15
        assert_eq!(tree.score(1_u64.into()), 15);
        assert_eq!(tree.score(2_u64.into()), 3);

        tree.prefix_add(2, -4);
        assert_eq!(tree.score(1_u64.into()), 11);
        assert_eq!(tree.score(2_u64.into()), -1);

        // Now trigger another grow
        tree.append_leaf(3_u64.into(), 7, NEGATIVE);
        tree.append_leaf(4_u64.into(), -2, NEGATIVE);

        // Should still be correct after grow 2->4
        assert_eq!(tree.score(1_u64.into()), 11);
        assert_eq!(tree.score(2_u64.into()), -1);
        assert_eq!(tree.score(3_u64.into()), 7);
        assert_eq!(tree.score(4_u64.into()), -2);
    }
}

// ============================================================================
// Cascade Maintainer
// ============================================================================

/// Maintains exact cascade scores for one fixed k using chain decomposition
/// and lazy segment trees with event-driven sign-flip propagation.
pub struct CascadeMaintainer {
     chains: Vec<Vec<Hash>>,
    trees: Vec<AppendableChainSegmentTree>,
    sign: HashMap<Hash, Sign>,
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
            sign: HashMap::new(),
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
        let initial_sign = if initial_score >= 0 { POSITIVE } else { NEGATIVE };

        let chain_id = self.find_extendable_chain(block, reachability)
            .unwrap_or_else(|| {
                let id = self.chains.len();
                self.chains.push(Vec::new());
                self.trees.push(AppendableChainSegmentTree::new());
                id
            });

        self.append_to_chain(chain_id, block, initial_score, initial_sign);

        // New blue block emits an event for its own sign to ancestors
        self.process_event(block, initial_sign as i64, reachability);
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

    fn append_to_chain(&mut self, chain_id: usize, block: Hash, initial_score: i64, initial_sign: Sign) {
        self.chains[chain_id].push(block);
        self.trees[chain_id].append_leaf(block, initial_score, initial_sign);
        self.chain_id.insert(block, chain_id);
        self.sign.insert(block, initial_sign);
        self.blue_count += 1;

        if initial_sign == NEGATIVE {
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
                    self.sign.insert(v, NEGATIVE);
                    self.negative_count += 1;
                    queue.push((v, -2));
                    changed = true;
                }

                while let Some(v) = tree.extract_negative_at_least_zero() {
                    tree.flip_to_positive(v);
                    self.sign.insert(v, POSITIVE);
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
pub fn run_cascade(
    blues: &[Hash],
    reds: &[Hash],
    grays: &[Hash],
    deficit: i64,
    reachability: &impl ReachabilityService,
) -> bool {
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
