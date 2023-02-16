use std::sync::Arc;

use consensus_core::{tx::ScriptPublicKeys, BlockHashSet};
use database::prelude::{StoreError, DB};
use kaspa_core::trace;

use crate::{
    model::UtxoSetByScriptPublicKey,
    stores::{
        indexed_utxos::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
        supply::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
    },
    IDENT,
};

#[derive(Clone)]
pub struct Store {
    utxoindex_tips_store: DbUtxoIndexTipsStore,
    circulating_supply_store: DbCirculatingSupplyStore,
    utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            utxoindex_tips_store: DbUtxoIndexTipsStore::new(db.clone()),
            circulating_supply_store: DbCirculatingSupplyStore::new(db.clone()),
            utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore::new(db, 0),
        }
    }

    pub fn get_utxos_by_script_public_key(
        &self,
        script_public_keys: &ScriptPublicKeys,
    ) -> Result<UtxoSetByScriptPublicKey, StoreError> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys(script_public_keys)
    }

    pub fn update_utxo_state(
        &mut self,
        to_add: &UtxoSetByScriptPublicKey,
        to_remove: &UtxoSetByScriptPublicKey,
        try_reset_on_err: bool,
    ) -> Result<(), StoreError> {
        let mut res = Ok(());

        if !to_remove.is_empty() {
            res = self.utxos_by_script_public_key_store.remove_utxo_entries(to_remove);
        };

        if res.is_err() {
            if try_reset_on_err {
                self.delete_all()?;
            }
            return res;
        }

        if !to_add.is_empty() {
            res = self.utxos_by_script_public_key_store.add_utxo_entries(to_add);
        };

        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        };
        res
    }

    pub fn get_circulating_supply(&self) -> Result<u64, StoreError> {
        self.circulating_supply_store.get()
    }

    pub fn add_circulating_supply(&mut self, circulating_supply_diff: u64, try_reset_on_err: bool) -> Result<u64, StoreError> {
        if circulating_supply_diff != 0 {
            let res = self.circulating_supply_store.add_circulating_supply(circulating_supply_diff);
            if try_reset_on_err && res.is_err() {
                self.delete_all()?;
            }
            return res;
        }
        Ok(0u64)
    }

    pub fn insert_circulating_supply(&mut self, circulating_supply: u64, try_reset_on_err: bool) -> Result<(), StoreError> {
        let res = self.circulating_supply_store.insert(circulating_supply);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    pub fn get_tips(&self) -> Result<Arc<BlockHashSet>, StoreError> {
        self.utxoindex_tips_store.get()
    }

    pub fn set_tips(&mut self, tips: BlockHashSet, try_reset_on_err: bool) -> Result<(), StoreError> {
        let res = self.utxoindex_tips_store.set_tips(tips);
        if try_reset_on_err && res.is_err() {
            self.delete_all()?;
        }
        res
    }

    /// Resets the utxoindex database:
    pub fn delete_all(&mut self) -> Result<(), StoreError> {
        // TODO: explore possibility of deleting and replacing whole db, currently there is an issue because of file lock and db being in an arc.
        trace!("[{0}] attempting to clear utxoindex database...", IDENT);

        // Clear all
        self.utxoindex_tips_store.remove()?;
        self.circulating_supply_store.remove()?;
        self.utxos_by_script_public_key_store.delete_all()?;

        trace!("[{0}] clearing utxoindex database - success!", IDENT);

        Ok(())
    }
}
