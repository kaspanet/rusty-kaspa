use std::{
    collections::HashMap,
    hash::Hash,
    ops::{AddAssign, Range, Sub},
};

use num_traits::Zero;

use crate::processes::dagknight::appendable_segment_tree_api::{
    AppendableSegmentTreeApi, Bucket, DEFAULT_INITIAL_CAPACITY, bucket_for_score,
};

type LeafPosition = usize;
type NodeIndex = usize;

use super::segment_tree_ops::{
    coalesce_ranges, left_child, parent, range_fully_contains, ranges_are_disjoint, right_child, split_range,
};

const ROOT_NODE: NodeIndex = 1;

#[derive(Clone, Copy, Debug)]
struct ScoreCandidate<T, S> {
    score: S,
    leaf: T,
}

#[derive(Clone, Debug)]
struct BucketExtrema<T, S> {
    min_score: Option<ScoreCandidate<T, S>>,
    min_positive: Option<ScoreCandidate<T, S>>,
    max_negative: Option<ScoreCandidate<T, S>>,
    pending_delta: S,
}

impl<T, S> BucketExtrema<T, S>
where
    T: Copy,
    S: Copy + PartialOrd + AddAssign + Sub<Output = S> + Zero,
{
    fn empty() -> Self {
        Self { min_score: None, min_positive: None, max_negative: None, pending_delta: S::zero() }
    }

    /// Restores a leaf together with its stored bucket. The bucket is intentionally
    /// not recomputed from `score`, because a pending crossing may make them disagree.
    fn restore_leaf_with_bucket(leaf: T, score: S, bucket: Bucket) -> Self {
        let candidate = ScoreCandidate { score, leaf };
        match bucket {
            Bucket::Positive => {
                Self { min_score: Some(candidate), min_positive: Some(candidate), max_negative: None, pending_delta: S::zero() }
            }
            Bucket::Negative => {
                Self { min_score: Some(candidate), min_positive: None, max_negative: Some(candidate), pending_delta: S::zero() }
            }
        }
    }

    fn create_leaf_with_score(leaf: T, score: S) -> Self {
        Self::restore_leaf_with_bucket(leaf, score, bucket_for_score(score))
    }

    fn merge(left_child: &Self, right_child: &Self) -> Self {
        Self {
            min_score: minimum_candidate(left_child.min_score, right_child.min_score),
            min_positive: minimum_candidate(left_child.min_positive, right_child.min_positive),
            max_negative: maximum_candidate(left_child.max_negative, right_child.max_negative),
            pending_delta: S::zero(),
        }
    }

    fn apply_delta(&mut self, delta: S) {
        if let Some(candidate) = self.min_score.as_mut() {
            candidate.score += delta;
        }
        if let Some(candidate) = self.min_positive.as_mut() {
            candidate.score += delta;
        }
        if let Some(candidate) = self.max_negative.as_mut() {
            candidate.score += delta;
        }
        self.pending_delta += delta;
    }
}

fn minimum_candidate<T: Copy, S: Copy + PartialOrd>(
    left_candidate: Option<ScoreCandidate<T, S>>,
    right_candidate: Option<ScoreCandidate<T, S>>,
) -> Option<ScoreCandidate<T, S>> {
    match (left_candidate, right_candidate) {
        (Some(left_candidate), Some(right_candidate)) => {
            Some(if left_candidate.score <= right_candidate.score { left_candidate } else { right_candidate })
        }
        (candidate @ Some(_), None) | (None, candidate @ Some(_)) => candidate,
        (None, None) => None,
    }
}

fn maximum_candidate<T: Copy, S: Copy + PartialOrd>(
    left_candidate: Option<ScoreCandidate<T, S>>,
    right_candidate: Option<ScoreCandidate<T, S>>,
) -> Option<ScoreCandidate<T, S>> {
    match (left_candidate, right_candidate) {
        (Some(left_candidate), Some(right_candidate)) => {
            Some(if left_candidate.score >= right_candidate.score { left_candidate } else { right_candidate })
        }
        (candidate @ Some(_), None) | (None, candidate @ Some(_)) => candidate,
        (None, None) => None,
    }
}

