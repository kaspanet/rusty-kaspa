use std::ops::{Add, Range, Sub};

use num_traits::Zero;

type LeafPosition = usize;
type NodeIndex = usize;

/// Node relationships in the tree's one-based heap layout.
pub(super) fn left_child(node: NodeIndex) -> NodeIndex {
    node * 2
}

pub(super) fn right_child(node: NodeIndex) -> NodeIndex {
    node * 2 + 1
}

pub(super) fn parent(node: NodeIndex) -> NodeIndex {
    debug_assert!(node > 1, "root node has no parent");
    node / 2
}

#[repr(u8)]
#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum BoundaryKind {
    End = 0,
    Start = 1,
}

#[derive(Clone, Copy)]
struct RangeBoundary<S> {
    position: LeafPosition,
    kind: BoundaryKind,
    value: S,
}

impl<S> RangeBoundary<S> {
    fn start(position: LeafPosition, value: S) -> Self {
        Self { position, kind: BoundaryKind::Start, value }
    }

    fn end(position: LeafPosition, value: S) -> Self {
        Self { position, kind: BoundaryKind::End, value }
    }
}

/// Half-open ranges that only touch at a boundary are disjoint.
pub(super) fn ranges_are_disjoint(first: &Range<LeafPosition>, second: &Range<LeafPosition>) -> bool {
    first.end <= second.start || second.end <= first.start
}

/// Returns whether `outer` contains every position in `inner`.
pub(super) fn range_fully_contains(outer: &Range<LeafPosition>, inner: &Range<LeafPosition>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// Splits a non-leaf range into its two contiguous child ranges.
pub(super) fn split_range(range: &Range<LeafPosition>) -> (Range<LeafPosition>, Range<LeafPosition>) {
    debug_assert!(range.len() > 1, "cannot split a leaf range");
    let midpoint = range.start + range.len() / 2;
    (range.start..midpoint, midpoint..range.end)
}

pub(super) fn coalesce_ranges<S>(ranges: &[(Range<LeafPosition>, S)]) -> Vec<(Range<LeafPosition>, S)>
where
    S: Copy + Add<Output = S> + Sub<Output = S> + Zero,
{
    // Represent every range by a start event and an end event. The sweep
    // between two consecutive positions has one constant combined value.
    let mut boundary_events = Vec::with_capacity(ranges.len() * 2);
    for (range, value) in ranges {
        if !range.is_empty() && !value.is_zero() {
            boundary_events.push(RangeBoundary::start(range.start, *value));
            boundary_events.push(RangeBoundary::end(range.end, *value));
        }
    }
    // End events sort before start events at the same position, so an ended
    // range is removed before a new range beginning there is added.
    boundary_events.sort_unstable_by_key(|event| (event.position, event.kind));

    let mut coalesced_ranges = Vec::new();
    let mut active_value = S::zero();
    let mut previous_boundary = None;
    for event in boundary_events {
        if let Some(previous_boundary) = previous_boundary
            && previous_boundary < event.position
            && !active_value.is_zero()
        {
            coalesced_ranges.push((previous_boundary..event.position, active_value));
        }
        // Update the accumulated value for the interval starting at event_position.
        previous_boundary = Some(event.position);
        active_value = match event.kind {
            BoundaryKind::Start => active_value + event.value,
            BoundaryKind::End => active_value - event.value,
        };
    }
    coalesced_ranges
}
