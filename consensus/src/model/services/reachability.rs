use std::ops::Deref;
use std::sync::Arc;

use consensus_core::blockhash;
use parking_lot::RwLock;

use crate::model::stores::reachability::ReachabilityStoreReader;
use crate::processes::reachability::{inquirer, ReachabilityError, Result};
use hashes::Hash;

pub trait ReachabilityService {
    fn is_chain_ancestor_of(&self, this: Hash, queried: Hash) -> bool;
    fn is_dag_ancestor_of(&self, this: Hash, queried: Hash) -> bool;
    fn get_next_chain_ancestor(&self, descendant: Hash, ancestor: Hash) -> Hash;
}

/// Multi-threaded reachability service imp
pub struct MTReachabilityService<T: ReachabilityStoreReader + ?Sized> {
    store: Arc<RwLock<T>>,
}

impl<T: ReachabilityStoreReader + ?Sized> MTReachabilityService<T> {
    pub fn new(store: Arc<RwLock<T>>) -> Self {
        Self { store }
    }
}

impl<T: ReachabilityStoreReader + ?Sized> ReachabilityService for MTReachabilityService<T> {
    fn is_chain_ancestor_of(&self, this: Hash, queried: Hash) -> bool {
        let read_guard = self.store.read();
        inquirer::is_chain_ancestor_of(read_guard.deref(), this, queried).unwrap()
    }

    fn is_dag_ancestor_of(&self, this: Hash, queried: Hash) -> bool {
        let read_guard = self.store.read();
        inquirer::is_dag_ancestor_of(read_guard.deref(), this, queried).unwrap()
    }

    fn get_next_chain_ancestor(&self, descendant: Hash, ancestor: Hash) -> Hash {
        let read_guard = self.store.read();
        inquirer::get_next_chain_ancestor(read_guard.deref(), descendant, ancestor).unwrap()
    }
}

impl<T: ReachabilityStoreReader + ?Sized> MTReachabilityService<T> {
    /// Returns a forward iterator walking up the chain-selection tree from `from_ancestor`
    /// to `to_descendant`, where `to_descendant` is included if `inclusive` is set to true.
    ///
    /// To skip `from_ancestor` simply apply `skip(1)`.
    ///
    /// The caller is expected to verify that `from_ancestor` is indeed a chain ancestor of
    /// `to_descendant`, otherwise an error will be returned.  
    pub fn forward_chain_iterator(
        &self, from_ancestor: Hash, to_descendant: Hash, inclusive: bool,
    ) -> impl Iterator<Item = Result<Hash>> {
        ForwardChainIterator::new(self.store.clone(), from_ancestor, to_descendant, inclusive)
    }

    /// Returns a backward iterator walking down the selected chain from `from_descendant`
    /// to `to_ancestor`, where `to_ancestor` is included if `inclusive` is set to true.
    ///
    /// To skip `from_descendant` simply apply `skip(1)`.
    ///
    /// The caller is expected to verify that `to_ancestor` is indeed a chain ancestor of
    /// `from_descendant`, otherwise the iterator will eventually return an error.  
    pub fn backward_chain_iterator(
        &self, from_descendant: Hash, to_ancestor: Hash, inclusive: bool,
    ) -> impl Iterator<Item = Result<Hash>> {
        BackwardChainIterator::new(self.store.clone(), from_descendant, to_ancestor, inclusive)
    }

    /// Returns the default chain iterator, walking from `from` backward down the
    /// selected chain until `virtual genesis` (aka `blockhash::ORIGIN`; exclusive)
    pub fn default_chain_iterator(&self, from: Hash) -> impl Iterator<Item = Result<Hash>> {
        BackwardChainIterator::new(self.store.clone(), from, blockhash::ORIGIN, false)
    }
}

/// Iterator design: we currently read-lock at each movement of the iterator.
/// Other options are to keep the read guard throughout the iterator lifetime, or
/// a compromise where the lock is released every constant number of items.
///
/// TODO: decide if these alternatives require overall system benchmarking

struct BackwardChainIterator<T: ReachabilityStoreReader + ?Sized> {
    store: Arc<RwLock<T>>,
    current: Option<Hash>,
    ancestor: Hash,
    inclusive: bool,
}

impl<T: ReachabilityStoreReader + ?Sized> BackwardChainIterator<T> {
    fn new(store: Arc<RwLock<T>>, from_descendant: Hash, to_ancestor: Hash, inclusive: bool) -> Self {
        Self { store, current: Some(from_descendant), ancestor: to_ancestor, inclusive }
    }
}

