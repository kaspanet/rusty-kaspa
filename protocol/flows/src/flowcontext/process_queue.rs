use std::collections::{HashSet, VecDeque};

/// A simple deque backed by a set for efficient duplication filtering
pub struct ProcessQueue<T: Copy + PartialEq + Eq + std::hash::Hash> {
    deque: VecDeque<T>,
    set: HashSet<T>,
}

impl<T: Copy + PartialEq + Eq + std::hash::Hash> ProcessQueue<T> {
    pub fn new() -> Self {
        Self::from(HashSet::default())
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { deque: VecDeque::with_capacity(capacity), set: HashSet::with_capacity(capacity) }
    }

    pub fn len(&self) -> usize {
        self.deque.len()
    }

    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    pub fn from(set: HashSet<T>) -> Self {
        Self { deque: set.iter().copied().collect(), set }
    }

    pub fn enqueue_chunk<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            if self.set.insert(item) {
                self.deque.push_back(item);
            }
        }
    }

    pub fn dequeue(&mut self) -> Option<T> {
        if let Some(item) = self.deque.pop_front() {
            self.set.remove(&item);
            Some(item)
        } else {
            None
        }
    }

    pub fn dequeue_chunk(&mut self, max_chunk_size: usize) -> impl ExactSizeIterator<Item = T> + '_ {
        self.deque.drain(0..max_chunk_size.min(self.deque.len())).inspect(|x| assert!(self.set.remove(x)))
    }
}

impl<T: Copy + PartialEq + Eq + std::hash::Hash> IntoIterator for ProcessQueue<T> {
    type Item = T;
    type IntoIter = <std::collections::VecDeque<T> as std::iter::IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.deque.into_iter()
    }
}

impl<T: Copy + PartialEq + Eq + std::hash::Hash> FromIterator<T> for ProcessQueue<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let iter = iter.into_iter();
        let mut q = Self::with_capacity(iter.size_hint().0);
        q.enqueue_chunk(iter);
        q
    }
}

#[cfg(test)]
mod tests {
    use super::ProcessQueue;
    use itertools::Itertools;

    #[test]
    fn test_process_queue() {
        let mut q = ProcessQueue::new();
        q.enqueue_chunk([1, 2, 3, 4, 5, 5, 6]);
        assert_eq!(q.len(), 6);
        assert_eq!(q.dequeue(), Some(1));
        assert_eq!(q.dequeue_chunk(2).collect_vec(), vec![2, 3]);
        assert_eq!(q.len(), 3);
        assert_eq!(q.dequeue_chunk(10).collect_vec(), vec![4, 5, 6]);
        assert!(q.is_empty());
        q.enqueue_chunk([7, 8, 7]);
        assert_eq!(q.into_iter().collect_vec(), vec![7, 8]);
    }
}
