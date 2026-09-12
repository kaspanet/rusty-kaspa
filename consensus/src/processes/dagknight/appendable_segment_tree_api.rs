use std::ops::{AddAssign, Range, Sub};

use num_traits::Zero;

pub(super) const DEFAULT_INITIAL_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bucket {
    Positive,
    Negative,
}

/// Returns the canonical initial bucket for a score.
///
/// A tree leaf can temporarily be stored in the opposite bucket while a threshold
/// crossing is pending processing, so this function does not describe every leaf's
/// current stored bucket.
pub fn bucket_for_score<S: PartialOrd + Zero>(score: S) -> Bucket {
    if score >= S::zero() { Bucket::Positive } else { Bucket::Negative }
}

/// Public API for the appendable segment tree used by UMC cascade.
///
/// Runtime characteristics (where n is current tree size):
/// - `new`: O(1)
/// - `with_initial_capacity`: O(initial capacity)
/// - `bucket_for_score`: O(1)
/// - `append_leaf`: amortized O(log n) due to occasional growth and ancestor rebuild
/// - `prefix_add`: O(log n)
/// - `range_add`: O(log n)
/// - `range_add_batch`: O(d log(n/d)) for `d` disjoint ranges
/// - `has_positive_below_zero`: O(1)
/// - `has_negative_at_least_zero`: O(1)
/// - `has_negative_score_in_prefix`: O(log n)
/// - `extract_positive_below_zero`: O(1)
/// - `extract_negative_at_least_zero`: O(1)
/// - `flip_to_negative`: O(log n)
/// - `flip_to_positive`: O(log n)
/// - `score`: O(log n)
/// - `remove_head`: O(log n)
pub trait AppendableSegmentTreeApi<T, S = i64>
where
    S: Copy + PartialOrd + AddAssign + Sub<Output = S> + Zero,
{
    fn new() -> Self
    where
        Self: Sized,
    {
        Self::with_initial_capacity(DEFAULT_INITIAL_CAPACITY)
    }

    fn with_initial_capacity(initial_capacity: usize) -> Self
    where
        Self: Sized;

    /// Appends a leaf while preserving any threshold crossings already pending in the tree.
    fn append_leaf(&mut self, leaf: T, initial_score: S);
    fn prefix_add(&mut self, prefix_length: usize, delta: S);
    fn range_add(&mut self, range: Range<usize>, delta: S);
    fn range_add_batch(&mut self, ranges: &[(Range<usize>, S)]);

    fn has_positive_below_zero(&self) -> bool;
    fn has_negative_at_least_zero(&self) -> bool;
    /// Whether any leaf in the prefix has a negative score, regardless of bucket membership.
    ///
    /// This may materialize lazy deltas along visited paths, hence the mutable receiver.
    fn has_negative_score_in_prefix(&mut self, prefix_length: usize) -> bool;

    fn extract_positive_below_zero(&self) -> Option<T>;
    fn extract_negative_at_least_zero(&self) -> Option<T>;
    fn extract_positive_below_zero_batch(&mut self) -> Vec<T>;
    fn extract_negative_at_least_zero_batch(&mut self) -> Vec<T>;

    fn flip_to_negative(&mut self, leaf: T);
    fn flip_to_positive(&mut self, leaf: T);

    fn score(&mut self, leaf: T) -> S;

    /// Returns all leaves in left-to-right (position) order.
    fn leaves(&self) -> Vec<T>;

    /// Removes `leaf` when it is the most recently appended leaf.
    ///
    /// Returns `true` if the tree head was removed, and `false` otherwise.
    fn remove_head(&mut self, leaf: T) -> bool;
}
