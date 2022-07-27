//!
//! Test utils for reachability
//!
use super::{inquirer::*, tree::*};
use crate::{
    model::{
        api::hash::Hash,
        stores::{errors::StoreError, reachability::ReachabilityStore},
    },
    processes::reachability::interval::Interval,
};
use std::collections::VecDeque;
use thiserror::Error;

/// A struct with fluent API to streamline reachability store building
pub struct StoreBuilder<'a> {
    store: &'a mut dyn ReachabilityStore,
}

impl<'a> StoreBuilder<'a> {
    pub fn new(store: &'a mut dyn ReachabilityStore) -> Self {
        Self { store }
    }

    pub fn add_block(&mut self, hash: Hash, parent: Hash) -> &mut Self {
        let parent_height = if !parent.is_zero() { self.store.append_child(parent, hash).unwrap() } else { 0 };
        self.store
            .insert(hash, parent, Interval::empty(), parent_height + 1)
            .unwrap();
        self
    }
}

/// A struct with fluent API to streamline tree building
pub struct TreeBuilder<'a> {
    store: &'a mut dyn ReachabilityStore,
    reindex_depth: u64,
    reindex_slack: u64,
}

impl<'a> TreeBuilder<'a> {
    pub fn new(store: &'a mut dyn ReachabilityStore) -> Self {
        Self {
            store,
            reindex_depth: crate::constants::perf::DEFAULT_REINDEX_DEPTH,
            reindex_slack: crate::constants::perf::DEFAULT_REINDEX_SLACK,
        }
    }

    pub fn new_with_params(store: &'a mut dyn ReachabilityStore, reindex_depth: u64, reindex_slack: u64) -> Self {
        Self { store, reindex_depth, reindex_slack }
    }

    pub fn init(&mut self, origin: Hash, capacity: Interval) -> &mut Self {
        init_with_params(self.store, origin, capacity).unwrap();
        self
    }

    pub fn add_block(&mut self, hash: Hash, parent: Hash) -> &mut Self {
        add_tree_block(self.store, hash, parent, self.reindex_depth, self.reindex_slack).unwrap();
        try_advancing_reindex_root(self.store, hash, self.reindex_depth, self.reindex_slack).unwrap();
        self
    }
}

#[derive(Error, Debug)]
pub enum TestError {
    #[error("data store error")]
    StoreError(#[from] StoreError),

    #[error("empty interval")]
    EmptyInterval(Hash, Interval),

    #[error("sibling intervals are expected to be consecutive")]
    NonConsecutiveSiblingIntervals(Interval, Interval),

    #[error("child interval out of parent bounds")]
    IntervalOutOfParentBounds { parent: Hash, child: Hash, parent_interval: Interval, child_interval: Interval },
}

pub fn validate_intervals(store: &dyn ReachabilityStore, root: Hash) -> std::result::Result<(), TestError> {
    let mut queue = VecDeque::<Hash>::from([root]);
    while !queue.is_empty() {
        let parent = queue.pop_front().unwrap();
        let children = store.get_children(parent)?;
        queue.extend(children.iter());

        let parent_interval = store.get_interval(parent)?;
        if parent_interval.is_empty() {
            return Err(TestError::EmptyInterval(parent, parent_interval));
        }

        // Verify parent-child strict relation
        for child in children.iter().cloned() {
            let child_interval = store.get_interval(child)?;
            if !parent_interval.strictly_contains(child_interval) {
                return Err(TestError::IntervalOutOfParentBounds { parent, child, parent_interval, child_interval });
            }
        }

        // Iterate over consecutive siblings
        for siblings in children.windows(2) {
            let sibling_interval = store.get_interval(siblings[0])?;
            let current_interval = store.get_interval(siblings[1])?;
            if sibling_interval.end + 1 != current_interval.start {
                return Err(TestError::NonConsecutiveSiblingIntervals(sibling_interval, current_interval));
            }
        }
    }
    Ok(())
}
