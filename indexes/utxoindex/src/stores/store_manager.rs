use std::{collections::HashSet, sync::Arc};

use kaspa_consensus_core::{
    BlockHashSet,
    tx::{ScriptPublicKey, ScriptPublicKeys, TransactionOutpoint},
};
use kaspa_core::trace;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, DB, StoreResult};
use kaspa_hashes::Hash;
use kaspa_index_core::indexed_utxos::{BalanceByScriptPublicKey, UtxoReferenceEntry};
use rocksdb::WriteBatch;

use crate::{
    IDENT,
    model::UtxoSetByScriptPublicKey,
    stores::{
        indexed_utxos::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
        indexed_utxos_by_covenant::{DbUtxoSetByCovenantIdStore, UtxoSetByCovenantIdStore, UtxoSetByCovenantIdStoreReader},
        supply::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
    },
};

#[derive(Clone)]
pub struct Store {
    db: Arc<DB>,
    utxoindex_tips_store: DbUtxoIndexTipsStore,
    circulating_supply_store: DbCirculatingSupplyStore,
    utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore,
    utxos_by_covenant_id_store: DbUtxoSetByCovenantIdStore,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db: db.clone(),
            utxoindex_tips_store: DbUtxoIndexTipsStore::new(db.clone()),
            circulating_supply_store: DbCirculatingSupplyStore::new(db.clone()),
            utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty),
            utxos_by_covenant_id_store: DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty),
        }
    }

    pub fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys(script_public_keys)
    }

    pub fn get_balance_by_script_public_key(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_balance_from_script_public_keys(script_public_keys)
    }

    pub fn get_utxos_by_covenant_id(
        &self,
        covenant_id: Hash,
        script_public_key: Option<ScriptPublicKey>,
    ) -> StoreResult<Vec<UtxoReferenceEntry>> {
        let access_keys = self.utxos_by_covenant_id_store.get_utxo_access_keys_from_covenant_id(covenant_id, script_public_key)?;
        self.utxos_by_script_public_key_store.get_utxo_reference_entries(access_keys)
    }

    // This can have a big memory footprint, so it should be used only for tests.
    pub fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        self.utxos_by_script_public_key_store.get_all_outpoints()
    }

    pub fn update_utxo_state(
        &mut self,
        to_add: &UtxoSetByScriptPublicKey,
        to_remove: &UtxoSetByScriptPublicKey,
        // only applied on batch write error
        // @TODO(izio): validate desired behavior
        try_reset_on_err: bool,
    ) -> StoreResult<()> {
        // A UTXO entry can appear both in removed and in added (if the DAA score of the entry changed). Thus
        // we must first apply removals and then additions (so it will be re-added in the addition phase)
        // Each mutations are applied on the same writer and executed as part of the same batch
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

        self.utxos_by_script_public_key_store.remove_utxo_entries(&mut writer, to_remove)?;
        self.utxos_by_covenant_id_store.remove_utxo_entries(&mut writer, to_remove)?;

        // Now apply additions
        self.utxos_by_script_public_key_store.add_utxo_entries(&mut writer, to_add)?;
        self.utxos_by_covenant_id_store.add_utxo_entries(&mut writer, to_add)?;

        let res = self.db.write(batch).map_err(Into::into);

        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        };

        res
    }

    pub fn get_circulating_supply(&self) -> StoreResult<u64> {
        self.circulating_supply_store.get()
    }

    pub fn update_circulating_supply(&mut self, circulating_supply_diff: i64, try_reset_on_err: bool) -> StoreResult<u64> {
        let res = self.circulating_supply_store.update_circulating_supply(circulating_supply_diff);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn insert_circulating_supply(&mut self, circulating_supply: u64, try_reset_on_err: bool) -> StoreResult<()> {
        let res = self.circulating_supply_store.insert(circulating_supply);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn get_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.utxoindex_tips_store.get()
    }

    pub fn set_tips(&mut self, tips: BlockHashSet, try_reset_on_err: bool) -> StoreResult<()> {
        let res = self.utxoindex_tips_store.set_tips(tips);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    /// Resets the utxoindex database:
    pub fn delete_all(&mut self) -> StoreResult<()> {
        // TODO: explore possibility of deleting and replacing whole db, currently there is an issue because of file lock and db being in an arc.
        trace!("[{0}] attempting to clear utxoindex database...", IDENT);

        // Clear all
        self.utxoindex_tips_store.remove()?;
        self.circulating_supply_store.remove()?;
        self.utxos_by_script_public_key_store.delete_all()?;
        self.utxos_by_covenant_id_store.delete_all()?;

        trace!("[{0}] clearing utxoindex database - success!", IDENT);

        Ok(())
    }
}
