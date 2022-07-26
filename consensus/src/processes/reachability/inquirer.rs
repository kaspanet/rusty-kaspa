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

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::processes::reachability::interval::Interval;
    use std::collections::VecDeque;
    use thiserror::Error;

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

            for child in children.iter().cloned() {
                let child_interval = store.get_interval(child)?;
                if !parent_interval.strictly_contains(child_interval) {
                    return Err(TestError::IntervalOutOfParentBounds {
                        parent,
                        child,
                        parent_interval,
                        child_interval,
                    });
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

    #[test]
    fn test_add_block() {}
}
