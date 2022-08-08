use std::ops::Deref;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::model::stores::reachability::ReachabilityStoreReader;
use crate::model::Hash;
use crate::processes::reachability::inquirer;

pub trait ReachabilityService {
    fn is_chain_ancestor_of(&self, this: Hash, queried: Hash) -> bool;
    fn is_dag_ancestor_of(&self, this: Hash, queried: Hash) -> bool;
    fn get_next_chain_ancestor(&self, descendant: Hash, ancestor: Hash) -> Hash;
}

/// Single-thread reachability service imp
pub struct STReachabilityService<T: ReachabilityStoreReader> {
    store: T,
}

impl<T: ReachabilityStoreReader> STReachabilityService<T> {
    pub fn new(store: T) -> Self {
        Self { store }
    }
}

impl<T: ReachabilityStoreReader> ReachabilityService for STReachabilityService<T> {
    fn is_chain_ancestor_of(&self, this: Hash, queried: Hash) -> bool {
        inquirer::is_chain_ancestor_of(&self.store, this, queried).unwrap()
    }

    fn is_dag_ancestor_of(&self, this: Hash, queried: Hash) -> bool {
        inquirer::is_dag_ancestor_of(&self.store, this, queried).unwrap()
    }

    fn get_next_chain_ancestor(&self, descendant: Hash, ancestor: Hash) -> Hash {
        inquirer::get_next_chain_ancestor(&self.store, descendant, ancestor).unwrap()
    }
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
