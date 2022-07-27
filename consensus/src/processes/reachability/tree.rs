//!
//! Tree-related functions internal to the module
//!
use super::{inquirer::*, reindex::ReindexOperationContext, *};
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub fn add_tree_block(
    store: &mut dyn ReachabilityStore, new_block: Hash, parent: Hash, reindex_depth: u64, reindex_slack: u64,
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
        let mut ctx = ReindexOperationContext::new(store, reindex_depth, reindex_slack);
        ctx.reindex_intervals(new_block, reindex_root)?;
    } else {
        let allocated = remaining.split_half().0;
        store.insert(new_block, parent, allocated, parent_height + 1)?;
    };
    Ok(())
}

/// Finds the most recent tree ancestor common to both `block` and the given `reindex root`.
/// Note that we assume that almost always the chain between the reindex root and the common
/// ancestor is longer than the chain between block and the common ancestor, hence we iterate
/// from `block`.
pub fn find_common_tree_ancestor(store: &dyn ReachabilityStore, block: Hash, reindex_root: Hash) -> Result<Hash> {
    let mut current = block;
    loop {
        if is_chain_ancestor_of(store, current, reindex_root)? {
            return Ok(current);
        }
        current = store.get_parent(current)?;
    }
}

/// Finds a possible new reindex root, based on the `current` reindex root and the selected tip `hint`
pub fn find_next_reindex_root(
    store: &dyn ReachabilityStore, current: Hash, hint: Hash, reindex_depth: u64, reindex_slack: u64,
) -> Result<(Hash, Hash)> {
    let mut ancestor = current;
    let mut next = current;

    let hint_height = store.get_height(hint)?;

    // Test if current root is ancestor of selected tip (`hint`) - if not, this is a reorg case
    if !is_chain_ancestor_of(store, current, hint)? {
        let current_height = store.get_height(current)?;

        // We have reindex root out of (hint) selected tip chain, however we switch chains only after a sufficient
        // threshold of `reindex_slack` diff in order to address possible alternating reorg attacks.
        // The `reindex_slack` constant is used as an heuristic large enough on the one hand, but
        // one which will not harm performance on the other hand - given the available slack at the chain split point.
        //
        // Note: In some cases the height of the (hint) selected tip can be lower than the current reindex root height.
        // If that's the case we keep the reindex root unchanged.
        if hint_height < current_height || hint_height - current_height < reindex_slack {
            return Ok((current, current));
        }

        let common = find_common_tree_ancestor(store, hint, current)?;
        ancestor = common;
        next = common;
    }

    // Iterate from ancestor towards the selected tip (`hint`) until passing the
    // `reindex_window` threshold, for finding the new reindex root
    loop {
        let child = get_next_chain_ancestor_unchecked(store, hint, next)?;
        let child_height = store.get_height(child)?;

        if hint_height < child_height {
            return Err(ReachabilityError::DataInconsistency);
        }
        if hint_height - child_height < reindex_depth {
            break;
        }
        next = child;
    }

    Ok((ancestor, next))
}

pub fn try_advancing_reindex_root(
    store: &mut dyn ReachabilityStore, hint: Hash, reindex_depth: u64, reindex_slack: u64,
) -> Result<()> {
    // Get current root from the store
    let current = store.get_reindex_root()?;

    // Find the possible new root
    let (mut ancestor, next) = find_next_reindex_root(store, current, hint, reindex_depth, reindex_slack)?;

    // No update to root, return
    if current == next {
        return Ok(());
    }

    // if ancestor == next {
    //     trace!("next reindex root is an ancestor of current one, skipping concentration.")
    // }
    while ancestor != next {
        let child = get_next_chain_ancestor_unchecked(store, next, ancestor)?;
        let mut ctx = ReindexOperationContext::new(store, reindex_depth, reindex_slack);
        ctx.concentrate_interval(ancestor, child, child == next)?;
        ancestor = child;
    }

    // Update reindex root in the data store
    store.set_reindex_root(next)?;
    Ok(())
}
