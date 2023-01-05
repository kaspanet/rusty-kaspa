use consensus::model::stores::database::prelude::*;
use consensus_core::{
    tx::{TransactionIndexType, TransactionOutpoint, UtxoEntry},
    utxo::{
        utxo_diff::{ImmutableUtxoDiff, UtxoDiff},
        utxo_view::UtxoView,
    },
};
use consensus::model::stores::errors::*;
use hashes::Hash;
use rocksdb::WriteBatch;
use std::{fmt::Display, sync::Arc};
use consensus::model::stores::DB;

use super::model::{UtxoIndexedUtxoEntry, UtxoIndexedUtxoEntryCollection, UtxosByScriptPublicKey};

pub const STORE_NAME: &[u8] = b"utxos-by-script-public-key";
pub trait UtxosByScriptPublicKeyStoreReader {
    fn get(&self, outpoint: &TransactionOutpoint) -> Result<Arc<UtxoEntry>, StoreError>;
    // TODO: UTXO entry iterator
}

pub trait UtxosByScriptPublicKeyStore: UtxosByScriptPublicKeyStoreReader {
    /// Updates the store according to the UTXO diff -- adding and deleting entries correspondingly.
    /// Note we define `self` as `mut` in order to require write access even though the compiler does not require it.
    /// This is because concurrent readers can interfere with cache consistency.  
    fn write_diff(&mut self, utxo_diff: &UtxoIndex) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbUtxosByScriptPublicKeyStore {
    db: Arc<DB>,
    access: CachedDbAccess<UtxoKey, Arc<UtxoEntry>>,
}

impl DbUtxosByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(Arc::clone(&db), cache_size, STORE_NAME) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size, self.prefix)
    }

    /// See comment at [`UtxosByScriptPublicKeyStore::write_diff`]
    pub fn write_diff_batch(&mut self, batch: &mut WriteBatch, utxo_diff: &impl ImmutableUtxoDiff) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_many(&mut writer, &mut utxo_diff.removed().keys().map(|o| (*o).into()))?;
        self.access.write_many(&mut writer, &mut utxo_diff.added().iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
        Ok(())
    }
}

impl UtxoView for DbUtxosByScriptPublicKeyStore {
    fn get(&self, script_public_key: &ScriptPublicKey) -> Option<PartialUtxoEntr> {
        UtxosByScriptPublicKeyStoreReader::get(self, outpoint).map(|v| v.as_ref().clone()).unwrap_option()
    }
}

impl UtxosByScriptPublicKeyStoreReader for DbUtxosByScriptPublicKeyStore {
    fn get(&self, script_public_key: &ScriptPublicKey) -> Result<Arc<UtxoEntry>, StoreError> {
        self.access.read((*outpoint).into())
    }
}

impl UtxosByScriptPublicKeyStore for DbUtxosByScriptPublicKeyStore {
    fn write_diff(&mut self, utxo_diff: &UtxoDiff) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.delete_many(&mut writer, &mut utxo_diff.removed().keys().map(|o| (*o).into()))?;
        self.access.write_many(&mut writer, &mut utxo_diff.added().iter().map(|(o, e)| ((*o).into(), Arc::new(e.clone()))))?;
        Ok(())
    }
}
