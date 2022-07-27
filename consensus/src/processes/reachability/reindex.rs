use std::collections::{HashMap, VecDeque};

use super::*;
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};

pub const DEFAULT_REINDEX_DEPTH: u64 = 100;
pub const DEFAULT_REINDEX_SLACK: u64 = 1 << 12;

pub(super) struct ReindexOperationContext<'a> {
    store: &'a mut dyn ReachabilityStore,
    root: Hash,
    subtree_sizes: HashMap<Hash, u64>,
    depth: u64,
    slack: u64,
}

impl<'a> ReindexOperationContext<'a> {
    pub(super) fn new(store: &'a mut dyn ReachabilityStore, root: Hash, depth: u64, slack: u64) -> Self {
        Self { store, root, subtree_sizes: HashMap::new(), depth, slack }
    }

    pub(super) fn reindex_intervals(&mut self, new_child: Hash) -> Result<()> {
        let mut current = new_child;
        loop {
            let current_interval = self.store.get_interval(current)?;
            self.count_subtrees(current)?;

            if current_interval.size() >= self.subtree_sizes[&current] {
                break;
            }

            let parent = self.store.get_parent(current)?;

            if parent.is_zero() {
                // TODO: comment and add detailed inner error
                return Err(ReachabilityError::DataOverflow);
            }

            if current == self.root {
                // TODO: comment and add detailed inner error
                return Err(ReachabilityError::DataOverflow);
            }

            if inquirer::is_strict_chain_ancestor_of(self.store, parent, self.root)? {
                return self.reindex_intervals_earlier_than_root(current, parent, self.subtree_sizes[&current]);
            }

            current = parent
        }

        self.propagate_interval(current)
    }

    ///
    /// Core (BFS) algorithms used during reindexing (see `count_subtrees` and `propagate_interval` below)
    ///

    ///
    /// count_subtrees counts the size of each subtree under this block,
    /// and populates self.subtree_sizes with the results.
    /// It is equivalent to the following recursive implementation:
    ///
    /// fn count_subtrees(&mut self, block: Hash) -> Result<u64> {
    ///     let mut subtree_size = 0u64;
    ///     for child in self.store.get_children(block)?.iter().cloned() {
    ///         subtree_size += self.count_subtrees(child)?;
    ///     }
    ///     self.subtree_sizes.insert(block, subtree_size + 1);
    ///     Ok(subtree_size + 1)
    /// }
    ///
    /// However, we are expecting (linearly) deep trees, and so a
    /// recursive stack-based approach is inefficient and will hit
    /// recursion limits. Instead, the same logic was implemented
    /// using a (queue-based) BFS method. At a high level, the
    /// algorithm uses BFS for reaching all leaves and pushes
    /// intermediate updates from leaves via parent chains until all
    /// size information is gathered at the root of the operation
    /// (i.e. at block).
    fn count_subtrees(&mut self, block: Hash) -> Result<()> {
        if self.subtree_sizes.contains_key(&block) {
            return Ok(());
        }

        let mut queue = VecDeque::<Hash>::from([block]);
        let mut counts = HashMap::<Hash, u64>::new();

        while !queue.is_empty() {
            let mut current = queue.pop_front().unwrap();
            let children = self.store.get_children(current)?;
            if children.is_empty() {
                // We reached a leaf
                self.subtree_sizes.insert(current, 1);
            } else if !self.subtree_sizes.contains_key(&current) {
                // We haven't yet calculated the subtree size of
                // the current block. Add all its children to the
                // queue
                queue.extend(children.iter());
                continue;
            }

            // We reached a leaf or a pre-calculated subtree.
            // Push information up
            while current != block {
                current = self.store.get_parent(current)?;

                let count = counts.entry(current).or_insert(0);
                let children = self.store.get_children(current)?;

                *count += 1;
                if *count < children.len() as u64 {
                    // Not all subtrees of the current block are ready
                    break;
                }

                // All children of `current` have calculated their subtree size.
                // Sum them all together and add 1 to get the sub tree size of
                // `current`.
                let subtree_sum: u64 = children
                    .iter()
                    .map(|c| self.subtree_sizes[c])
                    .sum();
                self.subtree_sizes
                    .insert(current, subtree_sum + 1);
            }
        }

        Ok(())
    }

    /// propagate_interval propagates a new interval using a BFS traversal.
    /// Subtree intervals are recursively allocated according to subtree sizes and
    /// the allocation rule in Interval::split_exponential.
    fn propagate_interval(&mut self, block: Hash) -> Result<()> {
        // Make sure subtrees are counted before propagating
        self.count_subtrees(block)?;

        let mut queue = VecDeque::<Hash>::from([block]);
        while !queue.is_empty() {
            let current = queue.pop_front().unwrap();
            let children = self.store.get_children(current)?;
            if !children.is_empty() {
                let sizes: Vec<u64> = children
                    .iter()
                    .map(|c| self.subtree_sizes[c])
                    .collect();
                let interval = self.store.interval_children_capacity(current)?;
                let intervals = interval.split_exponential(&sizes);
                for (c, ci) in children.iter().cloned().zip(intervals) {
                    self.store.set_interval(c, ci)?;
                }
                queue.extend(children.iter());
            }
        }
        Ok(())
    }

    fn reindex_intervals_earlier_than_root(
        &mut self, allocation_block: Hash, common_ancestor: Hash, required_allocation: u64,
    ) -> Result<()> {
        todo!()
    }

    pub(super) fn concentrate_interval(
        &mut self, ancestor: Hash, child: Hash, is_final_reindex_root: bool,
    ) -> Result<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;
    use crate::{model::stores::reachability::MemoryReachabilityStore, processes::reachability::interval::Interval};

    #[test]
    fn test_count_subtrees() {
        let mut store: Box<dyn ReachabilityStore> = Box::new(MemoryReachabilityStore::new());

        // Arrange
        let root: Hash = 1.into();
        StoreBuilder::new(store.as_mut())
            .add_block(root, Hash::ZERO)
            .add_block(2.into(), root)
            .add_block(3.into(), 2.into())
            .add_block(4.into(), 2.into())
            .add_block(5.into(), 3.into())
            .add_block(6.into(), 5.into())
            .add_block(7.into(), 1.into())
            .add_block(8.into(), 6.into());

        // Act
        let mut ctx = ReindexOperationContext::new(store.as_mut(), root, 10, 16);
        ctx.count_subtrees(root).unwrap();

        // Assert
        let expected = [(1u64, 8u64), (2, 6), (3, 4), (4, 1), (5, 3), (6, 2), (7, 1), (8, 1)]
            .iter()
            .cloned()
            .map(|(h, c)| (Hash::from(h), c))
            .collect::<HashMap<Hash, u64>>();

        assert_eq!(expected, ctx.subtree_sizes);

        // Act
        ctx.store
            .set_interval(root, Interval::new(1, 8))
            .unwrap();
        ctx.propagate_interval(root).unwrap();

        // Assert intervals manually
        let expected_intervals = [
            (1u64, (1u64, 8u64)),
            (2, (1, 6)),
            (3, (1, 4)),
            (4, (5, 5)),
            (5, (1, 3)),
            (6, (1, 2)),
            (7, (7, 7)),
            (8, (1, 1)),
        ];
        let actual_intervals = (1u64..=8)
            .map(|i| (i, ctx.store.get_interval(i.into()).unwrap().into()))
            .collect::<Vec<(u64, (u64, u64))>>();
        assert_eq!(actual_intervals, expected_intervals);

        // Assert intervals follow the general rules
        validate_intervals(store.as_ref(), root).unwrap();
    }
}
