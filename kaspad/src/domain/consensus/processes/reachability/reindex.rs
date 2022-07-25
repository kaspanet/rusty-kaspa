use std::collections::{HashMap, VecDeque};

use super::*;
use crate::domain::consensus::model::{api::hash::DomainHash, stores::reachability::ReachabilityStore};

const DEFAULT_REINDEX_DEPTH: u64 = 200;
const DEFAULT_REINDEX_SLACK: u64 = 1 << 12;

struct ReindexOperationContext<'a> {
    store: &'a mut dyn ReachabilityStore,
    root: DomainHash,
    subtree_sizes: HashMap<DomainHash, u64>,
    depth: u64,
    slack: u64,
}

impl<'a> ReindexOperationContext<'a> {
    fn new(store: &'a mut dyn ReachabilityStore, root: &DomainHash, depth: Option<u64>, slack: Option<u64>) -> Self {
        Self {
            store,
            root: *root,
            subtree_sizes: HashMap::new(),
            depth: depth.unwrap_or(DEFAULT_REINDEX_DEPTH),
            slack: slack.unwrap_or(DEFAULT_REINDEX_SLACK),
        }
    }

    fn reindex_intervals(&mut self, new_child: &DomainHash) -> Result<()> {
        let mut current = *new_child;
        loop {
            let current_interval = self.store.get_interval(&current)?;
            self.count_subtrees(current)?;

            if current_interval.size() >= self.subtree_sizes[&current] {
                break;
            }

            let parent = self.store.get_parent(&current)?;

            if parent.has_default_value() {
                // TODO: comment and add detailed inner error
                return Err(ReachabilityError::ReachabilityDataOverflowError);
            }

            if current == self.root {
                // TODO: comment and add detailed inner error
                return Err(ReachabilityError::ReachabilityDataOverflowError);
            }

            if inquirer::is_strict_chain_ancestor_of(self.store, &parent, &self.root)? {
                return self.reindex_intervals_earlier_than_root(current, parent, self.subtree_sizes[&current]);
            }

            current = parent
        }

        self.propagate_interval(current)
    }

    //
    // Core (BFS) algorithms used during reindexing (see `count_subtrees` and `propagate_interval` below)
    //

    // count_subtrees counts the size of each subtree under this block,
    // and populates self.subtree_sizes with the results.
    // It is equivalent to the following recursive implementation:
    //
    // fn count_subtrees(&mut self, block: DomainHash) -> Result<u64> {
    //     let mut subtree_size = 0u64;
    //     for child in self.store.get_children(&block)?.to_vec() {
    //         subtree_size += self.count_subtrees(child)?;
    //     }
    //     self.subtree_sizes.insert(block, subtree_size + 1);
    //     Ok(subtree_size + 1)
    // }
    //
    // However, we are expecting (linearly) deep trees, and so a
    // recursive stack-based approach is inefficient and will hit
    // recursion limits. Instead, the same logic was implemented
    // using a (queue-based) BFS method. At a high level, the
    // algorithm uses BFS for reaching all leaves and pushes
    // intermediate updates from leaves via parent chains until all
    // size information is gathered at the root of the operation
    // (i.e. at block).
    fn count_subtrees(&mut self, block: DomainHash) -> Result<()> {
        if self.subtree_sizes.contains_key(&block) {
            return Ok(());
        }

        let mut queue = VecDeque::<DomainHash>::new();
        let mut counts = HashMap::<DomainHash, u64>::new();

        queue.push_back(block);
        while !queue.is_empty() {
            let mut current = queue.pop_front().unwrap();
            let children = self.store.get_children(&current)?;
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
                current = self.store.get_parent(&current)?;

                // If the current has default value, it means that the previous
                // `current` was the (virtual) genesis block -- the only block that
                // does not have parents
                if current.has_default_value() {
                    break;
                }

                let entry = counts.entry(current).or_insert(0);
                *entry += 1;
                let children = self.store.get_children(&current)?;

                if *entry != children.len() as u64 {
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

    // propagate_interval propagates a new interval using a BFS traversal.
    // Subtree intervals are recursively allocated according to subtree sizes and
    // the allocation rule in Interval::split_exponential.
    fn propagate_interval(&mut self, block: DomainHash) -> Result<()> {
        // Make sure subtrees are counted before propagating
        self.count_subtrees(block)?;

        let mut queue = VecDeque::<DomainHash>::new();
        queue.push_back(block);
        while !queue.is_empty() {
            let current = queue.pop_front().unwrap();
            let children = self.store.get_children(&current)?;
            if !children.is_empty() {
                let sizes: Vec<u64> = children
                    .iter()
                    .map(|c| self.subtree_sizes[c])
                    .collect();
                let interval = self.store.interval_children_capacity(&current)?;
                let intervals = interval.split_exponential(&sizes);
                for (c, ci) in children.iter().zip(intervals) {
                    self.store.set_interval(c, ci)?;
                }
                queue.extend(children.iter());
            }
        }
        Ok(())
    }

    fn reindex_intervals_earlier_than_root(
        &mut self, allocation_block: DomainHash, common_ancestor: DomainHash, required_allocation: u64,
    ) -> Result<()> {
        todo!()
    }
}
