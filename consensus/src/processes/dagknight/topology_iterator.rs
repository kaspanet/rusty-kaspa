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

/// Direction of topology iteration between two blocks. This is internal; the public API exposes
/// `FromTo` via [`TopologyIterator::new`] and `ToFrom` via [`TopologyIterator::reverse`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Direction {
    /// `from` to `to`: ascending topological order.
    FromTo,
    /// `to` to `from`: descending topological order.
    ToFrom,
}

impl Direction {
    fn opposite(self) -> Direction {
        match self {
            Direction::FromTo => Direction::ToFrom,
            Direction::ToFrom => Direction::FromTo,
        }
    }
}

/// The traversal heap, backed by [`SortableBlock`]. The direction picks the pop order.
enum TopoHeap {
    /// Past to future: pops the smallest `(blue_work, hash)` first.
    Forward(BinaryHeap<Reverse<SortableBlock>>),
    /// Future to past: pops the largest `(blue_work, hash)` first.
    Backward(BinaryHeap<SortableBlock>),
}

impl TopoHeap {
    fn new(direction: Direction) -> Self {
        match direction {
            Direction::FromTo => TopoHeap::Forward(BinaryHeap::new()),
            Direction::ToFrom => TopoHeap::Backward(BinaryHeap::new()),
        }
    }

    fn push(&mut self, block: SortableBlock) {
        match self {
            TopoHeap::Forward(heap) => heap.push(Reverse(block)),
            TopoHeap::Backward(heap) => heap.push(block),
        }
    }

    fn pop(&mut self) -> Option<SortableBlock> {
        match self {
            TopoHeap::Forward(heap) => heap.pop().map(|Reverse(block)| block),
            TopoHeap::Backward(heap) => heap.pop(),
        }
    }
}

/// Lazily iterates the blocks in the region between `from` and `to`, in topological order.
/// Call [`Self::reverse`] to iterate the same region in reverse topological order.
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
pub struct TopologyIterator<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService> {
    heap: TopoHeap,
    seen: BlockHashSet,
    /// The start blocks. A block is kept if it is a DAG descendant of at least one of them.
    from: Vec<Hash>,
    /// The end blocks (tips). A block is kept if it is a DAG ancestor of at least one of them.
    to: Vec<Hash>,
    direction: Direction,
    headers_store: &'a H,
    relations_store: &'a L,
    reachability_service: &'a R,
    /// Optional skip predicate applied at child-expansion time (FromTo). `None` skips nothing.
    skip: SkipPredicate<'a>,
}

impl<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService> TopologyIterator<'a, H, L, R> {
    /// Creates an iterator over the region between `from` and `to` in ascending topological order.
    /// `from` and `to` are sets of blocks; a single element in each yields the closed interval
    /// between them.
    pub fn new(from: &[Hash], to: &[Hash], headers_store: &'a H, relations_store: &'a L, reachability_service: &'a R) -> Self {
        assert!(
            to.iter().all(|&t| from.iter().any(|&f| reachability_service.is_dag_ancestor_of(f, t))),
            "TopologyIterator requires every block in `to` to have a DAG ancestor in `from`"
        );
        Self::with_direction(from.to_vec(), to.to_vec(), Direction::FromTo, headers_store, relations_store, reachability_service, None)
    }

    /// Returns an iterator over the same region in the opposite (reverse topological) order.
    /// Consumes `self`. The skip predicate (if any) is carried over.
    pub fn reverse(self) -> Self {
        Self::with_direction(
            self.from,
            self.to,
            self.direction.opposite(),
            self.headers_store,
            self.relations_store,
            self.reachability_service,
            self.skip,
        )
    }

    /// Adds a skip predicate applied at child-expansion time (FromTo direction only). A child for
    /// which the predicate returns `true` is not pushed onto the heap, so it is never yielded and
    /// its descendants are never visited. The default (`None`) skips nothing. Consumes `self`.
    pub fn skip<F: Fn(&Hash) -> bool + 'a>(mut self, f: F) -> Self {
        self.skip = Some(Box::new(f));
        self
    }

    fn with_direction(
        from: Vec<Hash>,
        to: Vec<Hash>,
        direction: Direction,
        headers_store: &'a H,
        relations_store: &'a L,
        reachability_service: &'a R,
        skip: SkipPredicate<'a>,
    ) -> Self {
        let mut heap = TopoHeap::new(direction);
        let mut seen = BlockHashSet::default();
        // Seed the heap with the start blocks of the traversal.
        match direction {
            // FromTo seeds with every start block; those outside the region are pruned at pop time.
            Direction::FromTo => {
                for &f in &from {
                    if seen.insert(f) {
                        let blue_work = headers_store.get_header(f).unwrap().blue_work;
                        heap.push(SortableBlock { hash: f, blue_work });
                    }
                }
            }
            // ToFrom seeds with every end block; those outside `from`'s future are pruned at pop time.
            Direction::ToFrom => {
                for &t in &to {
                    if seen.insert(t) {
                        let blue_work = headers_store.get_header(t).unwrap().blue_work;
                        heap.push(SortableBlock { hash: t, blue_work });
                    }
                }
            }
        }
        Self { heap, seen, from, to, direction, headers_store, relations_store, reachability_service, skip }
    }

    /// Pushes `h` onto the direction's heap, keyed by its header blue_work.
    fn push_block(&mut self, h: Hash) {
        let blue_work = self.headers_store.get_header(h).unwrap().blue_work;
        self.heap.push(SortableBlock { hash: h, blue_work });
    }

    fn pop_block(&mut self) -> Option<SortableBlock> {
        self.heap.pop()
    }
}

