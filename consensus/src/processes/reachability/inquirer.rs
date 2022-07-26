use super::*;
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn init(store: &mut dyn ReachabilityStore) -> Result<()> {
    todo!()
}

pub fn add_block(
    store: &mut dyn ReachabilityStore, block: Hash, selected_parent: Hash, mergeset: &[Hash], is_selected_leaf: bool,
) -> Result<()> {
    let remaining = store.interval_remaining_after(selected_parent)?;

    store.append_child(selected_parent, block)?;

    if remaining.is_empty() {
        store.insert(block, selected_parent, remaining)?;

        //
        // Start reindex context
        //
    } else {
        let allocated = remaining.split_half().0;
        store.insert(block, selected_parent, allocated)?;
    }

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
