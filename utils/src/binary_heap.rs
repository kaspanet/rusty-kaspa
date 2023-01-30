use std::collections::BinaryHeap;

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
