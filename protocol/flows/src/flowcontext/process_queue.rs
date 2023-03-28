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

    pub fn len(&self) -> usize {
        self.deque.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
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

    pub fn drain(&mut self, chunk_size: usize) -> impl Iterator<Item = T> + '_ {
        self.deque.drain(0..chunk_size).filter(|x| self.set.remove(x))
    }
}
