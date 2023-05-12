use super::ghostdag::mergeset::unordered_mergeset_without_selected_parent;
use crate::model::{services::reachability::ReachabilityService, stores::relations::RelationsStore};
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashes, ORIGIN},
    BlockHashSet,
};
use kaspa_hashes::Hash;

pub fn init<S: RelationsStore + ?Sized>(relations: &mut S) {
    if !relations.has(ORIGIN).unwrap() {
        relations.insert(ORIGIN, BlockHashes::new(vec![])).unwrap();
    }
}

pub fn delete_level_relations<S: RelationsStore + ?Sized>(relations: &mut S, hash: Hash) {
    let children = relations.get_children(hash).unwrap();
    for child in children.iter().copied() {
        relations.replace_parent(child, hash, &[]).unwrap();
    }
    relations.delete(hash).unwrap();
}

pub fn delete_reachability_relations<S: RelationsStore + ?Sized, U: ReachabilityService + ?Sized>(
    relations: &mut S,
    reachability: &U,
    hash: Hash,
) -> BlockHashSet {
    let selected_parent = reachability.get_chain_parent(hash);
    let parents = relations.get_parents(hash).unwrap();
    let children = relations.get_children(hash).unwrap();
    let mergeset = unordered_mergeset_without_selected_parent(relations, reachability, selected_parent, &parents);
    for child in children.iter().copied() {
        let other_parents = relations.get_parents(child).unwrap().iter().copied().filter(|&p| p != hash).collect_vec();
        let needed_grandparents = parents
            .iter()
            .copied()
            .filter(|&grandparent| {
                // Find grandparents of `v` which are not in the past of any of current parents of `v` (other than `current`)
                !reachability.is_dag_ancestor_of_any(grandparent, &mut other_parents.iter().copied())
            })
            .collect_vec();
        // Replace `hash` with needed grandparents
        relations.replace_parent(child, hash, &needed_grandparents).unwrap();
    }
    relations.delete(hash).unwrap();
    mergeset
}
