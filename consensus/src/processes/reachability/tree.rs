//!
//! Tree-related functions internal to the module
//!
use super::{inquirer::is_chain_ancestor_of, reindex::ReindexOperationContext, *};
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn add_tree_block(
    store: &mut dyn ReachabilityStore, new_block: Hash, parent: Hash, reindex_depth: Option<u64>,
    reindex_slack: Option<u64>,
) -> Result<()> {
    // Get the remaining interval capacity
    let remaining = store.interval_remaining_after(parent)?;
    // Append the new child to `parent.children`
    let parent_height = store.append_child(parent, new_block)?;
    if remaining.is_empty() {
        // Init with the empty interval.
        // Note: internal logic relies on interval being this specific interval
        //       which comes exactly at the end of current capacity
        store.insert(new_block, parent, remaining, parent_height + 1)?;

        // Start a reindex operation (TODO: add timing)
        let reindex_root = store.get_reindex_root()?;
        let mut ctx = ReindexOperationContext::new(store, reindex_root, reindex_depth, reindex_slack);
        ctx.reindex_intervals(new_block)?;
    } else {
        let allocated = remaining.split_half().0;
        store.insert(new_block, parent, allocated, parent_height + 1)?;
    };
    Ok(())
}

pub fn find_common_tree_ancestor(store: &dyn ReachabilityStore, block: Hash, reindex_root: Hash) -> Result<Hash> {
    let mut current = block;
    loop {
        if is_chain_ancestor_of(store, current, reindex_root)? {
            return Ok(current);
        }
        current = store.get_parent(current)?;
    }

    // Can also be written with a backward iterator:
    //
    // for result in default_chain_iterator(store, block) {
    //     let current = result?;
    //     if is_chain_ancestor_of(store, current, reindex_root)? {
    //         return Ok(current);
    //     }
    // }

    // Err(ReachabilityError::BadQuery)
}
