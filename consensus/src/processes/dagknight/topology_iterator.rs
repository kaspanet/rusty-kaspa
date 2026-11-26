use std::cmp::Reverse;
use std::collections::BinaryHeap;

use kaspa_consensus_core::{BlockHashSet, blockhash::ORIGIN};
use kaspa_hashes::Hash;

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{headers::HeaderStoreReader, relations::RelationsStoreReader},
};
use crate::processes::ghostdag::ordering::SortableBlock;

/// Optional skip predicate applied at child-expansion time. `None` skips nothing.
type SkipPredicate<'a> = Option<Box<dyn Fn(&Hash) -> bool + 'a>>;

/// Ascending (past-to-future) traversal: seeds `from`, keeps blocks in `to`'s past, expands children.
pub struct ForwardTraversal;
/// Descending (future-to-past) traversal: seeds `to`, keeps blocks in `from`'s future, expands parents.
pub struct BackwardTraversal;

/// Direction-specific behavior. Sealed: only [`ForwardTraversal`] and [`BackwardTraversal`] implement it, so the
/// direction is fixed at the type level and the traversal carries no runtime direction checks.
pub trait DirectionSealed<'a>: sealed::Sealed {
    type Heap;
    fn pop(heap: &mut Self::Heap) -> Option<SortableBlock>;
    /// Create the traversal heap seeded with the start blocks.
    fn seed<H: HeaderStoreReader>(from: &'a [Hash], to: &'a [Hash], pending: &mut BlockHashSet, headers: &H) -> Self::Heap;
    /// Whether `curr` lies within the region between `from` and `to`.
    fn is_in_region<R: ReachabilityService>(curr: Hash, from: &'a [Hash], to: &'a [Hash], reach: &R) -> bool;
    /// Enqueue `curr`'s neighbors in the traversal direction.
    fn expand<H: HeaderStoreReader, L: RelationsStoreReader>(
        curr: Hash,
        pending: &mut BlockHashSet,
        heap: &mut Self::Heap,
        relations: &L,
        headers: &H,
        skip: &SkipPredicate<'a>,
    );
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::ForwardTraversal {}
    impl Sealed for super::BackwardTraversal {}
}

impl<'a> DirectionSealed<'a> for ForwardTraversal {
    type Heap = BinaryHeap<Reverse<SortableBlock>>;
    fn pop(heap: &mut Self::Heap) -> Option<SortableBlock> {
        heap.pop().map(|Reverse(block)| block)
    }
    fn seed<H: HeaderStoreReader>(from: &'a [Hash], _to: &'a [Hash], pending: &mut BlockHashSet, headers: &H) -> Self::Heap {
        let mut heap = BinaryHeap::new();
        for &f in from {
            if pending.insert(f) {
                let blue_work = headers.get_header(f).unwrap().blue_work;
                heap.push(Reverse(SortableBlock { hash: f, blue_work }));
            }
        }
        heap
    }
    fn is_in_region<R: ReachabilityService>(curr: Hash, _from: &'a [Hash], to: &'a [Hash], reach: &R) -> bool {
        reach.is_dag_ancestor_of_any(curr, &mut to.iter().copied())
    }
    fn expand<H: HeaderStoreReader, L: RelationsStoreReader>(
        curr: Hash,
        pending: &mut BlockHashSet,
        heap: &mut Self::Heap,
        relations: &L,
        headers: &H,
        skip: &SkipPredicate<'a>,
    ) {
        for &c in relations.get_children(curr).unwrap().read().iter() {
            // Zone gate: skip children the predicate rejects, pruning their branch.
            if let Some(skip) = skip
                && skip(&c)
            {
                continue;
            }
            if pending.insert(c) {
                let blue_work = headers.get_header(c).unwrap().blue_work;
                heap.push(Reverse(SortableBlock { hash: c, blue_work }));
            }
        }
    }
}

