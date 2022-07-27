use super::interval::Interval;
use super::{tree::*, *};
use crate::model;
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn init(store: &mut dyn ReachabilityStore) -> Result<()> {
    if store.has(model::ORIGIN)? {
        return Ok(());
    }
    store.insert(model::ORIGIN, Hash::ZERO, Interval::maximal())?;
    store.set_reindex_root(model::ORIGIN)?;
    Ok(())
}

pub fn add_block(
    store: &mut dyn ReachabilityStore, new_block: Hash, selected_parent: Hash, mergeset: &[Hash],
    is_selected_leaf: bool,
) -> Result<()> {
    add_tree_child(store, new_block, selected_parent)?;

    // // Update the future covering set for blocks in the mergeset
    // for merged_block in mergeset {
    //     self.insert_to_fcs(store, merged_block, &block)?;
    // }

    // // Update the reindex root by the new selected leaf
    // if is_selected_leaf {
    //     self.update_reindex_root(store, &block)?;
    // }

    Ok(())
}

/// is_strict_chain_ancestor_of checks if the `anchor` block is a strict
/// chain ancestor of the `queried` block. Note that this results in `false`
/// if `anchor == queried`
pub fn is_strict_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    Ok(store
        .get_interval(anchor)?
        .strictly_contains(store.get_interval(queried)?))
}

/// is_chain_ancestor_of checks if the `anchor` block is a chain ancestor
/// of the `queried` block. Note that we use the graph theory convention
/// here which defines that a block is also an ancestor of itself.
pub fn is_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    Ok(store
        .get_interval(anchor)?
        .contains(store.get_interval(queried)?))
}

pub fn is_dag_ancestor_of(store: &dyn ReachabilityStore, anchor: Hash, queried: Hash) -> Result<bool> {
    todo!()
}

/// get_next_chain_ancestor finds the reachability/selection tree child
/// of 'ancestor' which is also an ancestor of 'descendant'.
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

pub fn forward_chain_iterator(
    store: &dyn ReachabilityStore, descendant: Hash, ancestor: Hash,
) -> ForwardChainIterator<'_> {
    ForwardChainIterator::new(store, ancestor, descendant, false)
}

pub struct ForwardChainIterator<'a> {
    store: &'a dyn ReachabilityStore,
    current: Hash,
    descendant: Hash,
    inclusive: bool,
}

impl<'a> ForwardChainIterator<'a> {
    fn new(store: &'a dyn ReachabilityStore, current: Hash, descendant: Hash, inclusive: bool) -> Self {
        Self { store, current, descendant, inclusive }
    }
}

impl<'a> Iterator for ForwardChainIterator<'a> {
    type Item = Result<Hash>;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.current;
        // TODO: test for end of sequence
        match get_next_chain_ancestor(self.store, self.descendant, current) {
            Ok(next) => {
                self.current = next;
                Some(Ok(current))
            }
            Err(e) => Some(Err(e)),
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
    }

    impl<'a> TreeBuilder<'a> {
        pub fn new(store: &'a mut dyn ReachabilityStore) -> Self {
            Self { store }
        }

        pub fn init(&mut self, root: Hash, interval: Interval) -> &mut Self {
            self.store
                .insert(root, Hash::ZERO, interval)
                .unwrap();
            self.store.set_reindex_root(root).unwrap();
            self
        }

        pub fn add_block(&mut self, hash: Hash, parent: Hash) -> &mut Self {
            add_tree_child(self.store, hash, parent).unwrap();
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

        // Act
        let iter = forward_chain_iterator(store.as_ref(), 10.into(), 2.into());

        // Assert (wip)
        for block in iter.take(12) {
            println!("{:?}", block);
        }
    }
}
