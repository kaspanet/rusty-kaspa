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
pub struct PruningUtxosetStores {
    pub utxo_set: DbUtxoSetStore,
    utxoset_position_access: CachedDbItem<Hash>,
    utxoset_sync_flag_access: CachedDbItem<bool>,
}

impl PruningUtxosetStores {
    pub fn new(db: Arc<DB>, utxoset_cache_policy: CachePolicy) -> Self {
        Self {
            utxo_set: DbUtxoSetStore::new(db.clone(), utxoset_cache_policy, DatabaseStorePrefixes::PruningUtxoset.into()),
            utxoset_position_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetPosition.into()),
            utxoset_sync_flag_access: CachedDbItem::new(db.clone(), DatabaseStorePrefixes::PruningUtxosetSyncFlag.into()),
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
    pub fn set_sync_flag(&mut self, batch: &mut WriteBatch, synced: bool) -> StoreResult<()> {
        self.utxoset_sync_flag_access.write(BatchDbWriter::new(batch), &synced)
    }

    /// Read the flag; default to false if missing
    pub fn sync_flag(&self) -> StoreResult<bool> {
        self.utxoset_sync_flag_access.read().or(Ok(false))
    }
}
