use crate::model::{UtxoSetByScriptPublicKey, UtxoSetDiffByScriptPublicKey, CompactUtxoCollection, CompactUtxoEntry};

use std::{mem::{size_of}};
use consensus::model::stores::{
    database::prelude::{DbKey, BatchDbWriter, CachedDbAccess, DirectDbWriter, SEP_SIZE, SEP},
    errors::{StoreError},
    DB,
};
use consensus_core::{
    tx::{TransactionIndexType, TransactionOutpoint, ScriptPublicKey, ScriptPublicKeys, ScriptVec, VersionType},
};
use hashes::Hash;
use rocksdb::WriteBatch;
use std::{fmt::Display, sync::Arc};

pub const UTXO_SET_BY_SCRIPT_PUBLIC_KEY_STORE_PREFIX: &[u8] = b"utxo-set-by-script-public-key";
pub const SCRIPT_PUBLIC_KEY_BUCKET_SIZE: usize = size_of::<VersionType>() +  size_of::<ScriptVec>();
pub const TRANSACTION_OUTPOINT_KEY_SIZE: usize = hashes::HASH_SIZE + size_of::<TransactionIndexType>();
pub const UTXO_ENTRY_KEY_SIZE:usize =  SCRIPT_PUBLIC_KEY_BUCKET_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE + SEP_SIZE;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct ScriptPublicKeyBucket([u8; SCRIPT_PUBLIC_KEY_BUCKET_SIZE]);
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct TransactionOutpointKey([u8; TRANSACTION_OUTPOINT_KEY_SIZE]);
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct UtxoEntryKey([u8; UTXO_ENTRY_KEY_SIZE]);


impl AsRef<[u8]> for ScriptPublicKeyBucket {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for TransactionOutpointKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for UtxoEntryKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

fn utxo_entry_key_from_script_public_key_and_transaction_outpoint(
    script_public_key: ScriptPublicKey, 
    transaction_outpoint: TransactionOutpoint
    ) -> UtxoEntryKey {
        
        let script_public_key_bucket = TransactionOutpointKey::from(transaction_outpoint);
        ler
        UtxoEntryKey::try_from([
            [ScriptPublicKeyBucket::from(script_public_key).as_ref(), TransactionOutpointKey::from(transaction_outpoint).as_ref()]
        ].concat()
    }

impl From<ScriptPublicKeyBucket> for ScriptPublicKey {
    fn from(k: ScriptPublicKeyBucket) -> Self {
        let script = ScriptVec::from_slice(&k.0[size_of::<VersionType>()..]);
        let version = VersionType::from_le_bytes(
            <[u8; size_of::<VersionType>()]>::try_from(&k.0[..size_of::<VersionType>()]).expect("expecting version size"),
        );
        Self::new(version, script)
    }
}

impl From<TransactionOutpointKey> for TransactionOutpoint {
    fn from(k: TransactionOutpointKey) -> Self {
        let transaction_id = Hash::from_slice(&k.0[..hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; size_of::<TransactionIndexType>()]>::try_from(&k.0[hashes::HASH_SIZE..]).expect("expecting index size"),
        );
        Self::new(transaction_id, index)
    }
}
pub trait UtxoSetByScriptPublicKeyStoreReader {
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> Result<Arc<Ut>, StoreError>;
}

pub trait UtxoSetByScriptPublicKeyStore:  UtxoSetByScriptPublicKeyStoreReader{
    /// Updates the store according to the UTXO diff -- adding and deleting entries correspondingly.
    /// Note we define `self` as `mut` in order to require write access even though the compiler does not require it.
    /// This is because concurrent readers can interfere with cache consistency.  
    fn write_diff(&mut self, utxo_diff_by_script_public_key: &UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError>;
}



#[derive(Clone)]
pub struct DbUtxoSetByScriptPublicKeyStore {
    db: Arc<DB>,
    prefix: &'static [u8],
    access: CachedDbAccess<UtxoEntryKey, CompactUtxoEntry>,
}

impl DbUtxoSetByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { 
            db: Arc::clone(&db), 
            access: CachedDbAccess::new(db, cache_size, UTXO_SET_BY_SCRIPT_PUBLIC_KEY_STORE_PREFIX),
            prefix: UTXO_SET_BY_SCRIPT_PUBLIC_KEY_STORE_PREFIX,
         }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size,)
    }

    pub fn write_diff_batch(&mut self, batch: &mut WriteBatch, utxo_diff: UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        remove_iter_keys = utxo_diff.removed.iter().map(
            move |(k, v)| 
            ([*k as ScriptPublicKeyKey, v.keys().next().expect("expected tx outpoint") as UtxoKey].concat())
        );
        added_iter_items = utxo_diff.added.iter().map(
            move |(k, v)| {
            let (outpoint, utxo) = v.iter().next().expect("expected tx outpoint / utxo entry");
            ([*k as ScriptPublicKeyKey, outpoint as UtxoKey].concat(), utxo)
            }
        );
        self.access.delete_many(writer, &mut remove_iter_keys)?;
        self.access.write_many(writer, &mut added_iter_items)?;
        Ok(())
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> Result<Arc<UtxoByScriptPublicKey>, StoreError> //TODO: chunking
    {
        let mut utxos_by_script_public_keys =Arc::new(UtxoSetByScriptPublicKey::new());
        for script_public_key in script_public_keys{
            let mut utxos_by_script_public_keys_inner = CompactUtxoCollection::new();
            utxos_by_script_public_keys_inner.extend(
                self.access.iter_prefix::<TransactionOutpoint, CompactUtxoEntry>(DbKey::new(self.prefix, ScriptPublicKey as ScriptPublicKeyBucket)).into_iter().map(
                    move |value| -> (TransactionOutpoint, CompactUtxoEntry) {
                        let (k, v) = value.expect("expected key value pair");
                        (k, v)
                    }
                ),
            );
            utxos_by_script_public_keys.insert(script_public_key, utxos_by_script_public_keys_inner);
        };
        Ok(utxos_by_script_public_keys)
    }
}

impl UtxoSetByScriptPublicKeyStore for DbUtxoSetByScriptPublicKeyStore {
    fn write_diff(&mut self, utxo_diff_by_script_public_key: &UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        let remove_iter_keys = utxo_diff_by_script_public_key.added.iter().map(
            move |(k, v)| 
            utxo_entry_key_from_script_public_key_and_transaction_outpoint(k, v.keys().next())
        );
        let added_iter_items = utxo_diff_by_script_public_key.removed.iter().map(
            move |(k, v)| {
            (
                utxo_entry_key_from_script_public_key_and_transaction_outpoint(k,v.keys().next()), 
                v.values().next()
            )
        });
        self.access.delete_many(writer, &mut remove_iter_keys)?;
        self.access.write_many(writer, &mut added_iter_items)?;
        Ok(())
    }
}