impl<T: ReachabilityStoreReader + ?Sized> Iterator for BackwardChainIterator<T> {
    type Item = Result<Hash>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current {
            if current == self.ancestor {
                if self.inclusive {
                    self.current = None;
                    Some(Ok(current))
                } else {
                    self.current = None;
                    None
                }
            } else {
                debug_assert_ne!(current, blockhash::NONE);
                match self.store.read().get_parent(current) {
                    Ok(next) => {
                        self.current = Some(next);
                        Some(Ok(current))
                    }
                    Err(e) => {
                        self.current = None;
                        Some(Err(ReachabilityError::StoreError(e)))
                    }
                }
            }
        } else {
            None
        }
    }
}

struct ForwardChainIterator<T: ReachabilityStoreReader + ?Sized> {
    store: Arc<RwLock<T>>,
    current: Option<Hash>,
    descendant: Hash,
    inclusive: bool,
}

impl<T: ReachabilityStoreReader + ?Sized> ForwardChainIterator<T> {
    fn new(store: Arc<RwLock<T>>, from_ancestor: Hash, to_descendant: Hash, inclusive: bool) -> Self {
        Self { store, current: Some(from_ancestor), descendant: to_descendant, inclusive }
    }
}

impl<T: ReachabilityStoreReader + ?Sized> Iterator for ForwardChainIterator<T> {
    type Item = Result<Hash>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(current) = self.current {
            if current == self.descendant {
                if self.inclusive {
                    self.current = None;
                    Some(Ok(current))
                } else {
                    self.current = None;
                    None
                }
            } else {
                match inquirer::get_next_chain_ancestor(self.store.read().deref(), self.descendant, current) {
                    Ok(next) => {
                        self.current = Some(next);
                        Some(Ok(current))
                    }
                    Err(e) => {
                        self.current = None;
                        Some(Err(e))
                    }
                }
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::stores::reachability::MemoryReachabilityStore,
        processes::reachability::{interval::Interval, tests::TreeBuilder},
    };

    #[test]
    fn test_forward_iterator() {
        // Arrange
        let mut store = MemoryReachabilityStore::new();

        // Act
        let root: Hash = 1.into();
        TreeBuilder::new(&mut store)
            .init_with_params(root, Interval::new(1, 15))
            .add_block(2.into(), root)
            .add_block(3.into(), 2.into())
            .add_block(4.into(), 2.into())
            .add_block(5.into(), 3.into())
            .add_block(6.into(), 5.into())
            .add_block(7.into(), 1.into())
            .add_block(8.into(), 6.into())
            .add_block(9.into(), 6.into())
            .add_block(10.into(), 6.into())
            .add_block(11.into(), 6.into());

        let service = MTReachabilityService::new(Arc::new(RwLock::new(store)));

        // Exclusive
        let iter = service.forward_chain_iterator(2.into(), 10.into(), false);

        // Assert
        let expected_hashes = [2u64, 3, 5, 6].map(Hash::from);
        assert!(expected_hashes
            .iter()
            .cloned()
            .eq(iter.map(|r| r.unwrap())));

        // Inclusive
        let iter = service.forward_chain_iterator(2.into(), 10.into(), true);

        // Assert
        let expected_hashes = [2u64, 3, 5, 6, 10].map(Hash::from);
        assert!(expected_hashes
            .iter()
            .cloned()
            .eq(iter.map(|r| r.unwrap())));

        // Compare backward to reversed forward
        let forward_iter = service
            .forward_chain_iterator(2.into(), 10.into(), true)
            .map(|r| r.unwrap());
        let backward_iter: Result<Vec<Hash>> = service
            .backward_chain_iterator(10.into(), 2.into(), true)
            .collect();
        assert!(forward_iter.eq(backward_iter.unwrap().iter().cloned().rev()))
    }

    #[test]
    fn test_iterator_boundaries() {
        // Arrange & Act
        let mut store = MemoryReachabilityStore::new();
        let root: Hash = 1.into();
        TreeBuilder::new(&mut store)
            .init_with_params(root, Interval::new(1, 5))
            .add_block(2.into(), root);

        let service = MTReachabilityService::new(Arc::new(RwLock::new(store)));

        // Asserts
        assert!([1u64, 2]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(service
                .forward_chain_iterator(1.into(), 2.into(), true)
                .map(|r| r.unwrap())));

        assert!([1u64].map(Hash::from).iter().cloned().eq(service
            .forward_chain_iterator(1.into(), 2.into(), false)
            .map(|r| r.unwrap())));

        assert!([2u64, 1]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(service
                .backward_chain_iterator(2.into(), root, true)
                .map(|r| r.unwrap())));

        assert!([2u64].map(Hash::from).iter().cloned().eq(service
            .backward_chain_iterator(2.into(), root, false)
            .map(|r| r.unwrap())));

        assert!(std::iter::once_with(|| root).eq(service
            .backward_chain_iterator(root, root, true)
            .map(|r| r.unwrap())));

        assert!(std::iter::empty::<Hash>().eq(service
            .backward_chain_iterator(root, root, false)
            .map(|r| r.unwrap())));
    }
}
