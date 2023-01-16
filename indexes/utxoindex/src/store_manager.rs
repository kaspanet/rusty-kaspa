use std::{fs, sync::Arc};

use consensus::{
    consensus::VirtualStores,
    model::stores::{errors::StoreError, DB},
};
use consensus_core::{tx::ScriptPublicKeys, BlockHashSet};
use parking_lot::RwLock;

use crate::{
    model::{UtxoSetByScriptPublicKey, UtxoSetDiffByScriptPublicKey},
    stores::{
        circulating_supply_store::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips_store::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
        utxo_set_store::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
    },
};

pub struct StoreManager {
    db: Arc<DB>,

    pub utxoindex_tips_store: Arc<RwLock<DbUtxoIndexTipsStore>>,
    pub circulating_suppy_store: Arc<RwLock<DbCirculatingSupplyStore>>,
    pub utxos_by_script_public_key_store: Arc<RwLock<DbUtxoSetByScriptPublicKeyStore>>,
}

impl StoreManager {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            db,

            utxoindex_tips_store: Arc::new(RwLock::new(DbUtxoIndexTipsStore::new(db))),
            circulating_suppy_store: Arc::new(RwLock::new(DbCirculatingSupplyStore::new(db))),
            utxos_by_script_public_key_store: Arc::new(RwLock::new(DbUtxoSetByScriptPublicKeyStore::new(db, 0))),
        }
    }

    pub fn get_utxos_by_script_public_key(
        &self,
        script_public_keys: ScriptPublicKeys,
    ) -> Result<Arc<UtxoSetByScriptPublicKey>, StoreError> {
        let reader = self.utxos_by_script_public_key_store.read();
        reader.get_utxos_from_script_public_keys(script_public_keys)
    }

    pub async fn update_utxo_state(&self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError> {
        let writer = self.utxos_by_script_public_key_store.write().await;
        writer.write_diff(utxo_diff_by_script_public_key)
    }

    pub fn get_circulating_supply(&self) -> StoreResult<u64> {
        let reader = self.circulating_suppy_store.read();
        reader.get()
    }

    pub async fn update_circulating_supply(&self, circulating_supply_diff: i64) -> Result<u64, StoreError> {
        let writer = self.circulating_suppy_store.write().await;
        writer.add_circulating_supply_diff(utxo_diff_by_script_public_key)
    }

    pub fn get_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        let reader = self.utxoindex_tips_store.read();
        reader.get()?
    }

    pub async fn insert_tips(&self, tips: BlockHashSet) -> Result<(), StoreError> {
        let writer = self.utxoindex_tips_store.write().await;
        writer.add_tips(tips)
    }

    pub fn delete_all(&mut self) {
        //hold all individual locks in-place
        _ = self.circulating_suppy_store.write();
        _ = self.utxos_by_script_public_key_store.write();
        _ = self.utxoindex_tips_store.write();
        let old_db = self.db; //we know database is thread-safe because we hold all individual access locks.
        let db_path = old_db.path(); //extract the path
        fs::remove_dir_all(path); //remove directory
        fs::create_dir_all(path); //recreate directory
        let new_db = DB::open_default(utxoindex_store.to_str().unwrap()).unwrap(); //create new db
        let old_db = new_db; //swap out databases
    }
}
