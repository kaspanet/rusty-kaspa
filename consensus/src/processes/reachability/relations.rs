use itertools::Itertools;
use kaspa_consensus_core::{BlockHashSet, blockhash::BlockHashes};
use kaspa_database::prelude::{ReadLock, StoreError, StoreResult};
use kaspa_hashes::Hash;

use crate::model::{services::reachability::ReachabilityService, stores::relations::RelationsStoreReader};

/// A relations-store reader restricted to the future of a fixed root block (including the root).
///
/// Only parents and children that lie within the root’s future are exposed.
/// This provides a consistent, root-relative view of relations when operating on
/// proofs or subgraphs confined to that region of the DAG.
#[derive(Clone)]
pub struct FutureIntersectRelations<T: RelationsStoreReader, U: ReachabilityService> {
    relations_store: T,
    reachability_service: U,
    root: Hash,
}

impl<T: RelationsStoreReader, U: ReachabilityService> FutureIntersectRelations<T, U> {
    pub fn new(relations_store: T, reachability_service: U, root: Hash) -> Self {
        Self { relations_store, reachability_service, root }
    }
}

impl<T: RelationsStoreReader, U: ReachabilityService> RelationsStoreReader for FutureIntersectRelations<T, U> {
    fn get_parents(&self, hash: Hash) -> Result<BlockHashes, StoreError> {
        self.relations_store.get_parents(hash).map(|hashes| {
            // Reachability queries are safe here, since in this context all blocks are reached via `reachable_parents_at_level`
            hashes.iter().copied().filter(|&h| self.reachability_service.is_dag_ancestor_of(self.root, h)).collect_vec().into()
        })
    }

    fn get_children(&self, hash: Hash) -> StoreResult<ReadLock<BlockHashSet>> {
        assert!(self.reachability_service.is_dag_ancestor_of(self.root, hash), "future(root) invariant violated");
        self.relations_store.get_children(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, StoreError> {
        Ok(self.relations_store.has(hash)? && self.reachability_service.is_dag_ancestor_of(self.root, hash))
    }

    fn counts(&self) -> Result<(usize, usize), StoreError> {
        unreachable!("not expected to be called in this context")
    }
}
