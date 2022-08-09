use std::collections::{HashSet, VecDeque};

use consensus_core::blockhash::BlockHashes;
use hashes::Hash;

use crate::model::services::reachability::ReachabilityService;
use crate::model::stores::ghostdag::GhostdagStoreReader;
use crate::model::stores::relations::RelationsStoreReader;

use super::protocol::GhostdagManager;

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService> GhostdagManager<T, S, U> {
    pub fn ordered_mergeset_without_selected_parent(&self, selected_parent: Hash, parents: &BlockHashes) -> Vec<Hash> {
        let mut queue: VecDeque<Hash> = parents
            .iter()
            .cloned()
            .filter(|p| *p != selected_parent)
            .collect();
        let mut mergeset = HashSet::<Hash>::from_iter(queue.iter().cloned());
        let mut selected_parent_past: HashSet<Hash> = HashSet::new();

        while let Some(current) = queue.pop_front() {
            let current_parents = self.relations_store.get_parents(current).unwrap();

            // For each parent of the current block we check whether it is in the past of the selected parent. If not,
            // we add it to the resulting merge-set and queue it for further processing.
            for parent in current_parents.iter() {
                if mergeset.contains(parent) {
                    break;
                }

                if selected_parent_past.contains(parent) {
                    break;
                }

                if self
                    .reachability_service
                    .is_dag_ancestor_of(*parent, selected_parent)
                {
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
