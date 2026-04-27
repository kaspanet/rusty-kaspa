use crate::core::model::UtxoSetByScriptPublicKey;
use crate::stores::indexed_utxos::TRANSACTION_OUTPOINT_KEY_SIZE;

use super::indexed_utxos::{ScriptPublicKeyBucket, TransactionOutpointKey, UtxoEntryFullAccessKey};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DbWriter, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use std::sync::Arc;

#[derive(Eq, Hash, PartialEq, Debug, Clone)]
/// covenant_id (32) | spk (variable length) | transaction_outpoint
struct CovenantUtxoMarkerAccessKey(Arc<Vec<u8>>);

impl CovenantUtxoMarkerAccessKey {
    fn new(
        covenant_id: Hash,
        script_public_key_bucket: ScriptPublicKeyBucket,
        transaction_outpoint_key: TransactionOutpointKey,
    ) -> Self {
        let mut bytes =
            Vec::with_capacity(kaspa_hashes::HASH_SIZE + script_public_key_bucket.as_ref().len() + TRANSACTION_OUTPOINT_KEY_SIZE);
        bytes.extend_from_slice(&covenant_id.as_bytes());
        bytes.extend_from_slice(script_public_key_bucket.as_ref());
        bytes.extend_from_slice(transaction_outpoint_key.as_ref());
        Self(Arc::new(bytes))
    }

    fn query_bucket(covenant_id: Hash, script_public_key: Option<&ScriptPublicKey>) -> Vec<u8> {
        let script_public_key_bucket = script_public_key.map(ScriptPublicKeyBucket::from);
        let mut bytes =
            Vec::with_capacity(kaspa_hashes::HASH_SIZE + script_public_key_bucket.as_ref().map_or(0, |bucket| bucket.as_ref().len()));
        bytes.extend_from_slice(&covenant_id.as_bytes());
        if let Some(script_public_key_bucket) = script_public_key_bucket {
            bytes.extend_from_slice(script_public_key_bucket.as_ref());
        }
        bytes
    }
}

impl AsRef<[u8]> for CovenantUtxoMarkerAccessKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

pub trait UtxoSetByCovenantIdStoreReader {
    fn get_utxo_access_keys_from_covenant_id(
        &self,
        covenant_id: Hash,
        script_public_key: Option<ScriptPublicKey>,
    ) -> StoreResult<Vec<UtxoEntryFullAccessKey>>;
}

pub trait UtxoSetByCovenantIdStore: UtxoSetByCovenantIdStoreReader {
    fn remove_utxo_entries(&self, writer: impl DbWriter, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;
    fn add_utxo_entries(&self, writer: impl DbWriter, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

#[derive(Clone)]
/// Secondary UTXO index by covenant id.
/// Value is no op, only key is used to then fetch from primary index.
pub struct DbUtxoSetByCovenantIdStore {
    db: Arc<DB>,
    access: CachedDbAccess<CovenantUtxoMarkerAccessKey, u8>,
}

impl DbUtxoSetByCovenantIdStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndexByCovenant.into()) }
    }
}

impl UtxoSetByCovenantIdStoreReader for DbUtxoSetByCovenantIdStore {
    fn get_utxo_access_keys_from_covenant_id(
        &self,
        covenant_id: Hash,
        script_public_key: Option<ScriptPublicKey>,
    ) -> StoreResult<Vec<UtxoEntryFullAccessKey>> {
        let query_bucket = CovenantUtxoMarkerAccessKey::query_bucket(covenant_id, script_public_key.as_ref());
        let query_script_public_key_bucket = script_public_key.as_ref().map(ScriptPublicKeyBucket::from);

        self.access
            .seek_iterator(Some(query_bucket.as_slice()), None, usize::MAX, false)
            .map(|res| {
                res.map_err(|err| StoreError::DataInconsistency(err.to_string())).map(|(key, _)| {
                    let full_access_key = if let Some(script_public_key_bucket) = query_script_public_key_bucket.as_ref() {
                        let mut full_access_key = Vec::with_capacity(script_public_key_bucket.as_ref().len() + key.len());
                        full_access_key.extend_from_slice(script_public_key_bucket.as_ref());
                        full_access_key.extend_from_slice(&key);
                        full_access_key
                    } else {
                        key.to_vec()
                    };
                    UtxoEntryFullAccessKey::from(full_access_key)
                })
            })
            .collect()
    }
}

impl UtxoSetByCovenantIdStore for DbUtxoSetByCovenantIdStore {
    fn remove_utxo_entries(&self, mut writer: impl DbWriter, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        let mut to_remove = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.iter().filter_map(move |(transaction_outpoint, compact_utxo)| {
                compact_utxo.covenant_id.map(|covenant_id| {
                    CovenantUtxoMarkerAccessKey::new(
                        covenant_id,
                        ScriptPublicKeyBucket::from(script_public_key),
                        TransactionOutpointKey::from(transaction_outpoint),
                    )
                })
            })
        });

        self.access.delete_many(&mut writer, &mut to_remove)?;
        Ok(())
    }

    fn add_utxo_entries(&self, mut writer: impl DbWriter, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        let mut to_add = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.iter().filter_map(move |(transaction_outpoint, compact_utxo)| {
                compact_utxo.covenant_id.map(|covenant_id| {
                    (
                        CovenantUtxoMarkerAccessKey::new(
                            covenant_id,
                            ScriptPublicKeyBucket::from(script_public_key),
                            TransactionOutpointKey::from(transaction_outpoint),
                        ),
                        1u8,
                    )
                })
            })
        });

        self.access.write_many(&mut writer, &mut to_add)?;
        Ok(())
    }

    fn delete_all(&mut self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}
