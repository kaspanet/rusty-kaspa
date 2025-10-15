use std::sync::Arc;

use kaspa_database::prelude::CachePolicy;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

use super::utxo_set::DbUtxoSetStore;

/// Used in order to group stores related to the pruning point utxoset under a single lock
pub struct PruningMetaStores {
    pub utxo_set: DbUtxoSetStore,
    utxoset_position_access: CachedDbItem<Hash>,
    utxoset_sync_flag_access: CachedDbItem<bool>,
    disembodied_anticone_blocks: CachedDbItem<Vec<Hash>>,
}

impl PruningMetaStores {
    pub fn new(db: Arc<DB>, utxoset_cache_policy: CachePolicy) -> Self {
        Self {
            utxo_set: DbUtxoSetStore::new(db.clone(), utxoset_cache_policy, DatabaseStorePrefixes::PruningUtxoset.into()),
            utxoset_position_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetPosition.into()),
            utxoset_sync_flag_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetSyncFlag.into()),
            disembodied_anticone_blocks: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::DisembodiedAnticoneBlocks.into()),
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
    pub fn set_utxo_sync_flag(&mut self, batch: &mut WriteBatch, synced: bool) -> StoreResult<()> {
        self.utxoset_sync_flag_access.write(BatchDbWriter::new(batch), &synced)
    }

    /// Read the flag; default to true if missing, which corresponds to a new consensus
    pub fn utxo_sync_flag(&self) -> StoreResult<bool> {
        self.utxoset_sync_flag_access.read().or(Ok(true))
    }

    /// Represents blocks in the anticone of the current pruning point which may lack a block body
    /// These blocks need to be kept track of as they require trusted validation,
    /// so that downloading of further blocks on top of them could resume
    pub fn set_disembodied_anticone(&mut self, batch: &mut WriteBatch, disembodied_anticone: Vec<Hash>) -> StoreResult<()> {
        self.disembodied_anticone_blocks.write(BatchDbWriter::new(batch), &disembodied_anticone)
    }

    pub fn get_disembodied_anticone(&self) -> StoreResult<Vec<Hash>> {
        self.disembodied_anticone_blocks.read()
    }

    // check if there are any disembodied blocks remanining in the anticone of the current pruning point
    pub fn is_anticone_fully_synced(&self) -> bool {
        self.disembodied_anticone_blocks.read().unwrap_or_default().is_empty()
    }
}