impl<'a> DirectionSealed<'a> for BackwardTraversal {
    type Heap = BinaryHeap<SortableBlock>;
    fn pop(heap: &mut Self::Heap) -> Option<SortableBlock> {
        heap.pop()
    }
    fn seed<H: HeaderStoreReader>(_from: &'a [Hash], to: &'a [Hash], pending: &mut BlockHashSet, headers: &H) -> Self::Heap {
        let mut heap = BinaryHeap::new();
        for &t in to {
            if pending.insert(t) {
                let blue_work = headers.get_header(t).unwrap().blue_work;
                heap.push(SortableBlock { hash: t, blue_work });
            }
        }
        heap
    }
    fn is_in_region<R: ReachabilityService>(curr: Hash, from: &'a [Hash], _to: &'a [Hash], reach: &R) -> bool {
        from.iter().any(|&f| curr == f || reach.is_dag_ancestor_of(f, curr))
    }
    fn expand<H: HeaderStoreReader, L: RelationsStoreReader>(
        curr: Hash,
        pending: &mut BlockHashSet,
        heap: &mut Self::Heap,
        relations: &L,
        headers: &H,
        _skip: &SkipPredicate<'a>,
    ) {
        for &p in relations.get_parents(curr).unwrap().iter() {
            if p != ORIGIN && pending.insert(p) {
                let blue_work = headers.get_header(p).unwrap().blue_work;
                heap.push(SortableBlock { hash: p, blue_work });
            }
        }
    }
}

/// Lazily iterates the blocks in the region between `from` and `to`, in topological order.
/// [`TopologyIterator::new`] yields in ascending order; [`Self::reverse`] yields the same region in
/// reverse topological order.
///
/// `from` and `to` are sets of blocks and the region is everything in the future of `from` and in
/// the past of `to`. With one element each this is the closed interval between them; with several
/// `from` blocks it is the union of their futures and with several `to` blocks the union of their
/// pasts (a virtual endpoint). Every block in `to` must have a DAG ancestor in `from`, which
/// [`Self::new`] asserts. The end blocks are yielded; filter them out for an open interval.
///
/// The heap is backed by [`SortableBlock`], so blocks come out in topological order and a branch
/// is pruned once it leaves the region. Use [`Self::skip`] to prune branches at child-expansion
/// time, e.g. to keep a committed-search traversal within the chain descendants of the root.
pub struct TopologyIterator<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService, D: DirectionSealed<'a>> {
    heap: D::Heap,
    /// Tracks the blocks currently in the heap. A block leaves it the moment it is popped, so the
    /// set stays bounded by the traversal frontier rather than growing to the whole region.
    pending_blocks: BlockHashSet,
    /// The start blocks. A block is kept if it is a DAG descendant of at least one of them.
    from: &'a [Hash],
    /// The end blocks (tips). A block is kept if it is a DAG ancestor of at least one of them.
    to: &'a [Hash],
    headers_store: &'a H,
    relations_store: &'a L,
    reachability_service: &'a R,
    /// Optional skip predicate applied at child-expansion time (ForwardTraversal). `None` skips nothing.
    skip: SkipPredicate<'a>,
}