/// Append-only segment tree keyed by a caller-defined leaf identifier.
///
/// Tracks extremal scores in two buckets:
/// positive bucket (minimum score) and negative bucket (maximum score).
///
/// The tree supports prefix range adds and extraction of threshold crossings:
/// - positive score dropping below 0
/// - negative score rising to at least 0
///
/// Bucket memberships may temporarily disagree with scores while threshold crossings are
/// pending; extraction exposes those crossings and flipping restores the bucket invariant.
pub struct AppendableSegmentTree<T, S = i64> {
    len: usize,
    leaf_capacity: usize,
    /// Stable logical positions, independent of the current heap layout in `nodes`.
    position_by_leaf: HashMap<T, LeafPosition>,
    nodes: Vec<BucketExtrema<T, S>>,
}

impl<T, S> AppendableSegmentTree<T, S>
where
    T: Copy + Eq + Hash,
    S: Copy + PartialOrd + AddAssign + Sub<Output = S> + Zero,
{
    // ---------------------------------------------------------------------
    // Public API
    // ---------------------------------------------------------------------

    pub fn new() -> Self {
        Self::with_initial_capacity(DEFAULT_INITIAL_CAPACITY)
    }

    /// Creates an empty tree with room for at least `initial_capacity` leaves.
    /// The internal capacity is rounded up to the next power of two.
    pub fn with_initial_capacity(initial_capacity: usize) -> Self {
        assert!(initial_capacity > 0, "initial capacity must be positive");
        let leaf_capacity = initial_capacity.checked_next_power_of_two().expect("initial capacity is too large");
        let node_count = leaf_capacity.checked_mul(2).expect("initial capacity is too large");
        Self { len: 0, leaf_capacity, position_by_leaf: HashMap::new(), nodes: vec![BucketExtrema::empty(); node_count] }
    }

    /// Append a new leaf while preserving any crossings produced by earlier updates.
    /// The initial bucket is inferred from `initial_score`.
    pub fn append_leaf(&mut self, leaf: T, initial_score: S) {
        self.append_leaf_with_bucket(leaf, initial_score, bucket_for_score(initial_score));
    }

    /// Restore a leaf whose bucket may temporarily disagree with its score.
    pub(crate) fn append_leaf_with_bucket(&mut self, leaf: T, initial_score: S, bucket: Bucket) {
        assert!(!self.position_by_leaf.contains_key(&leaf), "leaf already present");
        if self.len == self.leaf_capacity {
            self.grow();
        }

        let position = self.len;
        self.len += 1;
        self.position_by_leaf.insert(leaf, position);

        let node = self.leaf_node(position);
        self.nodes[node] = BucketExtrema::restore_leaf_with_bucket(leaf, initial_score, bucket);
        self.recompute_ancestors_of(node);
    }

    pub(crate) fn bucket(&self, leaf: T) -> Bucket {
        self.bucketed_leaf_candidate_at(self.position_of(leaf)).1
    }

    pub fn prefix_add(&mut self, prefix_length: usize, delta: S) {
        self.range_add(0..prefix_length, delta);
    }

    /// Add `delta` to the half-open range of logical leaf positions.
    pub fn range_add(&mut self, update_range: Range<LeafPosition>, delta: S) {
        // TODO(relaxed): avoid code duplication by calling range_add_batch to implement this.
        // Current version uses direct implementation for easier reviewability
        assert!(update_range.start <= update_range.end, "range start exceeds range end");
        assert!(update_range.end <= self.len, "range exceeds tree length");
        if update_range.is_empty() || delta.is_zero() {
            return;
        }
        self.add_to_range(ROOT_NODE, self.full_leaf_range(), &update_range, delta);
    }

    /// Add independent deltas to several ranges in one tree traversal.
    pub fn range_add_batch(&mut self, ranges: &[(Range<LeafPosition>, S)]) {
        for (range, _) in ranges {
            assert!(range.start <= range.end, "range start exceeds range end");
            assert!(range.end <= self.len, "range exceeds tree length");
        }
        if ranges.is_empty() {
            return;
        }
        if ranges.len() == 1 {
            // Handle common case without superflous logic
            let (range, delta) = ranges[0].clone();
            self.range_add(range, delta);
            return;
        }
        let coalesced_ranges = coalesce_ranges(ranges);
        self.add_to_ranges(ROOT_NODE, self.full_leaf_range(), &coalesced_ranges);
    }

    pub fn has_positive_below_zero(&self) -> bool {
        self.root().min_positive.is_some_and(|candidate| candidate.score < S::zero())
    }

    pub fn has_negative_at_least_zero(&self) -> bool {
        self.root().max_negative.is_some_and(|candidate| candidate.score >= S::zero())
    }

    pub fn has_negative_score_in_prefix(&mut self, prefix_length: usize) -> bool {
        assert!(prefix_length <= self.len, "prefix exceeds tree length");
        self.has_negative_score_in_range(ROOT_NODE, self.full_leaf_range(), 0..prefix_length)
    }

    pub fn extract_positive_below_zero(&self) -> Option<T> {
        self.root().min_positive.filter(|candidate| candidate.score < S::zero()).map(|candidate| candidate.leaf)
    }

    pub fn extract_negative_at_least_zero(&self) -> Option<T> {
        self.root().max_negative.filter(|candidate| candidate.score >= S::zero()).map(|candidate| candidate.leaf)
    }

    pub fn extract_positive_below_zero_batch(&mut self) -> Vec<T> {
        let mut leaves = Vec::new();
        self.collect_positive_below_zero(ROOT_NODE, self.full_leaf_range(), &mut leaves);
        leaves
    }

    pub fn extract_negative_at_least_zero_batch(&mut self) -> Vec<T> {
        let mut leaves = Vec::new();
        self.collect_negative_at_least_zero(ROOT_NODE, self.full_leaf_range(), &mut leaves);
        leaves
    }

    pub fn flip_to_negative(&mut self, leaf: T) {
        self.set_bucket(leaf, Bucket::Negative);
    }

    pub fn flip_to_positive(&mut self, leaf: T) {
        self.set_bucket(leaf, Bucket::Positive);
    }

    pub fn score(&mut self, leaf: T) -> S {
        let target_position = self.position_of(leaf);
        // The position identifies the physical leaf directly, but its score may not yet include
        // lazy deltas stored by ancestors. Descend from the root to propagate those deltas first.
        self.point_score(ROOT_NODE, self.full_leaf_range(), target_position)
    }

    /// Returns all leaves in left-to-right (position) order.
    pub fn leaves(&self) -> Vec<T> {
        (0..self.len)
            .map(|pos| {
                let leaf = &self.nodes[self.leaf_node(pos)];
                if let Some(candidate) = leaf.min_positive {
                    candidate.leaf
                } else {
                    leaf.max_negative.expect("occupied leaf must have a candidate").leaf
                }
            })
            .collect()
    }

    /// Removes `leaf` if it is the most recently appended leaf.
    pub fn remove_head(&mut self, leaf: T) -> bool {
        let Some(&position) = self.position_by_leaf.get(&leaf) else {
            return false;
        };
        if position + 1 != self.len {
            return false;
        }

        self.remove_position(ROOT_NODE, self.full_leaf_range(), position);
        self.position_by_leaf.remove(&leaf);
        self.len -= 1;
        true
    }

    // ---------------------------------------------------------------------
    // Internal queries and bucket transitions
    // ---------------------------------------------------------------------

    fn has_negative_score_in_range(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, query: Range<LeafPosition>) -> bool {
        if ranges_are_disjoint(&node_range, &query) {
            return false;
        }
        if range_fully_contains(&query, &node_range) {
            return self.nodes[node].min_score.is_some_and(|candidate| candidate.score < S::zero());
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.has_negative_score_in_range(left_child(node), left_child_range, query.clone())
            || self.has_negative_score_in_range(right_child(node), right_child_range, query)
    }

    fn point_score(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, target_position: LeafPosition) -> S {
        if node_range.len() == 1 {
            debug_assert_eq!(node_range.start, target_position);
            debug_assert_eq!(node, self.leaf_node(target_position));
            self.bucketed_leaf_candidate_at(target_position).0.score
        } else {
            self.push_pending_delta(node);
            let (left_child_range, right_child_range) = split_range(&node_range);
            if left_child_range.contains(&target_position) {
                self.point_score(left_child(node), left_child_range, target_position)
            } else {
                self.point_score(right_child(node), right_child_range, target_position)
            }
        }
    }

    fn set_bucket(&mut self, leaf: T, new_bucket: Bucket) {
        let target_position = self.position_of(leaf);
        self.set_bucket_at_position(ROOT_NODE, self.full_leaf_range(), target_position, new_bucket);
    }

    fn collect_positive_below_zero(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, leaves: &mut Vec<T>) {
        let Some(candidate) = self.nodes[node].min_positive else {
            return;
        };
        if candidate.score >= S::zero() {
            return;
        }
        if node_range.len() == 1 {
            leaves.push(candidate.leaf);
            return;
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.collect_positive_below_zero(left_child(node), left_child_range, leaves);
        self.collect_positive_below_zero(right_child(node), right_child_range, leaves);
    }

    fn collect_negative_at_least_zero(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, leaves: &mut Vec<T>) {
        let Some(candidate) = self.nodes[node].max_negative else {
            return;
        };
        if candidate.score < S::zero() {
            return;
        }
        if node_range.len() == 1 {
            leaves.push(candidate.leaf);
            return;
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.collect_negative_at_least_zero(left_child(node), left_child_range, leaves);
        self.collect_negative_at_least_zero(right_child(node), right_child_range, leaves);
    }

    fn remove_position(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, position: LeafPosition) {
        // TODO(relaxed): consider if it is worthwhile to shrink the back the tree following a removal
        // this is standard practice, but in the Dagknight reorg context, removals will likely be
        // followed by a corresponding number of aditions, hence it likely makes sense not to temporarily shrink
        // capacity
        if node_range.len() == 1 {
            debug_assert_eq!(node_range.start, position);
            self.nodes[node] = BucketExtrema::empty();
            return;
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        if left_child_range.contains(&position) {
            self.remove_position(left_child(node), left_child_range, position);
        } else {
            self.remove_position(right_child(node), right_child_range, position);
        }
        self.recompute_node(node);
    }

    fn set_bucket_at_position(
        &mut self,
        node: NodeIndex,
        node_range: Range<LeafPosition>,
        target_position: LeafPosition,
        new_bucket: Bucket,
    ) {
        if node_range.len() == 1 {
            debug_assert_eq!(node_range.start, target_position);
            debug_assert_eq!(node, self.leaf_node(target_position));
            let (candidate, current_bucket) = self.bucketed_leaf_candidate_at(target_position);
            assert_ne!(current_bucket, new_bucket, "leaf already belongs to the requested bucket");
            assert_eq!(new_bucket, bucket_for_score(candidate.score), "destination bucket does not match the leaf score");

            self.nodes[node] = BucketExtrema::create_leaf_with_score(candidate.leaf, candidate.score);
            return;
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        if left_child_range.contains(&target_position) {
            self.set_bucket_at_position(left_child(node), left_child_range, target_position, new_bucket);
        } else {
            self.set_bucket_at_position(right_child(node), right_child_range, target_position, new_bucket);
        }
        self.recompute_node(node);
    }

    // ---------------------------------------------------------------------
    // Range updates
    // ---------------------------------------------------------------------

    fn add_to_range(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, update_range: &Range<LeafPosition>, delta: S) {
        // No overlap: this subtree lies entirely outside the requested update,
        // so neither its summary nor its descendants need to change.
        if ranges_are_disjoint(&node_range, update_range) {
            return;
        }

        // Full coverage: update the subtree summary and store the delta lazily;
        // descendants will receive it only when a later operation visits them.
        if range_fully_contains(update_range, &node_range) {
            self.apply_delta_to_node(node, delta);
            return;
        }

        // Partial overlap: materialize this node's existing lazy delta before
        // visiting both children, then rebuild the summary from their results.
        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.add_to_range(left_child(node), left_child_range, update_range, delta);
        self.add_to_range(right_child(node), right_child_range, update_range, delta);
        self.recompute_node(node);
    }

    fn add_to_ranges(&mut self, node: NodeIndex, node_range: Range<LeafPosition>, ranges: &[(Range<LeafPosition>, S)]) {
        // Partition the updates for this subtree. Updates that fully cover the
        // subtree can be combined into one lazy delta; only partial updates
        // need to be passed down to the children.
        let mut partial = Vec::new();
        let mut full_delta = S::zero();
        for (range, delta) in ranges {
            // This update cannot affect the subtree, so discard it here.
            if ranges_are_disjoint(&node_range, range) || delta.is_zero() {
                continue;
            }

            // Multiple full-cover updates are accumulated and applied together
            // at this node, avoiding separate traversals for their ranges.
            if range_fully_contains(range, &node_range) {
                full_delta += *delta;
            } else {
                partial.push((range.clone(), *delta));
            }
        }

        // Apply all updates that cover this subtree before descending. The
        // regular lazy-propagation path carries this combined delta to leaves.
        if !full_delta.is_zero() {
            self.apply_delta_to_node(node, full_delta);
        }

        // No partial updates remain, so this subtree is fully updated.
        if partial.is_empty() {
            return;
        }

        // Partial updates require visiting both children and rebuilding this
        // node's summary from their updated values.
        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.add_to_ranges(left_child(node), left_child_range, &partial);
        self.add_to_ranges(right_child(node), right_child_range, &partial);
        self.recompute_node(node);
    }

    // ---------------------------------------------------------------------
    // Lazy propagation
    // ---------------------------------------------------------------------

    fn apply_delta_to_node(&mut self, node: NodeIndex, delta: S) {
        self.nodes[node].apply_delta(delta);
    }

    fn push_pending_delta(&mut self, node: NodeIndex) {
        let delta = self.nodes[node].pending_delta;
        if delta.is_zero() {
            return;
        }

        self.apply_delta_to_node(left_child(node), delta);
        self.apply_delta_to_node(right_child(node), delta);
        self.nodes[node].pending_delta = S::zero();
    }

    fn materialize_subtree_deltas(&mut self, node: NodeIndex, node_range: Range<LeafPosition>) {
        if node_range.len() == 1 {
            self.nodes[node].pending_delta = S::zero();
            return;
        }

        self.push_pending_delta(node);
        let (left_child_range, right_child_range) = split_range(&node_range);
        self.materialize_subtree_deltas(left_child(node), left_child_range);
        self.materialize_subtree_deltas(right_child(node), right_child_range);
        self.recompute_node(node);
    }

    // ---------------------------------------------------------------------
    // Tree maintenance and growth
    // ---------------------------------------------------------------------

    fn root(&self) -> &BucketExtrema<T, S> {
        &self.nodes[ROOT_NODE]
    }

    fn position_of(&self, leaf: T) -> LeafPosition {
        *self.position_by_leaf.get(&leaf).expect("leaf not in tree")
    }

    fn leaf_node(&self, position: LeafPosition) -> NodeIndex {
        self.leaf_capacity + position
    }

    /// Returns the candidate and current bucket stored at an occupied logical leaf.
    fn bucketed_leaf_candidate_at(&self, position: LeafPosition) -> (ScoreCandidate<T, S>, Bucket) {
        debug_assert!(position < self.len);
        let leaf = &self.nodes[self.leaf_node(position)];
        debug_assert!(leaf.min_positive.is_some() ^ leaf.max_negative.is_some(), "occupied leaf must belong to exactly one bucket");

        if let Some(candidate) = leaf.min_positive {
            (candidate, Bucket::Positive)
        } else {
            (leaf.max_negative.expect("occupied leaf must have a candidate"), Bucket::Negative)
        }
    }

    fn full_leaf_range(&self) -> Range<LeafPosition> {
        0..self.leaf_capacity
    }

    fn recompute_node(&mut self, node: NodeIndex) {
        self.nodes[node] = BucketExtrema::merge(&self.nodes[left_child(node)], &self.nodes[right_child(node)]);
    }

    fn recompute_ancestors_of(&mut self, mut node: NodeIndex) {
        while node > ROOT_NODE {
            node = parent(node);
            self.recompute_node(node);
        }
    }

    fn grow(&mut self) {
        let leaves = self.materialized_leaves();
        self.leaf_capacity = self.leaf_capacity.checked_mul(2).expect("segment tree capacity is too large");
        self.nodes = vec![BucketExtrema::empty(); 2 * self.leaf_capacity];

        for (position, candidate, bucket) in leaves {
            let node = self.leaf_node(position);
            self.nodes[node] = BucketExtrema::restore_leaf_with_bucket(candidate.leaf, candidate.score, bucket);
        }

        for node in (ROOT_NODE..self.leaf_capacity).rev() {
            self.recompute_node(node);
        }
    }

    /// Flush lazy updates and snapshot the occupied leaves with their stable logical positions.
    fn materialized_leaves(&mut self) -> Vec<(LeafPosition, ScoreCandidate<T, S>, Bucket)> {
        self.materialize_subtree_deltas(ROOT_NODE, self.full_leaf_range());
        self.position_by_leaf
            .iter()
            .map(|(&leaf, &position)| {
                let (candidate, bucket) = self.bucketed_leaf_candidate_at(position);
                debug_assert!(candidate.leaf == leaf);
                (position, candidate, bucket)
            })
            .collect()
    }
}

impl<T, S> Default for AppendableSegmentTree<T, S>
where
    T: Copy + Eq + Hash,
    S: Copy + PartialOrd + AddAssign + Sub<Output = S> + Zero,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T, S> AppendableSegmentTreeApi<T, S> for AppendableSegmentTree<T, S>
where
    T: Copy + Eq + Hash,
    S: Copy + PartialOrd + AddAssign + Sub<Output = S> + Zero,
{
    fn with_initial_capacity(initial_capacity: usize) -> Self
    where
        Self: Sized,
    {
        AppendableSegmentTree::with_initial_capacity(initial_capacity)
    }

    fn append_leaf(&mut self, leaf: T, initial_score: S) {
        AppendableSegmentTree::append_leaf(self, leaf, initial_score)
    }

    fn prefix_add(&mut self, prefix_length: usize, delta: S) {
        AppendableSegmentTree::prefix_add(self, prefix_length, delta)
    }

    fn range_add(&mut self, range: Range<usize>, delta: S) {
        AppendableSegmentTree::range_add(self, range, delta)
    }

    fn range_add_batch(&mut self, ranges: &[(Range<usize>, S)]) {
        AppendableSegmentTree::range_add_batch(self, ranges)
    }

    fn has_positive_below_zero(&self) -> bool {
        AppendableSegmentTree::has_positive_below_zero(self)
    }

    fn has_negative_at_least_zero(&self) -> bool {
        AppendableSegmentTree::has_negative_at_least_zero(self)
    }

    fn has_negative_score_in_prefix(&mut self, prefix_length: usize) -> bool {
        AppendableSegmentTree::has_negative_score_in_prefix(self, prefix_length)
    }

    fn extract_positive_below_zero(&self) -> Option<T> {
        AppendableSegmentTree::extract_positive_below_zero(self)
    }

    fn extract_negative_at_least_zero(&self) -> Option<T> {
        AppendableSegmentTree::extract_negative_at_least_zero(self)
    }

    fn extract_positive_below_zero_batch(&mut self) -> Vec<T> {
        AppendableSegmentTree::extract_positive_below_zero_batch(self)
    }

    fn extract_negative_at_least_zero_batch(&mut self) -> Vec<T> {
        AppendableSegmentTree::extract_negative_at_least_zero_batch(self)
    }

    fn flip_to_negative(&mut self, leaf: T) {
        AppendableSegmentTree::flip_to_negative(self, leaf)
    }

    fn flip_to_positive(&mut self, leaf: T) {
        AppendableSegmentTree::flip_to_positive(self, leaf)
    }

    fn score(&mut self, leaf: T) -> S {
        AppendableSegmentTree::score(self, leaf)
    }

    fn leaves(&self) -> Vec<T> {
        AppendableSegmentTree::leaves(self)
    }

    fn remove_head(&mut self, leaf: T) -> bool {
        AppendableSegmentTree::remove_head(self, leaf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_score() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, 5);
        tree.append_leaf(2, -3);
        tree.append_leaf(3, 0);

        assert_eq!(tree.score(1), 5);
        assert_eq!(tree.score(2), -3);
        assert_eq!(tree.score(3), 0);
    }

    #[test]
    fn test_prefix_add() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, 2);
        tree.append_leaf(2, 0);
        tree.append_leaf(3, -1);
        tree.append_leaf(4, -3);
        tree.append_leaf(5, 4);

        tree.prefix_add(4, -1);

        assert_eq!(tree.score(1), 1);
        assert_eq!(tree.score(2), -1);
        assert_eq!(tree.score(3), -2);
        assert_eq!(tree.score(4), -4);
        assert_eq!(tree.score(5), 4);
    }

    #[test]
    fn test_range_add() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        for leaf in 0..5 {
            tree.append_leaf(leaf, leaf as i64);
        }

        tree.range_add(1..4, 10);

        assert_eq!(tree.score(0), 0);
        assert_eq!(tree.score(1), 11);
        assert_eq!(tree.score(2), 12);
        assert_eq!(tree.score(3), 13);
        assert_eq!(tree.score(4), 4);
    }

    #[test]
    fn test_crossing_detection_and_flip() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, 2);
        tree.append_leaf(2, 0);
        tree.append_leaf(3, -1);

        tree.prefix_add(3, -1);

        assert!(tree.has_positive_below_zero());
        let bad = tree.extract_positive_below_zero().unwrap();
        assert_eq!(bad, 2);

        tree.flip_to_negative(bad);

        assert!(!tree.has_positive_below_zero());
        assert!(!tree.has_negative_at_least_zero());
    }

    #[test]
    fn test_negative_flips_to_positive() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, -1);
        tree.append_leaf(2, -2);

        tree.prefix_add(2, 3);

        assert!(tree.has_negative_at_least_zero());
        let good = tree.extract_negative_at_least_zero().unwrap();
        assert_eq!(good, 1);

        tree.flip_to_positive(good);

        assert!(tree.has_negative_at_least_zero());
        assert!(!tree.has_positive_below_zero());

        let good2 = tree.extract_negative_at_least_zero().unwrap();
        assert_eq!(good2, 2);
        tree.flip_to_positive(good2);

        assert!(!tree.has_negative_at_least_zero());
        assert!(!tree.has_positive_below_zero());
    }

    #[test]
    fn test_grow_capacity() {
        let mut tree = AppendableSegmentTree::<u64>::with_initial_capacity(4);
        for i in 0..5u64 {
            tree.append_leaf(i, i as i64);
        }

        for i in 0..5u64 {
            assert_eq!(tree.score(i), i as i64);
        }

        tree.prefix_add(3, -2);
        assert_eq!(tree.score(0), -2);
        assert_eq!(tree.score(1), -1);
        assert_eq!(tree.score(2), 0);
        assert_eq!(tree.score(3), 3);
        assert_eq!(tree.score(4), 4);
    }

    #[test]
    fn test_prefix_add_zero() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, 5);
        tree.prefix_add(0, -100);
        assert_eq!(tree.score(1), 5);
    }

    #[test]
    fn test_grow_preserves_lazy_state() {
        let mut tree = AppendableSegmentTree::<u64>::with_initial_capacity(1);
        tree.append_leaf(1, 10);
        tree.prefix_add(1, 5);
        tree.append_leaf(2, 3);

        assert_eq!(tree.score(1), 15);
        assert_eq!(tree.score(2), 3);

        tree.prefix_add(2, -4);
        assert_eq!(tree.score(1), 11);
        assert_eq!(tree.score(2), -1);
        assert_eq!(tree.extract_positive_below_zero(), Some(2));
        tree.flip_to_negative(2);

        tree.append_leaf(3, 7);
        tree.append_leaf(4, -2);

        assert_eq!(tree.score(1), 11);
        assert_eq!(tree.score(2), -1);
        assert_eq!(tree.score(3), 7);
        assert_eq!(tree.score(4), -2);
    }

    #[test]
    fn test_append_infers_bucket_from_score() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, -5);
        tree.append_leaf(2, 0);

        assert!(!tree.has_negative_at_least_zero());
        assert!(!tree.has_positive_below_zero());
        assert_eq!(tree.score(1), -5);
        assert_eq!(tree.score(2), 0);
    }

    #[test]
    fn test_leaves_returns_in_position_order() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(10, 1);
        tree.append_leaf(20, 2);
        tree.append_leaf(30, 3);
        tree.append_leaf(40, 4);
        tree.append_leaf(50, 5);

        let leaves = tree.leaves();
        assert_eq!(leaves, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_leaves_after_prefix_add() {
        let mut tree = AppendableSegmentTree::<u64>::new();
        tree.append_leaf(1, 10);
        tree.append_leaf(2, 20);
        tree.append_leaf(3, 30);

        tree.prefix_add(2, -5);

        // Leaves should still be in position order regardless of score changes
        let leaves = tree.leaves();
        assert_eq!(leaves, vec![1, 2, 3]);

        // Verify scores are correct after prefix add
        assert_eq!(tree.score(1), 5);
        assert_eq!(tree.score(2), 15);
        assert_eq!(tree.score(3), 30);
    }

    #[test]
    fn test_leaves_empty_tree() {
        let tree = AppendableSegmentTree::<u64>::new();
        assert!(tree.leaves().is_empty());
    }
}
