use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

use kaspa_consensus_core::{BlueWorkType, KType};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB, DbKey, StoreError};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_math::Uint192;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

// ============================================================================
// Persistence Key
// ============================================================================

/// Key for UMC cascade checkpoint persistence.
///
/// Layout: conflict_genesis(32) || k(u16 BE) || next_chain_ancestor(32) || current_chain_block(32)
/// Total: 98 bytes
///
/// K-coloring partition is fixed per chain block (determined at block creation time),
/// so (CG, K, NCA, CB) uniquely identifies the checkpoint state.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct UmcCascadeKey {
    pub conflict_genesis: Hash,
    pub k: KType,
    pub next_chain_ancestor: Hash,
    pub current_chain_block: Hash,
    /// Precomputed bytes
    bytes: [u8; 98],
}

impl UmcCascadeKey {
    pub fn new(conflict_genesis: Hash, k: KType, next_chain_ancestor: Hash, current_chain_block: Hash) -> Self {
        let mut bytes = [0u8; 98];
        bytes[..32].copy_from_slice(conflict_genesis.as_ref());
        bytes[32..34].copy_from_slice(&k.to_be_bytes());
        bytes[34..66].copy_from_slice(next_chain_ancestor.as_ref());
        bytes[66..98].copy_from_slice(current_chain_block.as_ref());
        Self { conflict_genesis, k, next_chain_ancestor, current_chain_block, bytes }
    }
}

impl std::fmt::Display for UmcCascadeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.bytes)
    }
}

impl AsRef<[u8]> for UmcCascadeKey {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

/// A mergeset from one level of the virtual GD chain.
#[derive(Debug, Clone)]
pub struct Mergeset {
    /// The chain block whose stored gd produced this mergeset (None for the virtual mergeset).
    pub merging_chain_block: Option<Hash>,
    /// Blue blocks in this mergeset, assumed to be in topological order
    pub mergeset_blues: Vec<(Hash, BlueWorkType)>,
    /// Red blocks in this mergeset (may include grays - caller must filter), also assumed to be in topological order
    pub mergeset_reds: Vec<(Hash, BlueWorkType)>,
}

// ============================================================================
// Persisted State
// ============================================================================

/// Leaf data for a chain score tree, serialized for checkpoint persistence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainLeafEntry {
    pub hash: Hash,
    pub work: BlueWorkType,
    /// Absolute score at checkpoint time (positive value + sign flag)
    pub score_abs: Uint192,
    pub score_negative: bool,
}

/// Persisted checkpoint state for UMC cascade at a specific chain block.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UmcCascadePersistedState {
    pub blues_chains_decomposition: Vec<Vec<Hash>>,
    pub chains_leaves: Vec<Vec<ChainLeafEntry>>,
    pub blk_mapping_to_chains: HashMap<Hash, usize>,
    pub deficit_work: Uint192,
    pub blue_work: Uint192,
    pub red_work: Uint192,
    pub negative_blue_work: Uint192,
    /// Number of voting blocks processed up to this checkpoint
    pub voting_blocks: u64,
    /// Total bucket flips observed up to this checkpoint
    pub flip_count: u64,
}

impl MemSizeEstimator for UmcCascadePersistedState {
    fn estimate_mem_bytes(&self) -> usize {
        let mut bytes = size_of::<Self>();
        bytes += self.blues_chains_decomposition.iter().map(|c| c.len() * size_of::<Hash>()).sum::<usize>();
        bytes += self.chains_leaves.iter().map(|l| l.len() * size_of::<ChainLeafEntry>()).sum::<usize>();
        bytes += self.blk_mapping_to_chains.len() * size_of::<(Hash, usize)>();
        bytes
    }
}

// ============================================================================
// Store Traits
// ============================================================================

pub trait UmcCascadeStoreReader {
    fn get_checkpoint(&self, key: UmcCascadeKey) -> Result<Option<UmcCascadePersistedState>, StoreError>;
}

pub trait UmcCascadeStore: UmcCascadeStoreReader {
    fn insert_checkpoint(&self, key: UmcCascadeKey, state: UmcCascadePersistedState) -> Result<(), StoreError>;
    fn prune_by_conflict_genesis(&self, batch: &mut WriteBatch, conflict_genesis: Hash) -> Result<u32, StoreError>;
}

// ============================================================================
// Memory Store
// ============================================================================

#[derive(Clone, Default)]
pub struct MemoryUmcCascadeStore {
    map: Arc<RwLock<HashMap<UmcCascadeKey, UmcCascadePersistedState>>>,
}

impl MemoryUmcCascadeStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl UmcCascadeStoreReader for MemoryUmcCascadeStore {
    fn get_checkpoint(&self, key: UmcCascadeKey) -> Result<Option<UmcCascadePersistedState>, StoreError> {
        Ok(self.map.read().get(&key).cloned())
    }
}