impl<'a, H: HeaderStoreReader, L: RelationsStoreReader, R: ReachabilityService> Iterator for TopologyIterator<'a, H, L, R> {
    type Item = Hash;

    fn next(&mut self) -> Option<Hash> {
        loop {
            let SortableBlock { hash: curr, .. } = self.pop_block()?;
            let keep = match self.direction {
                Direction::FromTo => self.reachability_service.is_dag_ancestor_of_any(curr, &mut self.to.iter().copied()),
                Direction::ToFrom => self.from.iter().any(|&f| curr == f || self.reachability_service.is_dag_ancestor_of(f, curr)),
            };
            if !keep {
                // This block is not within the inclusive region between `from` and `to` so skip it
                continue;
            }

            match self.direction {
                Direction::FromTo => {
                    for &c in self.relations_store.get_children(curr).unwrap().read().iter() {
                        // Zone gate: skip children the predicate rejects, pruning their branch.
                        if let Some(skip) = &self.skip
                            && skip(&c)
                        {
                            continue;
                        }
                        if self.seen.insert(c) {
                            self.push_block(c);
                        }
                    }
                }
                Direction::ToFrom => {
                    for &p in self.relations_store.get_parents(curr).unwrap().iter() {
                        if p != ORIGIN && self.seen.insert(p) {
                            self.push_block(p);
                        }
                    }
                }
            }
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
    fn test_from_to_yields_interval_ascending() {
        let (headers, relations, reach) = build_dag(&diamond_blocks());
        let iter = TopologyIterator::new(&[to_hash(1)], &[to_hash(6)], &headers, &relations, &reach);
        let result: Vec<Hash> = iter.collect();
        let expected: Vec<Hash> = [1, 2, 3, 4, 5, 6].map(to_hash).to_vec();
        assert_eq!(result, expected, "FromTo should yield [A,B,C,D,E,F] ascending; G,H pruned; D deduped");
    }

    #[test]
    fn test_to_from_yields_interval_descending() {
        let (headers, relations, reach) = build_dag(&diamond_blocks());
        let iter = TopologyIterator::new(&[to_hash(1)], &[to_hash(6)], &headers, &relations, &reach).reverse();
        let result: Vec<Hash> = iter.collect();
        let expected: Vec<Hash> = [6, 5, 4, 3, 2, 1].map(to_hash).to_vec();
        assert_eq!(result, expected, "ToFrom should yield [F,E,D,C,B,A] descending; ORIGIN excluded");
    }

    /// A -> B -> C (simple chain).
    #[test]
    fn test_simple_chain_both_directions() {
        let blocks = vec![
            (to_hash(1), vec![ORIGIN], 1),     // A
            (to_hash(2), vec![to_hash(1)], 2), // B
            (to_hash(3), vec![to_hash(2)], 3), // C
        ];
        let (headers, relations, reach) = build_dag(&blocks);

        let from_to: Vec<Hash> = TopologyIterator::new(&[to_hash(1)], &[to_hash(3)], &headers, &relations, &reach).collect();
        assert_eq!(from_to, vec![to_hash(1), to_hash(2), to_hash(3)], "FromTo chain");

        let to_from: Vec<Hash> = TopologyIterator::new(&[to_hash(1)], &[to_hash(3)], &headers, &relations, &reach).reverse().collect();
        assert_eq!(to_from, vec![to_hash(3), to_hash(2), to_hash(1)], "ToFrom chain");
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
    fn test_tie_break_from_to_smaller_hash_first() {
        let (headers, relations, reach) = build_dag(&tie_blocks());
        let result: Vec<Hash> = TopologyIterator::new(&[to_hash(1)], &[to_hash(4)], &headers, &relations, &reach).collect();
        assert_eq!(
            result,
            vec![to_hash(1), to_hash(2), to_hash(3), to_hash(4)],
            "FromTo breaks the B/C blue_work tie by smaller hash first (B before C)"
        );
    }

    #[test]
    fn test_tie_break_to_from_larger_hash_first() {
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

    /// Virtual endpoint: `from = A`, tips = {F, H}. The region is the whole zone {A..H} (every
    /// block is a descendant of A and an ancestor of F or H).
    #[test]
    fn test_virtual_endpoint_from_to_ascending() {
        let (headers, relations, reach) = build_dag(&diamond_blocks());
        let tips = vec![to_hash(6), to_hash(8)]; // F, H
        let iter = TopologyIterator::new(&[to_hash(1)], &tips, &headers, &relations, &reach);
        let result: Vec<Hash> = iter.collect();
        let expected: Vec<Hash> = [1, 2, 3, 4, 5, 6, 7, 8].map(to_hash).to_vec();
        assert_eq!(result, expected, "Virtual FromTo should yield the whole zone [A..H] ascending");
    }

    #[test]
    fn test_virtual_endpoint_to_from_descending() {
        let (headers, relations, reach) = build_dag(&diamond_blocks());
        let tips = vec![to_hash(6), to_hash(8)]; // F, H
        let iter = TopologyIterator::new(&[to_hash(1)], &tips, &headers, &relations, &reach).reverse();
        let result: Vec<Hash> = iter.collect();
        let expected: Vec<Hash> = [8, 7, 6, 5, 4, 3, 2, 1].map(to_hash).to_vec();
        assert_eq!(result, expected, "Virtual ToFrom should yield the whole zone [H..A] descending");
    }
}
