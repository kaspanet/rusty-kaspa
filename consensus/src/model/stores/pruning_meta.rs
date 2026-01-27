use std::sync::Arc;

use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::StoreResultExt;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

use super::utxo_set::DbUtxoSetStore;

/// Used in order to group stores related to the pruning point utxoset under a single lock
pub struct PruningMetaStores {
    pub utxo_set: DbUtxoSetStore,
    utxoset_position_access: CachedDbItem<Hash>,
    utxoset_stable_flag_access: CachedDbItem<bool>,
    body_missing_anticone_blocks: CachedDbItem<Vec<Hash>>,
}

impl PruningMetaStores {
    pub fn new(db: Arc<DB>, utxoset_cache_policy: CachePolicy) -> Self {
        Self {
            utxo_set: DbUtxoSetStore::new(db.clone(), utxoset_cache_policy, DatabaseStorePrefixes::PruningUtxoset.into()),
            utxoset_position_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetPosition.into()),
            utxoset_stable_flag_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetSyncFlag.into()),
            body_missing_anticone_blocks: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::BodyMissingAnticone.into()),
        }
    }

    /// Represents the exact point of the current pruning point utxoset. Used in order to safely
    /// progress the pruning point utxoset in batches and to allow recovery if the process crashes
    /// during the pruning point utxoset movement
    pub fn utxoset_position(&self) -> StoreResult<Hash> {
        self.utxoset_position_access.read()
    }

    pub fn set_utxoset_position(&mut self, batch: &mut WriteBatch, pruning_utxoset_position: Hash) -> StoreResult<()> {
        self.utxoset_position_access.write(BatchDbWriter::new(batch), &pruning_utxoset_position)
    }

    /// Flip the sync flag in the same batch as your other writes
    pub fn set_pruning_utxoset_stable_flag(&mut self, batch: &mut WriteBatch, stable: bool) -> StoreResult<()> {
        self.utxoset_stable_flag_access.write(BatchDbWriter::new(batch), &stable)
    }

    /// Read the flag; default to true if missing - this is important because a node upgrading should have this value true
    /// as all non staging consensuses had a stable utxoset previously
    pub fn pruning_utxoset_stable_flag(&self) -> bool {
        self.utxoset_stable_flag_access.read().optional().unwrap().unwrap_or(true)
    }

    /// Represents blocks in the anticone of the current pruning point which may lack a block body
    /// These blocks need to be kept track of as they require trusted validation,
    /// so that downloading of further blocks on top of them could resume
    pub fn set_body_missing_anticone(&mut self, batch: &mut WriteBatch, body_missing_anticone: Vec<Hash>) -> StoreResult<()> {
        self.body_missing_anticone_blocks.write(BatchDbWriter::new(batch), &body_missing_anticone)
    }

    /// Default to empty if missing - this is important because a node upgrading should have this value empty
    /// since all non staging consensuses had no missing body anticone previously
    pub fn get_body_missing_anticone(&self) -> Vec<Hash> {
        self.body_missing_anticone_blocks.read().optional().unwrap().unwrap_or(vec![])
    }

    // check if there are any body missing blocks remaining in the anticone of the current pruning point
    pub fn is_anticone_fully_synced(&self) -> bool {
        self.get_body_missing_anticone().is_empty()
    }

    pub fn is_in_transitional_ibd_state(&self) -> bool {
        !self.is_anticone_fully_synced() || !self.pruning_utxoset_stable_flag()
    }
}
