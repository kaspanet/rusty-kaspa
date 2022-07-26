use super::{reindex::ReindexOperationContext, *};
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn init(store: &mut dyn ReachabilityStore) -> Result<()> {
    todo!()
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

fn add_tree_child(store: &mut dyn ReachabilityStore, new_child: Hash, parent: Hash) -> Result<()> {
    // Get the remaining interval capacity
    let remaining = store.interval_remaining_after(parent)?;
    // Append the new child to `parent.children`
    store.append_child(parent, new_child)?;
    if remaining.is_empty() {
        // Init with the empty interval.
        // Note: internal logic relies on interval being this specific interval
        //       which comes exactly at the end of current capacity
        store.insert(new_child, parent, remaining)?;

        // Start a reindex operation (TODO: add timing)
        let reindex_root = store.get_reindex_root()?;
        let mut ctx = ReindexOperationContext::new(store, reindex_root, None, None);
        ctx.reindex_intervals(new_child)?;
    } else {
        let allocated = remaining.split_half().0;
        store.insert(new_child, parent, allocated)?;
    };
    Ok(())
}

pub fn is_strict_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: &Hash, queried: &Hash) -> Result<bool> {
    todo!()
}

pub fn is_chain_ancestor_of(store: &dyn ReachabilityStore, anchor: &Hash, queried: &Hash) -> Result<bool> {
    todo!()
}

pub fn is_dag_ancestor_of(store: &dyn ReachabilityStore, anchor: &Hash, queried: &Hash) -> Result<bool> {
    todo!()
}

pub fn get_next_chain_ancestor(store: &dyn ReachabilityStore, descendant: &Hash, ancestor: &Hash) -> Result<Hash> {
    todo!()
}

#[cfg(test)]
pub(super) mod tests {
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

        pub fn add_block(&mut self, hash: Hash, parent: Hash) -> &mut Self {
            add_tree_child(self.store, hash, parent).unwrap();
            self
        }
    }

    #[test]
    fn test_add_blocks() {
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());

        // Init
        let root: Hash = 1.into();
        store
            .insert(root, Hash::DEFAULT, Interval::maximal())
            .unwrap();

        // Act
        TreeBuilder::new(store.as_mut())
            .add_block(2.into(), root)
            .add_block(3.into(), 2.into())
            .add_block(4.into(), 2.into())
            .add_block(5.into(), 3.into())
            .add_block(6.into(), 5.into())
            .add_block(7.into(), 1.into())
            .add_block(8.into(), 6.into());

        // Assert
        validate_intervals(store.as_ref(), root).unwrap();
    }
}
