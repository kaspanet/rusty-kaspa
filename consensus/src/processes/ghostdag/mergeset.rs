use crate::model::api::hash::{Hash, HashArray};
use crate::model::stores::ghostdag::GhostdagStore;
use crate::model::stores::reachability::ReachabilityStore;
use crate::model::stores::relations::RelationsStore;

use std::collections::{HashSet, VecDeque};

use crate::processes::reachability::inquirer::is_dag_ancestor_of;

use super::protocol::{GhostdagManager, StoreAccess};

impl<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore, V: StoreAccess<T, S, U>> GhostdagManager<T, S, U, V> {
    pub fn mergeset_without_selected_parent(&self, sa: &V, selected_parent: &Hash, parents: &HashArray) -> Vec<Hash> {
        let mut mergeset_set: HashSet<Hash> = HashSet::with_capacity(self.k.into());
        let mut selected_parent_past: HashSet<Hash> = HashSet::new();
        let mut queue: VecDeque<Hash> = VecDeque::new();

        for parent in parents.iter() {
            if parent == selected_parent {
                continue;
            }

            mergeset_set.insert(*parent);
            queue.push_back(*parent);
        }

        loop {
            let current = queue.pop_front();

            match current {
                Some(current) => {
                    let current_parents = sa.relations_store().get_parents(&current);

                    // For each parent of the current block we check whether it is in the past of the selected parent. If not,
                    // we add it to the resulting merge-set and queue it for further processing.
                    for parent in current_parents.unwrap().iter() {
                        if mergeset_set.contains(parent) {
                            break;
                        }

                        if selected_parent_past.contains(parent) {
                            break;
                        }

                        if is_dag_ancestor_of(sa.reachability_store(), *parent, *selected_parent).unwrap() {
                            selected_parent_past.insert(*parent);
                            continue;
                        }

                        mergeset_set.insert(*parent);
                        queue.push_back(*parent);
                    }
                }
                None => break,
            }
        }

        Self::sort_blocks(sa, mergeset_set)
    }
}
