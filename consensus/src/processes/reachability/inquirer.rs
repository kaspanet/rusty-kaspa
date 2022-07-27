use super::interval::Interval;
use super::{reindex::*, tree::*, *};
use crate::model;
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

/// Init the reachability store to match the state required by the algorithmic layer.
/// The function first checks the store for possibly being initialized already.
pub fn init(store: &mut dyn ReachabilityStore) -> Result<()> {
    init_with_params(store, model::ORIGIN, Interval::maximal())
}

fn init_with_params(store: &mut dyn ReachabilityStore, origin: Hash, capacity: Interval) -> Result<()> {
    if store.has(origin)? {
        return Ok(());
    }
    store.insert(origin, Hash::ZERO, capacity, 0)?;
    store.set_reindex_root(origin)?;
    Ok(())
}

/// Add a block to the DAG reachability data structures and persist using the provided `store`.
pub fn add_block(
    store: &mut dyn ReachabilityStore, new_block: Hash, selected_parent: Hash, mergeset: &[Hash],
) -> Result<()> {
    add_block_with_params(store, new_block, selected_parent, mergeset, None, None)
}

fn add_block_with_params(
    store: &mut dyn ReachabilityStore, new_block: Hash, selected_parent: Hash, mergeset: &[Hash],
    reindex_depth: Option<u64>, reindex_slack: Option<u64>,
) -> Result<()> {
    add_tree_block(
        store,
        new_block,
        selected_parent,
        reindex_depth.unwrap_or(DEFAULT_REINDEX_DEPTH),
        reindex_slack.unwrap_or(DEFAULT_REINDEX_SLACK),
    )?;
    add_dag_block(store, new_block, mergeset)?;
    Ok(())
}

fn add_dag_block(store: &mut dyn ReachabilityStore, new_block: Hash, mergeset: &[Hash]) -> Result<()> {
    // // Update the future covering set for blocks in the mergeset
    // for merged_block in mergeset {
    //     insert_to_fcs(store, merged_block, block)?;
    // }
    Ok(())
}

/// Hint to the reachability algorithm that `hint` is a candidate to become
/// the `virtual selected parent` (`VSP`). This might affect internal reachability heuristics such
/// as moving the reindex point. The consensus runtime is expected to call this function
/// for a new header selected tip which is `header only` / `pending UTXO verification`, or for a completely resolved `VSP`.
pub fn hint_virtual_selected_parent(store: &mut dyn ReachabilityStore, hint: Hash) -> Result<()> {
    try_advancing_reindex_root(store, hint, DEFAULT_REINDEX_DEPTH, DEFAULT_REINDEX_SLACK)
}

/// Checks if the `anchor` block is a strict chain ancestor of the `queried` block.
/// Note that this results in `false` if `anchor == queried`
pub fn is_strict_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    Ok(store
        .get_interval(anchor)?
        .strictly_contains(store.get_interval(queried)?))
}

/// Checks if `anchor` block is a chain ancestor of `queried` block. Note that we use the
/// graph theory convention here which defines that a block is also an ancestor of itself.
pub fn is_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    Ok(store
        .get_interval(anchor)?
        .contains(store.get_interval(queried)?))
}

/// Returns true if `anchor` is a DAG ancestor of `queried`.
/// Note: this method will return true if `anchor == queried`.
/// The complexity of this method is O(log(|future_covering_set(anchor)|))
pub fn is_dag_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    todo!()
}