impl UmcCascadeStore for MemoryUmcCascadeStore {
    fn insert_checkpoint(&self, key: UmcCascadeKey, state: UmcCascadePersistedState) -> Result<(), StoreError> {
        self.map.write().insert(key, state);
        Ok(())
    }

    fn prune_by_conflict_genesis(&self, _batch: &mut WriteBatch, conflict_genesis: Hash) -> Result<u32, StoreError> {
        let mut map = self.map.write();
        let before = map.len();
        map.retain(|key, _| key.conflict_genesis != conflict_genesis);
        Ok((before - map.len()) as u32)
    }
}

// ============================================================================
// Database Store
// ============================================================================

#[derive(Clone)]
pub struct DbUmcCascadeStore {
    db: Arc<DB>,
    access: CachedDbAccess<UmcCascadeKey, UmcCascadePersistedState>,
}

impl DbUmcCascadeStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        let prefix: Vec<u8> = kaspa_database::registry::DatabaseStorePrefixes::DagKnightUMC.into();
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, prefix) }
    }
}

impl UmcCascadeStoreReader for DbUmcCascadeStore {
    fn get_checkpoint(&self, key: UmcCascadeKey) -> Result<Option<UmcCascadePersistedState>, StoreError> {
        match self.access.read(key) {
            Ok(state) => Ok(Some(state)),
            Err(StoreError::KeyNotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl UmcCascadeStore for DbUmcCascadeStore {
    fn insert_checkpoint(&self, key: UmcCascadeKey, state: UmcCascadePersistedState) -> Result<(), StoreError> {
        let mut batch = WriteBatch::default();
        self.access.write(BatchDbWriter::new(&mut batch), key, state)?;
        self.db.write(batch)?;
        Ok(())
    }

    fn prune_by_conflict_genesis(&self, batch: &mut WriteBatch, conflict_genesis: Hash) -> Result<u32, StoreError> {
        // Build range prefix: DagKnightUMC_prefix || conflict_genesis
        let prefix: Vec<u8> = kaspa_database::registry::DatabaseStorePrefixes::DagKnightUMC.into();
        let mut start = prefix.clone();
        start.extend_from_slice(conflict_genesis.as_ref());
        // End is start with last byte incremented (or FF)
        let mut end = start.clone();
        if let Some(last) = end.last_mut() {
            *last = last.saturating_add(1);
        }
        let mut count = 0u32;
        let mut iter = self.db.raw_iterator();
        iter.seek(&start);
        while iter.valid() {
            let key = iter.key().ok_or(StoreError::KeyNotFound(DbKey::new(
                DatabaseStorePrefixes::DagKnightUMC.as_ref(),
                UmcCascadeKey::new(Hash::from_u64_word(0), 0, Hash::from_u64_word(0), Hash::from_u64_word(0)),
            )))?;
            if key >= end.as_slice() {
                break;
            }
            count += 1;
            iter.next();
        }
        batch.delete_range(start, end);
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(cg: u64, k: KType, nca: u64, cb: u64) -> UmcCascadeKey {
        UmcCascadeKey::new(Hash::from_u64_word(cg), k, Hash::from_u64_word(nca), Hash::from_u64_word(cb))
    }

    fn state(voting_blocks: u64) -> UmcCascadePersistedState {
        UmcCascadePersistedState {
            blues_chains_decomposition: vec![],
            chains_leaves: vec![],
            blk_mapping_to_chains: HashMap::new(),
            deficit_work: Uint192::ZERO,
            blue_work: Uint192::ZERO,
            red_work: Uint192::ZERO,
            negative_blue_work: Uint192::ZERO,
            voting_blocks,
            flip_count: 0,
        }
    }

    #[test]
    fn test_prune_by_conflict_genesis() {
        let store = MemoryUmcCascadeStore::new();
        store.insert_checkpoint(key(1, 1, 2, 3), state(1)).unwrap();
        store.insert_checkpoint(key(1, 2, 2, 4), state(5)).unwrap();
        store.insert_checkpoint(key(2, 1, 3, 5), state(2)).unwrap();

        let mut batch = WriteBatch::default();
        let deleted = store.prune_by_conflict_genesis(&mut batch, Hash::from_u64_word(1)).unwrap();

        assert_eq!(deleted, 2, "both CG-1 checkpoints must be removed");
        assert!(store.get_checkpoint(key(1, 1, 2, 3)).unwrap().is_none());
        assert!(store.get_checkpoint(key(1, 2, 2, 4)).unwrap().is_none());
        assert!(store.get_checkpoint(key(2, 1, 3, 5)).unwrap().is_some(), "other conflict genesis must be preserved");
    }
}