impl<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService> TopologyIterator<'a, H, L, R, ForwardTraversal> {
    /// Creates an iterator over the region between `from` and `to` in ascending topological order.
    /// `from` and `to` are sets of blocks; a single element in each yields the closed interval
    /// between them.
    pub fn new(from: &'a [Hash], to: &'a [Hash], headers_store: &'a H, relations_store: &'a L, reachability_service: &'a R) -> Self {
        assert!(
            to.iter().all(|&t| from.iter().any(|&f| reachability_service.is_dag_ancestor_of(f, t))),
            "TopologyIterator requires every block in `to` to have a DAG ancestor in `from`"
        );
        let mut pending_blocks = BlockHashSet::default();
        let heap = ForwardTraversal::seed(from, to, &mut pending_blocks, headers_store);
        TopologyIterator { heap, pending_blocks, from, to, headers_store, relations_store, reachability_service, skip: None }
    }

    /// Returns an iterator over the same region in the opposite (reverse topological) order.
    /// Consumes `self`. The skip predicate (if any) is carried over.
    pub fn reverse(self) -> TopologyIterator<'a, H, L, R, BackwardTraversal> {
        let mut pending_blocks = BlockHashSet::default();
        let heap = BackwardTraversal::seed(self.from, self.to, &mut pending_blocks, self.headers_store);
        TopologyIterator {
            heap,
            pending_blocks,
            from: self.from,
            to: self.to,
            headers_store: self.headers_store,
            relations_store: self.relations_store,
            reachability_service: self.reachability_service,
            skip: self.skip,
        }
    }

    /// Adds a skip predicate applied at child-expansion time (ForwardTraversal direction only). A child for
    /// which the predicate returns `true` is not pushed onto the heap, so it is never yielded and
    /// its descendants are never visited. The default (`None`) skips nothing. Consumes `self`.
    pub fn skip<F: Fn(&Hash) -> bool + 'a>(mut self, f: F) -> Self {
        self.skip = Some(Box::new(f));
        self
    }
}