/// Finds the child of `ancestor` which is also a chain ancestor of `descendant`.
pub fn get_next_chain_ancestor(store: &dyn ReachabilityStore, descendant: Hash, ancestor: Hash) -> Result<Hash> {
    if descendant == ancestor {
        // The next ancestor does not exist
        return Err(ReachabilityError::BadQuery);
    }
    if !is_strict_chain_ancestor_of(store, ancestor, descendant)? {
        // `ancestor` isn't actually a chain ancestor of `descendant`, so by def
        // we cannot find the next ancestor
        return Err(ReachabilityError::BadQuery);
    }

    let point = store.get_interval(descendant)?.start;
    let children = store.get_children(ancestor)?;

    // Works only with nightly and by adding the line `#![feature(is_sorted)]` to lib.rs
    //
    // debug_assert!(children.iter().is_sorted_by_key(|c| {
    //     store
    //         .get_interval(*c)
    //         .expect("reachability interval data missing from store")
    //         .start
    // }));

    // We use an `expect` here since otherwise we need to implement `binary_search`
    // ourselves, which is not worth the effort since this is an unrecoverable error anyhow
    match children.binary_search_by_key(&point, |c| {
        store
            .get_interval(*c)
            .expect("reachability interval data missing from store")
            .start
    }) {
        Ok(i) => Ok(children[i]),
        Err(i) => {
            // `i` is where `point` was expected (i.e., point < children[i].interval.start),
            // so we expect `children[i - 1].interval` to contain `point`
            if i > 0 && is_chain_ancestor_of(store, children[i - 1], descendant)? {
                Ok(children[i - 1])
            } else {
                Err(ReachabilityError::DataInconsistency)
            }
        }
    }
}

/// Returns a forward iterator walking up the chain-selection tree from `from_ancestor`
/// to `to_descendant`, where `to_descendant` is included if `inclusive` is set to true.
/// The caller is expected to verify that `from_ancestor` is indeed a chain ancestor of
/// `to_descendant`, otherwise a `ReachabilityError::BadQuery` error will be returned.  
pub fn forward_chain_iterator(
    store: &dyn ReachabilityStore, from_ancestor: Hash, to_descendant: Hash, inclusive: bool,
) -> ForwardChainIterator<'_> {
    ForwardChainIterator::new(store, from_ancestor, to_descendant, inclusive)
}

/// Returns a backward iterator walking down the selected chain from `from_descendant`
/// to `to_ancestor`, where `to_ancestor` is included if `inclusive` is set to true.
/// The caller is expected to verify that `to_ancestor` is indeed a chain ancestor of
/// `from_descendant`, otherwise the iterator will eventually return an error.  
pub fn backward_chain_iterator(
    store: &dyn ReachabilityStore, from_descendant: Hash, to_ancestor: Hash, inclusive: bool,
) -> BackwardChainIterator<'_> {
    BackwardChainIterator::new(store, from_descendant, to_ancestor, inclusive)
}

/// Returns the default chain iterator, walking from `from` backward down the
/// selected chain until `virtual genesis` (aka `model::ORIGIN`; exclusive)
pub fn default_chain_iterator(store: &dyn ReachabilityStore, from: Hash) -> BackwardChainIterator<'_> {
    BackwardChainIterator::new(store, from, model::ORIGIN, false)
}

pub struct ForwardChainIterator<'a> {
    store: &'a dyn ReachabilityStore,
    current: Option<Hash>,
    descendant: Hash,
    inclusive: bool,
}

impl<'a> ForwardChainIterator<'a> {
    fn new(store: &'a dyn ReachabilityStore, from_ancestor: Hash, to_descendant: Hash, inclusive: bool) -> Self {
        Self { store, current: Some(from_ancestor), descendant: to_descendant, inclusive }
    }
}

