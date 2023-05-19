use super::ghostdag::mergeset::unordered_mergeset_without_selected_parent;
use crate::model::{services::reachability::ReachabilityService, stores::relations::RelationsStore};
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashes, ORIGIN},
    BlockHashSet,
};
use kaspa_database::prelude::{BatchDbWriter, DbWriter, StoreError};
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Initializes this relations store with an `origin` root
pub fn init<S: RelationsStore + ?Sized>(relations: &mut S) {
    if !relations.has(ORIGIN).unwrap() {
        relations.insert(ORIGIN, BlockHashes::new(vec![])).unwrap();
    }
}

/// Delete relations of `hash` for the case where the relations store represents a specific level.
/// In this case we simply remove the entry locally, relying on the fact that level relations are
/// kept topologically continuous. If any child of this `hash` will remain with no parent, we make
/// sure to connect it to `origin`. Note that apart from the special case of `origin`, these relations
/// are always a subset of the original header relations for this level.
pub fn delete_level_relations<W, S>(mut writer: W, relations: &mut S, hash: Hash)
where
    W: DbWriter,
    S: RelationsStore + ?Sized,
{
    let children = relations.get_children(hash).unwrap();
    for child in children.iter().copied() {
        let child_parents = relations.get_parents(child).unwrap();
        // If the removed hash is the only parent of child, then replace it with `origin`
        let replace_with: &[Hash] = if child_parents.as_slice() == [hash] { &[ORIGIN] } else { &[] };
        relations.replace_parent(&mut writer, child, hash, replace_with).unwrap();
    }
    relations.delete(&mut writer, hash).unwrap();
}

/// Delete relations of `hash` for the case where relations represent the maximally known reachability
/// relations. In this case we preserve all topological info by connecting parents of `hash` as parents
/// of its children if necessary. This means that these relations do not correlate with header data and
/// can contain links which didn't appear in the original DAG (but yet follow from it)
pub fn delete_reachability_relations<W, S, U>(mut writer: W, relations: &mut S, reachability: &U, hash: Hash) -> BlockHashSet
where
    W: DbWriter,
    S: RelationsStore + ?Sized,
    U: ReachabilityService + ?Sized,
{
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
        relations.replace_parent(&mut writer, child, hash, &needed_grandparents).unwrap();
    }
    relations.delete(&mut writer, hash).unwrap();
    mergeset
}

pub trait RelationsStoreExtensions: RelationsStore {
    /// Inserts `parents` into a new store entry for `hash`, and for each `parent ∈ parents` adds `hash` to `parent.children`
    fn insert(&mut self, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.insert_with_writer(self.default_writer(), hash, parents)
    }

    fn insert_batch(&mut self, batch: &mut WriteBatch, hash: Hash, parents: BlockHashes) -> Result<(), StoreError> {
        self.insert_with_writer(BatchDbWriter::new(batch), hash, parents)
    }

    fn insert_with_writer<W>(&mut self, mut writer: W, hash: Hash, parents: BlockHashes) -> Result<(), StoreError>
    where
        W: DbWriter,
    {
        if self.has(hash)? {
            return Err(StoreError::HashAlreadyExists(hash));
        }

        // Insert a new entry for `hash`
        self.set_parents(&mut writer, hash, parents.clone())?;

        // The new hash has no children yet
        self.set_children(&mut writer, hash, BlockHashes::new(Vec::new()))?;

        // Update `children` for each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.set_children(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }

    fn delete<W>(&mut self, mut writer: W, hash: Hash) -> Result<(), StoreError>
    where
        W: DbWriter,
    {
        let parents = self.get_parents(hash)?;
        self.delete_entries(&mut writer, hash)?;

        // Remove `hash` from `children` of each parent
        for parent in parents.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            let index = children
                .iter()
                .copied()
                .position(|h| h == hash)
                .unwrap_or_else(|| panic!("inconsistent child-parent relation, hash: {}, parent: {}", hash, parent,));
            children.swap_remove(index);
            self.set_children(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }

    fn replace_parent<W>(&mut self, mut writer: W, hash: Hash, replaced_parent: Hash, replace_with: &[Hash]) -> Result<(), StoreError>
    where
        W: DbWriter,
    {
        let mut parents = (*self.get_parents(hash)?).clone();
        let replaced_index =
            parents.iter().copied().position(|h| h == replaced_parent).expect("callers must ensure replaced is a parent");
        parents.swap_remove(replaced_index);
        parents.extend(replace_with);
        self.set_parents(&mut writer, hash, BlockHashes::new(parents))?;

        for parent in replace_with.iter().cloned() {
            let mut children = (*self.get_children(parent)?).clone();
            children.push(hash);
            self.set_children(&mut writer, parent, BlockHashes::new(children))?;
        }

        Ok(())
    }
}

impl<S: RelationsStore + ?Sized> RelationsStoreExtensions for S {}
