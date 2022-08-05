use crate::model::api::hash::{Hash, HashArray};
use crate::model::stores::ghostdag::GhostdagStore;
use crate::model::stores::reachability::ReachabilityStore;
use crate::model::stores::relations::RelationsStore;

use std::collections::{HashSet, VecDeque};

use crate::processes::reachability::inquirer::is_dag_ancestor_of;

use super::protocol::{GhostdagManager, StoreAccess};

impl<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore, V: StoreAccess<T, S, U>> GhostdagManager<T, S, U, V> {
    pub fn ordered_mergeset_without_selected_parent(
        &self, sa: &V, selected_parent: Hash, parents: &HashArray,
    ) -> Vec<Hash> {
        let mut queue: VecDeque<Hash> = parents
            .iter()
            .cloned()
            .filter(|p| *p != selected_parent)
            .collect();
        let mut mergeset = HashSet::<Hash>::from_iter(queue.iter().cloned());
        let mut selected_parent_past: HashSet<Hash> = HashSet::new();

        while let Some(current) = queue.pop_front() {
            let current_parents = sa.relations_store().get_parents(current).unwrap();

            // For each parent of the current block we check whether it is in the past of the selected parent. If not,
            // we add it to the resulting merge-set and queue it for further processing.
            for parent in current_parents.iter() {
                if mergeset.contains(parent) {
                    break;
                }

                if selected_parent_past.contains(parent) {
                    break;
                }

                if is_dag_ancestor_of(sa.reachability_store(), *parent, selected_parent).unwrap() {
                    selected_parent_past.insert(*parent);
                    continue;
                }

                mergeset.insert(*parent);
                queue.push_back(*parent);
            }
        }

        Self::sort_blocks(sa, mergeset)
    }
}