impl<'a> Iterator for ForwardChainIterator<'a> {
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
                match get_next_chain_ancestor(self.store, self.descendant, current) {
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

pub struct BackwardChainIterator<'a> {
    store: &'a dyn ReachabilityStore,
    current: Option<Hash>,
    ancestor: Hash,
    inclusive: bool,
}

impl<'a> BackwardChainIterator<'a> {
    fn new(store: &'a dyn ReachabilityStore, from_descendant: Hash, to_ancestor: Hash, inclusive: bool) -> Self {
        Self { store, current: Some(from_descendant), ancestor: to_ancestor, inclusive }
    }
}

impl<'a> Iterator for BackwardChainIterator<'a> {
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
                debug_assert_ne!(current, Hash::ZERO);
                match self.store.get_parent(current) {
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

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;
    use crate::{model::stores::reachability::MemoryReachabilityStore, processes::reachability::interval::Interval};

    /// A struct with fluent API to streamline tree building
    struct TreeBuilder<'a> {
        store: &'a mut dyn ReachabilityStore,
        reindex_depth: u64,
        reindex_slack: u64,
    }

    impl<'a> TreeBuilder<'a> {
        pub fn new(store: &'a mut dyn ReachabilityStore) -> Self {
            Self { store, reindex_depth: DEFAULT_REINDEX_DEPTH, reindex_slack: DEFAULT_REINDEX_SLACK }
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
            self
        }
    }

    #[test]
    fn test_add_blocks() {
        // Arrange
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());

        // Act
        let root: Hash = 1.into();
        TreeBuilder::new(store.as_mut())
            .init(root, Interval::new(1, 15))
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

        // Assert
        validate_intervals(store.as_ref(), root).unwrap();
    }

    #[test]
    fn test_forward_iterator() {
        // Arrange
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());

        // Act
        let root: Hash = 1.into();
        TreeBuilder::new(store.as_mut())
            .init(root, Interval::new(1, 15))
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

        // Exclusive
        let iter = forward_chain_iterator(store.as_ref(), 2.into(), 10.into(), false);

        // Assert
        let expected_hashes = [2u64, 3, 5, 6].map(Hash::from);
        assert!(expected_hashes
            .iter()
            .cloned()
            .eq(iter.map(|r| r.unwrap())));
        assert_eq!(
            store.get_height(2.into()).unwrap() + expected_hashes.len() as u64,
            store.get_height(10.into()).unwrap()
        );

        // Inclusive
        let iter = forward_chain_iterator(store.as_ref(), 2.into(), 10.into(), true);

        // Assert
        let expected_hashes = [2u64, 3, 5, 6, 10].map(Hash::from);
        assert!(expected_hashes
            .iter()
            .cloned()
            .eq(iter.map(|r| r.unwrap())));

        // Compare backward to reversed forward
        let forward_iter = forward_chain_iterator(store.as_ref(), 2.into(), 10.into(), true).map(|r| r.unwrap());
        let backward_iter: Result<Vec<Hash>> =
            backward_chain_iterator(store.as_ref(), 10.into(), 2.into(), true).collect();
        assert!(forward_iter.eq(backward_iter.unwrap().iter().cloned().rev()))
    }

    #[test]
    fn test_iterator_boundaries() {
        // Arrange & Act
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());
        let root: Hash = 1.into();
        TreeBuilder::new(store.as_mut())
            .init(root, Interval::new(1, 5))
            .add_block(2.into(), root);

        // Asserts
        assert!([1u64, 2]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(forward_chain_iterator(store.as_ref(), 1.into(), 2.into(), true).map(|r| r.unwrap())));

        assert!([1u64]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(forward_chain_iterator(store.as_ref(), 1.into(), 2.into(), false).map(|r| r.unwrap())));

        assert!([2u64, 1]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(backward_chain_iterator(store.as_ref(), 2.into(), root, true).map(|r| r.unwrap())));

        assert!([2u64]
            .map(Hash::from)
            .iter()
            .cloned()
            .eq(backward_chain_iterator(store.as_ref(), 2.into(), root, false).map(|r| r.unwrap())));

        assert!(std::iter::once_with(|| root)
            .eq(backward_chain_iterator(store.as_ref(), root, root, true).map(|r| r.unwrap())));

        assert!(std::iter::empty::<Hash>()
            .eq(backward_chain_iterator(store.as_ref(), root, root, false).map(|r| r.unwrap())));
    }
}
