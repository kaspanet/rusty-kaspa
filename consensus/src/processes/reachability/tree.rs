//!
//! Tree-related functions internal to the module
//!
use super::{reindex::ReindexOperationContext, *};
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn add_tree_child(store: &mut dyn ReachabilityStore, new_child: Hash, parent: Hash) -> Result<()> {
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