impl<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService, D: DirectionSealed<'a>> Iterator
    for TopologyIterator<'a, H, L, R, D>
{
    type Item = Hash;

    fn next(&mut self) -> Option<Hash> {
        loop {
            let SortableBlock { hash: curr, .. } = D::pop(&mut self.heap)?;
            // Every neighbor that could enqueue curr has already been processed by topological
            // order, so curr will not be enqueued again and can be forgotten here.
            self.pending_blocks.remove(&curr);
            if !D::is_in_region(curr, self.from, self.to, self.reachability_service) {
                // This block is not within the inclusive region between `from` and `to` so skip it
                continue;
            }
            D::expand(curr, &mut self.pending_blocks, &mut self.heap, self.relations_store, self.headers_store, &self.skip);
            return Some(curr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stores::{headers::MemoryHeaderStore, reachability::MemoryReachabilityStore, relations::MemoryRelationsStore};
    use crate::processes::reachability::tests::{DagBlock, DagBuilder};
    use kaspa_consensus_core::blockhash::ORIGIN;
    use kaspa_consensus_core::header::Header;
    use kaspa_math::Uint192;
    use std::sync::Arc;

    /// Builds a DAG from `(hash, parents, blue_work)` triples and returns the stores/services the
    /// iterator needs.
    fn build_dag(blocks: &[(Hash, Vec<Hash>, u64)]) -> (MemoryHeaderStore, MemoryRelationsStore, MemoryReachabilityStore) {
        let mut reachability = MemoryReachabilityStore::new();
        let mut relations = MemoryRelationsStore::new();
        let headers_store = MemoryHeaderStore::new();

        {
            let mut builder = DagBuilder::new(&mut reachability, &mut relations);
            builder.init();
            for (hash, parents, _) in blocks {
                builder.add_block(DagBlock::new(*hash, parents.clone()));
            }
        }

        for (hash, parents, blue_work) in blocks {
            let mut header = Header::from_precomputed_hash(*hash, parents.clone());
            header.blue_work = Uint192::from_u64(*blue_work);
            headers_store.insert(Arc::new(header));
        }

        (headers_store, relations, reachability)
    }

    fn to_hash(id: u64) -> Hash {
        id.into()
    }

    ///        A (1)
    ///       / \
    ///      B(2) C(3)
    ///       \   /    \
    ///        D(4)    G(7)
    ///         |       |
    ///        E(5)    H(8)
    ///         |
    ///        F(6)
    ///
    /// Interval `[A, F]` = {A, B, C, D, E, F}. G, H are outside the interval (pruned). D has two
    /// parents (B, C), which exercises dedup.
    fn diamond_blocks() -> Vec<(Hash, Vec<Hash>, u64)> {
        vec![
            (to_hash(1), vec![ORIGIN], 1),                 // A
            (to_hash(2), vec![to_hash(1)], 2),             // B
            (to_hash(3), vec![to_hash(1)], 3),             // C
            (to_hash(4), vec![to_hash(2), to_hash(3)], 4), // D
            (to_hash(5), vec![to_hash(4)], 5),             // E
            (to_hash(6), vec![to_hash(5)], 6),             // F
            (to_hash(7), vec![to_hash(3)], 7),             // G (outside interval)
            (to_hash(8), vec![to_hash(7)], 8),             // H (outside interval)
        ]
    }

    #[test]
    fn test_from_to_yields_interval() {
        let (headers, relations, reach) = build_dag(&diamond_blocks());
        let from = [to_hash(1)];
        let to = [to_hash(6)];

        // FromTo iteration
        let iter = TopologyIterator::new(&from, &to, &headers, &relations, &reach);
        let result: Vec<Hash> = iter.collect();
        let expected: Vec<Hash> = [1, 2, 3, 4, 5, 6].map(to_hash).to_vec();
        assert_eq!(result, expected, "FromTo should yield [A,B,C,D,E,F] ascending; G,H pruned; D deduped");

        // ToFrom iteration
        let iter = TopologyIterator::new(&from, &to, &headers, &relations, &reach);
        let result: Vec<Hash> = iter.reverse().collect();
        let expected: Vec<Hash> = [6, 5, 4, 3, 2, 1].map(to_hash).to_vec();
        assert_eq!(result, expected, "ToFrom should yield [F,E,D,C,B,A] descending; ORIGIN excluded");
    }

    ///        A (1)
    ///       / \
    ///      B   C        B and C have EQUAL blue_work (2).
    ///      (2) (2)
    ///       \ /
    ///        D (3)
    ///
    /// FromTo breaks the B/C tie by smaller hash first (B before C).
    /// ToFrom breaks the B/C tie by larger hash first (C before B).
    fn tie_blocks() -> Vec<(Hash, Vec<Hash>, u64)> {
        vec![
            (to_hash(1), vec![ORIGIN], 1),                 // A
            (to_hash(2), vec![to_hash(1)], 2),             // B
            (to_hash(3), vec![to_hash(1)], 2),             // C (equal blue_work to B)
            (to_hash(4), vec![to_hash(2), to_hash(3)], 3), // D
        ]
    }

    #[test]
    fn test_tie_break_based_on_hash() {
        // FromTo smaller hash first
        let (headers, relations, reach) = build_dag(&tie_blocks());
        let result: Vec<Hash> = TopologyIterator::new(&[to_hash(1)], &[to_hash(4)], &headers, &relations, &reach).collect();
        assert_eq!(
            result,
            vec![to_hash(1), to_hash(2), to_hash(3), to_hash(4)],
            "FromTo breaks the B/C blue_work tie by smaller hash first (B before C)"
        );

        // ToFrom larger hash first
        let (headers, relations, reach) = build_dag(&tie_blocks());
        let result: Vec<Hash> = TopologyIterator::new(&[to_hash(1)], &[to_hash(4)], &headers, &relations, &reach).reverse().collect();
        assert_eq!(
            result,
            vec![to_hash(4), to_hash(3), to_hash(2), to_hash(1)],
            "ToFrom breaks the B/C blue_work tie by larger hash first (C before B)"
        );
    }

    /// `from` must be a strict DAG ancestor of `to`; otherwise `new` panics.
    #[should_panic]
    #[test]
    fn test_new_panics_when_from_not_ancestor() {
        let (headers, relations, reach) = build_dag(&tie_blocks());
        // D is not an ancestor of A (A is an ancestor of D).
        let _ = TopologyIterator::new(&[to_hash(4)], &[to_hash(1)], &headers, &relations, &reach);
    }
}
