use std::ops::Range;

pub(super) const DEFAULT_INITIAL_CAPACITY: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Bucket {
    Positive,
    Negative,
}

/// Assigns non-negative scores to the positive bucket and negative scores to the negative bucket.
pub fn bucket_for_score(score: i64) -> Bucket {
    if score >= 0 { Bucket::Positive } else { Bucket::Negative }
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
/// - `has_positive_below_zero`: O(1)
/// - `has_negative_at_least_zero`: O(1)
/// - `extract_positive_below_zero`: O(1)
/// - `extract_negative_at_least_zero`: O(1)
/// - `flip_to_negative`: O(log n)
/// - `flip_to_positive`: O(log n)
/// - `score`: O(log n)
pub trait AppendableSegmentTreeApi<T> {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self::with_initial_capacity(DEFAULT_INITIAL_CAPACITY)
    }

    fn with_initial_capacity(initial_capacity: usize) -> Self
    where
        Self: Sized;

    /// Appends a leaf after all threshold crossings produced by earlier updates have been consumed.
    /// Implementations may reject an append while an unconsumed crossing remains.
    fn append_leaf(&mut self, leaf: T, initial_score: i64);
    fn prefix_add(&mut self, prefix_length: usize, delta: i64);
    fn range_add(&mut self, range: Range<usize>, delta: i64);

    fn has_positive_below_zero(&self) -> bool;
    fn has_negative_at_least_zero(&self) -> bool;

    fn extract_positive_below_zero(&self) -> Option<T>;
    fn extract_negative_at_least_zero(&self) -> Option<T>;

    fn flip_to_negative(&mut self, leaf: T);
    fn flip_to_positive(&mut self, leaf: T);

    fn score(&mut self, leaf: T) -> i64;
}
