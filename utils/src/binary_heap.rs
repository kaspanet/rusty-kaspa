use std::{cmp::Reverse, collections::BinaryHeap};

pub trait BinaryHeapExtensions<T> {
    fn into_sorted_iter(self) -> BinaryHeapIntoSortedIter<T>;
}

pub struct BinaryHeapIntoSortedIter<T> {
    binary_heap: BinaryHeap<T>,
}

impl<T: Ord> Iterator for BinaryHeapIntoSortedIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.binary_heap.pop()
    }
}

impl<T> BinaryHeapExtensions<T> for BinaryHeap<T> {
    fn into_sorted_iter(self) -> BinaryHeapIntoSortedIter<T> {
        BinaryHeapIntoSortedIter { binary_heap: self }
    }
}

/// Maintains the top `K` elements seen so far (by `Ord`), using a bounded min-heap.
///
/// Internally keeps a `BinaryHeap<Reverse<T>>` so `peek()` returns the current minimum
/// among the kept items.
#[derive(Default, Clone)]
pub struct TopK<T: Ord, const K: usize> {
    heap: BinaryHeap<Reverse<T>>,
}

impl<T: Ord, const K: usize> TopK<T, K> {
    pub fn new() -> Self {
        Self { heap: BinaryHeap::new() }
    }

    /// Pushes an item, keeping only the top `K` items seen so far.
    pub fn push(&mut self, item: T) {
        if self.heap.len() < K {
            self.heap.push(Reverse(item));
        } else if let Some(Reverse(min)) = self.heap.peek()
            && item > *min
        {
            self.heap.pop();
            self.heap.push(Reverse(item));
        }
    }

    /// Consumes `self` and returns an iterator yielding items in ascending order.
    ///
    /// This uses `BinaryHeapExtensions::into_sorted_iter`, which pops from the underlying heap.
    pub fn into_sorted_iter_ascending(self) -> impl Iterator<Item = T> {
        self.heap.into_sorted_iter().map(|Reverse(x)| x)
    }
}
