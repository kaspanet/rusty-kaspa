use crate::model::stores::relations::RelationsStoreReader;
use hashes::Hash;
use parking_lot::RwLock;
use std::sync::Arc;

/// Multi-threaded block-relations service imp
#[derive(Clone)]
pub struct MTRelationsService<T: RelationsStoreReader> {
    store: Arc<RwLock<T>>,
}

impl<T: RelationsStoreReader> MTRelationsService<T> {
    pub fn new(store: Arc<RwLock<T>>) -> Self {
        Self { store }
    }
}

impl<T: RelationsStoreReader> RelationsStoreReader for MTRelationsService<T> {
    fn get_parents(&self, hash: Hash) -> Result<consensus_core::blockhash::BlockHashes, crate::model::stores::errors::StoreError> {
        self.store.read().get_parents(hash)
    }

    fn get_children(&self, hash: Hash) -> Result<consensus_core::blockhash::BlockHashes, crate::model::stores::errors::StoreError> {
        self.store.read().get_children(hash)
    }

    fn has(&self, hash: Hash) -> Result<bool, crate::model::stores::errors::StoreError> {
        self.store.read().has(hash)
    }

    fn get_parents_by_level(
        &self,
        hash: Hash,
        level: consensus_core::BlockLevel,
    ) -> Result<consensus_core::blockhash::BlockHashes, crate::model::stores::errors::StoreError> {
        self.store.read().get_parents_by_level(hash, level)
    }

    fn get_children_by_level(
        &self,
        hash: Hash,
        level: consensus_core::BlockLevel,
    ) -> Result<consensus_core::blockhash::BlockHashes, crate::model::stores::errors::StoreError> {
        self.store.read().get_children_by_level(hash, level)
    }

    fn has_by_level(&self, hash: Hash, level: consensus_core::BlockLevel) -> Result<bool, crate::model::stores::errors::StoreError> {
        self.store.read().has_by_level(hash, level)
    }
}
