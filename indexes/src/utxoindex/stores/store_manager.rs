use std::sync::Arc;

use crate::utxoindex::ToDo;

use super::utxos_by_script_public_key::DbUtxosByScriptPublicKeyStore;
use consensus::model::stores::DB;

pub struct UtxoIndexStoreManager {
        db: Arc<DB>,
        pub utxos_by_script_public_key_store: Arc<DbUtxosByScriptPublicKeyStore>,
        pub tips_store: ToDo,
        pub circulating_supply_store: ToDo,
}

impl UtxoIndexStoreManager {
    pub fn new() -> Self {
        Self {
            db = Arc::new(DB::new()),
            utxos_by_script_public_key_store: DbUtxosByScriptPublicKeyStore::new(db0, 0),
        }
    }
}