use super::protocol::GhostdagManager;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::relations::RelationsStoreReader;
use crate::model::{services::reachability::ReachabilityService, stores::headers::HeaderStoreReader};
use hashes::Hash;
use std::collections::{HashSet, VecDeque};

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn ordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &[Hash]) -> Vec<Hash> {
        let mut queue: VecDeque<_> = parents.iter().copied().filter(|p| p != &selected_parent).collect();
        let mut mergeset: HashSet<_> = queue.iter().copied().collect();
        let mut selected_parent_past = HashSet::new();

        while let Some(current) = queue.pop_front() {
            let current_parents = self.relations_store.get_parents(current).unwrap();

            // For each parent of the current block we check whether it is in the past of the selected parent. If not,
            // we add it to the resulting merge-set and queue it for further processing.
            for parent in current_parents.iter() {
                if mergeset.contains(parent) {
                    continue;
                }

                if selected_parent_past.contains(parent) {
                    continue;
                }

                if self.reachability_service.is_dag_ancestor_of(*parent, selected_parent) {
                    selected_parent_past.insert(*parent);
                    continue;
                }

                mergeset.insert(*parent);
                queue.push_back(*parent);
            }
        }

        self.sort_blocks(mergeset)
    }
}
