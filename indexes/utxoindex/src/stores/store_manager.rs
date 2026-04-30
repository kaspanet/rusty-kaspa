use std::{collections::HashSet, sync::Arc};

use kaspa_consensus_core::{
    BlockHashSet,
    tx::{ScriptPublicKeys, TransactionOutpoint},
};
use kaspa_core::trace;
use kaspa_database::prelude::{CachePolicy, DB, StoreResult};
use kaspa_index_core::indexed_utxos::{BalanceByScriptPublicKey, OrderedUtxoSetByScriptPublicKey};

use crate::{
    IDENT,
    errors::{UtxoIndexError, UtxoIndexResult},
    model::UtxoSetByScriptPublicKey,
    stores::{
        db_version::{DbUtxoIndexDbVersionStore, UtxoIndexDbVersionStore, UtxoIndexDbVersionStoreReader},
        indexed_utxos::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
        supply::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
    },
};

pub const UTXOINDEX_DB_VERSION: u16 = 1;

#[derive(Clone)]
pub struct Store {
    utxoindex_tips_store: DbUtxoIndexTipsStore,
    circulating_supply_store: DbCirculatingSupplyStore,
    db_version_store: DbUtxoIndexDbVersionStore,
    utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            utxoindex_tips_store: DbUtxoIndexTipsStore::new(db.clone()),
            circulating_supply_store: DbCirculatingSupplyStore::new(db.clone()),
            db_version_store: DbUtxoIndexDbVersionStore::new(db.clone()),
            utxos_by_script_public_key_store: DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty),
        }
    }

    pub fn has_legacy_db_version(&self) -> UtxoIndexResult<bool> {
        let tips_exist = match self.utxoindex_tips_store.get() {
            Ok(_) => true,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => false,
            Err(err) => return Err(UtxoIndexError::StoreAccessError(err)),
        };

        let supply_exists = match self.circulating_supply_store.get() {
            Ok(_) => true,
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => false,
            Err(err) => return Err(UtxoIndexError::StoreAccessError(err)),
        };

        match self.db_version_store.get() {
            Ok(version) => Ok(version != UTXOINDEX_DB_VERSION),
            Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => Ok(tips_exist || supply_exists),
            Err(err) => Err(UtxoIndexError::StoreAccessError(err)),
        }
    }

    pub fn ensure_db_version_current(&mut self) -> UtxoIndexResult<()> {
        match self.db_version_store.get() {
            Ok(version) if version == UTXOINDEX_DB_VERSION => Ok(()),
            Ok(_) | Err(kaspa_database::prelude::StoreError::KeyNotFound(_)) => {
                self.db_version_store.set(UTXOINDEX_DB_VERSION)?;
                Ok(())
            }
            Err(err) => Err(UtxoIndexError::StoreAccessError(err)),
        }
    }

    pub fn get_utxos_by_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys(script_public_keys)
    }

    pub fn get_utxos_by_script_public_keys_by_daa_score(
        &self,
        script_public_keys: ScriptPublicKeys,
        from_daa_score: Option<u64>,
        to_daa_score: Option<u64>,
    ) -> StoreResult<OrderedUtxoSetByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_utxos_from_script_public_keys_by_daa_score(
            script_public_keys,
            from_daa_score,
            to_daa_score,
        )
    }

    pub fn get_balance_by_script_public_key(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey> {
        self.utxos_by_script_public_key_store.get_balance_from_script_public_keys(script_public_keys)
    }

    // This can have a big memory footprint, so it should be used only for tests.
    pub fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        self.utxos_by_script_public_key_store.get_all_outpoints()
    }

    pub fn update_utxo_state(
        &mut self,
        to_add: &UtxoSetByScriptPublicKey,
        to_remove: &UtxoSetByScriptPublicKey,
        try_reset_on_err: bool,
    ) -> StoreResult<()> {
        // A UTXO entry can appear both in removed and in added (if the DAA score of the entry changed). Thus
        // we must first apply removals and then additions (so it will be re-added in the addition phase)
        let mut res = self.utxos_by_script_public_key_store.remove_utxo_entries(to_remove);

        if res.is_err() {
            if try_reset_on_err {
                self.delete_all()?;
            }
            return res;
        }

        // Now apply additions
        res = self.utxos_by_script_public_key_store.add_utxo_entries(to_add);

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
        self.db_version_store.remove()?;
        self.utxos_by_script_public_key_store.delete_all()?;

        trace!("[{0}] clearing utxoindex database - success!", IDENT);

        Ok(())
    }
}
