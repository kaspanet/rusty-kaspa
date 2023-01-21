use std::{fs, sync::Arc};

use consensus::{
    consensus::VirtualStores,
    model::stores::{errors::StoreError, DB},
};
use consensus_core::{
    tx::{ScriptPublicKeys, TransactionOutpoint, UtxoEntry},
    BlockHashSet,
};
use parking_lot::RwLock;

use crate::{
    model::{UTXOChanges, UtxoSetByScriptPublicKey},
    stores::{
        circulating_supply_store::{CirculatingSupplyStore, CirculatingSupplyStoreReader, DbCirculatingSupplyStore},
        tips_store::{DbUtxoIndexTipsStore, UtxoIndexTipsStore, UtxoIndexTipsStoreReader},
        utxo_set_store::{DbUtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStore, UtxoSetByScriptPublicKeyStoreReader},
    },
};

pub struct StoreManager {
    db: Arc<RwLock<DB>>,

    utxoindex_tips_store: Arc<RwLock<DbUtxoIndexTipsStore>>,
    circulating_suppy_store: Arc<RwLock<DbCirculatingSupplyStore>>,
    utxos_by_script_public_key_store: Arc<RwLock<DbUtxoSetByScriptPublicKeyStore>>,
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

    pub async fn update_utxo_state(&self, utxo_diff_by_script_public_key: UTXOChanges) -> Result<(), StoreError> {
        let writer = self.utxos_by_script_public_key_store.write();
        writer.write_diff(utxo_diff_by_script_public_key)
    }

    pub async fn insert_utxo_entries(&self, utxo_set_by_script_public_key: UtxoSetByScriptPublicKey) -> Result<(), StoreError> {
        let writer = self.utxos_by_script_public_key_store.write();
        writer.insert_utxo_entries(utxo_set_by_script_public_key)
    }

    pub fn get_circulating_supply(&self) -> StoreResult<u64> {
        let reader = self.circulating_suppy_store.read();
        reader.get()
    }

    pub async fn update_circulating_supply(&self, circulating_supply_diff: i64) -> Result<u64, StoreError> {
        let writer = self.circulating_suppy_store.write();
        writer.add_circulating_supply_diff(utxo_diff_by_script_public_key)
    }

    pub async fn insert_circulating_supply(&self, circulating_supply: u64) -> Result<u64, StoreError> {
        let writer = self.circulating_suppy_store.write();
        writer.insert(circulating_supply)
    }

    pub fn get_tips(&self) -> StoreResult<Arc<BlockHashSet>> {
        let reader = self.utxoindex_tips_store.read();
        reader.get()?
    }

    pub async fn insert_tips(&self, tips: BlockHashSet) -> Result<(), StoreError> {
        let writer = self.utxoindex_tips_store.write();
        writer.add_tips(tips)
    }

    /// Resets the utxoindex database:
    ///
    /// 1) Removes the entire utxoindex database,
    /// 2) Creates a new one in its place,
    /// 3) populates the new db with associated prefixes.
    pub fn delete_all(&mut self) {
        trace!("creating new utxoindex database, deleting the old one");
        //hold all individual store locks in-place
        let circulating_suppy_store = self.circulating_suppy_store.write();
        let utxos_by_script_public_key_store = self.utxos_by_script_public_key_store.write();
        let utxoindex_tips_store = self.utxoindex_tips_store.write();

        //remove old database path, and recreate a new one
        let old_db = self.db.write(); //although RwLocks are not part of the db Arc of the individual stores, we know it is thread-safe because we hold the individual write guards of each individual store.
        let db_path = old_db.path(); //extract the path
        fs::remove_dir_all(path); //remove directory
        fs::create_dir_all(path); //recreate directory

        //create new database and swap
        let mut new_db = DB::open_default(utxoindex_store.to_str().unwrap()).unwrap(); //create new db
        let mut old_db = new_db; //swap out databases

        //recreate individual stores (i.e. create a new access with the given store prefixes)
        let circulating_suppy_store = circulating_suppy_store.clone_with_new_cache();
        let utxos_by_script_public_key_store = utxos_by_script_public_key_store.clone_with_new_cache(0);
        let utxoindex_tips_store = utxoindex_tips_store.clone_with_new_cache();
    }
}
