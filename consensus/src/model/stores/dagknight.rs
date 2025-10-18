use std::{cell::RefCell, collections::HashMap, sync::Arc};

use kaspa_consensus_core::{BlockHashMap, KType};
use kaspa_database::{
    prelude::{DbKey, StoreError},
    registry::DatabaseStorePrefixes,
};
use kaspa_hashes::Hash;

use crate::model::stores::ghostdag::GhostdagData;

pub struct DagknightConflictEntry {
    // TODO: incremental colouring data for relevant k values
}

pub struct DagknightData {
    /// A mapping from conflict roots to incremental conflict data
    pub entries: BlockHashMap<DagknightConflictEntry>,

    /// The selected parent of this block as chosen by the DAGKNIGHT protocol
    pub selected_parent: Hash,
}

pub struct MemoryDagknightStore {
    dk_map: RefCell<HashMap<DagknightKey, Arc<GhostdagData>>>,
}

pub trait DagknightStoreReader {
    fn get_selected_parent(&self, dk_key: DagknightKey) -> Result<Hash, StoreError>;
    fn get_data(&self, dk_key: DagknightKey) -> Result<Arc<GhostdagData>, StoreError>;
}

#[derive(Eq, Hash, Clone)]
pub struct DagknightKey {
    pub pov_hash: Hash,
    pub root_hash: Hash,
    pub k: KType,
}

impl PartialEq for DagknightKey {
    fn eq(&self, other: &Self) -> bool {
        return self.pov_hash == other.pov_hash && self.root_hash == other.root_hash && self.k == other.k;
    }
}

pub trait DagknightStore {
    fn insert(&self, key: DagknightKey, dk_data: Arc<GhostdagData>) -> Result<(), StoreError>;
    fn delete(&self, key: DagknightKey) -> Result<(), StoreError>;
}

impl MemoryDagknightStore {
    pub fn new(dk_map: RefCell<HashMap<DagknightKey, Arc<GhostdagData>>>) -> Self {
        Self { dk_map }
    }
}

impl DagknightStoreReader for MemoryDagknightStore {
    fn get_selected_parent(&self, dk_key: DagknightKey) -> Result<Hash, StoreError> {
        Ok(self.get_data(dk_key)?.selected_parent)
    }

    fn get_data(&self, key: DagknightKey) -> Result<Arc<GhostdagData>, StoreError> {
        if let Some(pov_block_dk_data) = self.dk_map.borrow().get(&key) {
            return Ok(pov_block_dk_data.clone());
        } else {
            // FIXME: for DagKnight prefix
            return Err(StoreError::KeyNotFound(DbKey::new(DatabaseStorePrefixes::Ghostdag.as_ref(), "fixme")));
        };
    }
}

impl DagknightStore for MemoryDagknightStore {
    fn insert(&self, key: DagknightKey, dk_data: Arc<GhostdagData>) -> Result<(), StoreError> {
        self.dk_map.borrow_mut().insert(key, dk_data);

        Ok(())
    }

    fn delete(&self, key: DagknightKey) -> Result<(), StoreError> {
        self.dk_map.borrow_mut().remove(&key);

        Ok(())
    }
}
