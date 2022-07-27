use super::{inquirer::get_next_chain_ancestor_unchecked, interval::Interval, *};
use crate::model::{api::hash::Hash, stores::reachability::ReachabilityStore};
use std::collections::{HashMap, VecDeque};

pub(super) struct ReindexOperationContext<'a> {
    store: &'a mut dyn ReachabilityStore,
    subtree_sizes: HashMap<Hash, u64>, // Cache for subtree sizes computed during this operation
    depth: u64,
    slack: u64,
}

impl<'a> ReindexOperationContext<'a> {
    pub(super) fn new(store: &'a mut dyn ReachabilityStore, depth: u64, slack: u64) -> Self {
        Self { store, subtree_sizes: HashMap::new(), depth, slack }
    }

    pub(super) fn reindex_intervals(&mut self, new_child: Hash, reindex_root: Hash) -> Result<()> {
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

            if current == reindex_root {
                // TODO: comment and add detailed inner error
                return Err(ReachabilityError::DataOverflow);
            }

            if inquirer::is_strict_chain_ancestor_of(self.store, parent, reindex_root)? {
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
    /// ```no_run
    /// fn count_subtrees(&mut self, block: Hash) -> Result<u64> {
    ///     let mut subtree_size = 0u64;
    ///     for child in self.store.get_children(block)?.iter().cloned() {
    ///         subtree_size += self.count_subtrees(child)?;
    ///     }
    ///     self.subtree_sizes.insert(block, subtree_size + 1);
    ///     Ok(subtree_size + 1)
    /// }
    /// ```
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

    /// Propagates a new interval using a BFS traversal.
    /// Subtree intervals are recursively allocated according to subtree sizes and
    /// the allocation rule in `Interval::split_exponential`.
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

    /// This method implements the reindex algorithm for the case where the
    /// new child node is not in reindex root's subtree. The function is expected to allocate
    /// `required_allocation` to be added to interval of `allocation_block`. `common_ancestor` is
    /// expected to be a direct parent of `allocation_block` and an ancestor of current `reindex_root`.
    fn reindex_intervals_earlier_than_root(
        &mut self, allocation_block: Hash, common_ancestor: Hash, required_allocation: u64,
    ) -> Result<()> {
        todo!()
    }

    fn reclaim_interval_after(
        &mut self, allocation_block: Hash, common_ancestor: Hash, chosen_child: Hash, reindex_root: Hash,
        required_allocation: u64,
    ) -> Result<()> {
        let mut slack_sum = 0u64;
        let mut path_len = 0u64;
        let mut path_slack_alloc = 0u64;

        let mut current = chosen_child;
        // Walk up the chain from common ancestor's chosen child towards reindex root
        loop {
            if current == reindex_root {
                // Reached reindex root. In this case, since we reached (the unlimited) root,
                // we also re-allocate new slack for the chain we just traversed
                let offset = required_allocation + self.slack * path_len - slack_sum;
                self.apply_interval_op_and_propagate(current, offset, Interval::decrease_end)?;
                self.offset_siblings_after(allocation_block, current, offset)?;

                // Set the slack for each chain block to be reserved below during the chain walk-down
                path_slack_alloc = self.slack;
                break;
            }

            let slack_after_current = self
                .store
                .interval_remaining_after(current)?
                .size();
            slack_sum += slack_after_current;

            if slack_sum >= required_allocation {
                // Set offset to be just enough to satisfy required allocation
                let offset = slack_after_current - (slack_sum - required_allocation);
                self.apply_interval_op(current, offset, Interval::decrease_end)?;
                self.offset_siblings_after(allocation_block, current, offset)?;

                break;
            }

            current = get_next_chain_ancestor_unchecked(self.store, reindex_root, current)?;
            path_len += 1;
        }

        // Go back down the reachability tree towards the common ancestor.
        // On every hop we reindex the reachability subtree before the
        // current block with an interval that is smaller.
        // This is to make room for the required allocation.
        loop {
            current = self.store.get_parent(current)?;
            if current == common_ancestor {
                break;
            }

            let slack_after_current = self
                .store
                .interval_remaining_after(current)?
                .size();
            let offset = slack_after_current - path_slack_alloc;
            self.apply_interval_op(current, offset, Interval::decrease_end)?;
            self.offset_siblings_after(allocation_block, current, offset)?;
        }

        Ok(())
    }

    fn offset_siblings_before(&mut self, allocation_block: Hash, current: Hash, offset: u64) -> Result<()> {
        let parent = self.store.get_parent(current)?;
        let children = self.store.get_children(parent)?;

        let (siblings_before, _) = split_children(&children, current)?;
        for sibling in siblings_before.iter().cloned().rev() {
            if sibling == allocation_block {
                // We reached our final destination, allocate `offset` to `allocation_block` by increasing end and break
                self.apply_interval_op_and_propagate(allocation_block, offset, Interval::increase_end)?;
                break;
            }
            // For non-`allocation_block` siblings offset the interval upwards in order to create space
            self.apply_interval_op_and_propagate(sibling, offset, Interval::increase)?;
        }

        Ok(())
    }

    fn offset_siblings_after(&mut self, allocation_block: Hash, current: Hash, offset: u64) -> Result<()> {
        let parent = self.store.get_parent(current)?;
        let children = self.store.get_children(parent)?;

        let (_, siblings_after) = split_children(&children, current)?;
        for sibling in siblings_after.iter().cloned() {
            if sibling == allocation_block {
                // We reached our final destination, allocate `offset` to `allocation_block` by decreasing only start and break
                self.apply_interval_op_and_propagate(allocation_block, offset, Interval::decrease_start)?;
                break;
            }
            // For siblings before `allocation_block` offset the interval downwards to create space
            self.apply_interval_op_and_propagate(sibling, offset, Interval::decrease)?;
        }

        Ok(())
    }

    fn apply_interval_op(&mut self, block: Hash, offset: u64, op: fn(&Interval, u64) -> Interval) -> Result<()> {
        self.store
            .set_interval(block, op(&self.store.get_interval(block)?, offset))?;
        Ok(())
    }

    fn apply_interval_op_and_propagate(
        &mut self, block: Hash, offset: u64, op: fn(&Interval, u64) -> Interval,
    ) -> Result<()> {
        self.store
            .set_interval(block, op(&self.store.get_interval(block)?, offset))?;
        self.propagate_interval(block)?;
        Ok(())
    }

    pub(super) fn concentrate_interval(
        &mut self, parent: Hash, child: Hash, is_final_reindex_root: bool,
    ) -> Result<()> {
        let children = self.store.get_children(parent)?;

        // Split the `children` of `parent` to siblings before `child` and siblings after `child`
        let (siblings_before, siblings_after) = split_children(&children, child)?;

        let siblings_before_subtrees_sum: u64 = self.tighten_intervals_before(parent, siblings_before)?;
        let siblings_after_subtrees_sum: u64 = self.tighten_intervals_after(parent, siblings_after)?;

        self.expand_interval_to_chosen(
            parent,
            child,
            siblings_before_subtrees_sum,
            siblings_after_subtrees_sum,
            is_final_reindex_root,
        )?;

        Ok(())
    }

    pub(super) fn tighten_intervals_before(&mut self, parent: Hash, children_before: &[Hash]) -> Result<u64> {
        let sizes = children_before
            .iter()
            .cloned()
            .map(|block| {
                self.count_subtrees(block)?;
                Ok(self.subtree_sizes[&block])
            })
            .collect::<Result<Vec<u64>>>()?;
        let sum = sizes.iter().sum();

        let interval = self.store.get_interval(parent)?;
        let interval_before = Interval::new(interval.start + self.slack, interval.start + self.slack + sum - 1);

        for (c, ci) in children_before
            .iter()
            .cloned()
            .zip(interval_before.split_exact(sizes.as_slice()))
        {
            self.store.set_interval(c, ci)?;
            self.propagate_interval(c)?;
        }

        Ok(sum)
    }

    pub(super) fn tighten_intervals_after(&mut self, parent: Hash, children_after: &[Hash]) -> Result<u64> {
        let sizes = children_after
            .iter()
            .cloned()
            .map(|block| {
                self.count_subtrees(block)?;
                Ok(self.subtree_sizes[&block])
            })
            .collect::<Result<Vec<u64>>>()?;
        let sum = sizes.iter().sum();

        let interval = self.store.get_interval(parent)?;
        let interval_after = Interval::new(interval.end - self.slack - sum, interval.end - self.slack - 1);

        for (c, ci) in children_after
            .iter()
            .cloned()
            .zip(interval_after.split_exact(sizes.as_slice()))
        {
            self.store.set_interval(c, ci)?;
            self.propagate_interval(c)?;
        }

        Ok(sum)
    }

    pub(super) fn expand_interval_to_chosen(
        &mut self, parent: Hash, child: Hash, siblings_before_subtrees_sum: u64, siblings_after_subtrees_sum: u64,
        is_final_reindex_root: bool,
    ) -> Result<()> {
        let interval = self.store.get_interval(parent)?;
        let allocation = Interval::new(
            interval.start + siblings_before_subtrees_sum + self.slack,
            interval.end - siblings_after_subtrees_sum - self.slack - 1,
        );
        let current = self.store.get_interval(child)?;

        // Propagate interval only if the chosen `child` is the final reindex root AND
        // the new interval doesn't contain the previous one
        if is_final_reindex_root && !allocation.contains(current) {
            /*
            We deallocate slack on both sides as an optimization. Were we to
            assign the fully allocated interval, the next time the reindex root moves we
            would need to propagate intervals again. However when we do allocate slack,
            next time this method is called (next time the reindex root moves), `allocation` is likely to contain `current`.
            Note that below following the propagation we reassign the full `allocation` to `child`.
            */
            let narrowed = Interval::new(allocation.start + self.slack, allocation.end - self.slack);
            self.store.set_interval(child, narrowed)?;
            self.propagate_interval(child)?;
        }

        self.store.set_interval(child, allocation)?;
        Ok(())
    }
}

/// Splits `children` into two slices: the blocks that are before `pivot` and the blocks that are after.
fn split_children(children: &std::rc::Rc<Vec<Hash>>, pivot: Hash) -> Result<(&[Hash], &[Hash])> {
    if let Some(index) = children.iter().cloned().position(|c| c == pivot) {
        Ok((&children[..index], &children[index + 1..]))
    } else {
        Err(ReachabilityError::DataInconsistency)
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
        let mut ctx = ReindexOperationContext::new(store.as_mut(), 10, 16);
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
