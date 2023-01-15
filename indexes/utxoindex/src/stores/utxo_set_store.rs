use crate::model::{UtxoSetByScriptPublicKey, UtxoSetDiffByScriptPublicKey, CompactUtxoEntry, CompactUtxoCollection};

use std::mem::size_of;
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
use std::sync::Arc;

pub const UTXO_SET_BY_SCRIPT_PUBLIC_KEY_STORE_PREFIX: &[u8] = b"utxo-set-by-script-public-key";
pub const SCRIPT_PUBLIC_KEY_BUCKET_SIZE: usize = size_of::<VersionType>() +  size_of::<ScriptVec>();
pub const TRANSACTION_OUTPOINT_KEY_SIZE: usize = hashes::HASH_SIZE + size_of::<TransactionIndexType>();
pub const UTXO_ENTRY_KEY_SIZE:usize =  SCRIPT_PUBLIC_KEY_BUCKET_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE + SEP_SIZE;

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct ScriptPublicKeyBucket([u8; SCRIPT_PUBLIC_KEY_BUCKET_SIZE]);
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct TransactionOutpointKey([u8; TRANSACTION_OUTPOINT_KEY_SIZE]);

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct  UtxoEntryAccessKey([u8; UTXO_ENTRY_KEY_SIZE]);

impl UtxoEntryAccessKey{
    pub fn new(script_public_key: ScriptPublicKey, transaction_outpoint: TransactionOutpoint)-> Self {
        let mut bytes = [0; UTXO_ENTRY_KEY_SIZE];
        let script_public_key_bucket: ScriptPublicKeyBucket = script_public_key.into();
        let transaction_outpoint_bucket: TransactionOutpointKey = transaction_outpoint.into();
        bytes[..SCRIPT_PUBLIC_KEY_BUCKET_SIZE].copy_from_slice(script_public_key_bucket.as_ref());
        bytes[SCRIPT_PUBLIC_KEY_BUCKET_SIZE] = SEP;
        bytes[SCRIPT_PUBLIC_KEY_BUCKET_SIZE + SEP_SIZE..].copy_from_slice(transaction_outpoint_bucket.as_ref());
        Self(bytes)
    }
}

impl AsRef<[u8]> for UtxoEntryAccessKey{
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}


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

impl From<ScriptPublicKey> for ScriptPublicKeyBucket {
    fn from(script_public_key: ScriptPublicKey) -> Self {
        let mut bytes = [0; SCRIPT_PUBLIC_KEY_BUCKET_SIZE];
        bytes[..size_of::<VersionType>()].copy_from_slice(&script_public_key.version().to_le_bytes());
        bytes[size_of::<VersionType>()..].copy_from_slice(script_public_key.script());
        
        Self(bytes)
    }
}

impl From<ScriptPublicKeyBucket> for ScriptPublicKey {
    fn from(bucket: ScriptPublicKeyBucket) -> Self {
        let version = VersionType::from_le_bytes(
            <[u8; std::mem::size_of::<VersionType>()]>::try_from(&bucket.0[..size_of::<VersionType>()]).expect("expected version size")
        );
        let script = ScriptVec::from_slice(&bucket.0[size_of::<VersionType>()..]);
        Self::new(version, script)
    }
}

impl From<TransactionOutpointKey> for TransactionOutpoint {
    fn from(key: TransactionOutpointKey) -> Self {
        let transaction_id = Hash::from_slice(&key.0[..hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; std::mem::size_of::<TransactionIndexType>()]>::try_from(&key.0[hashes::HASH_SIZE..]).expect("expected index size")
        );
        Self::new(transaction_id, index)
    }
}

impl From<TransactionOutpoint> for TransactionOutpointKey {
    fn from(outpoint: TransactionOutpoint) -> Self {
        let mut bytes = [0; TRANSACTION_OUTPOINT_KEY_SIZE];
        bytes[..hashes::HASH_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[hashes::HASH_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}




pub trait UtxoSetByScriptPublicKeyStoreReader {
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> Result<Arc<UtxoSetByScriptPublicKey>, StoreError>;
}

pub trait UtxoSetByScriptPublicKeyStore:  UtxoSetByScriptPublicKeyStoreReader{
    /// Updates the store according to the UTXO diff -- adding and deleting entries correspondingly.
    /// Note we define `self` as `mut` in order to require write access even though the compiler does not require it.
    /// This is because concurrent readers can interfere with cache consistency.  
    fn write_diff(&mut self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbUtxoSetByScriptPublicKeyStore {
    db: Arc<DB>,
    prefix: &'static [u8],
    access: CachedDbAccess<UtxoEntryAccessKey, CompactUtxoEntry>,
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

    pub fn write_diff_batch(&mut self, batch: &mut WriteBatch, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError> {
        
        let mut writer = BatchDbWriter::new(batch);
        
        let remove_iter_keys = utxo_diff_by_script_public_key.removed.iter().map(
            move |(script_public_key, compact_utxo_collection)| {
                let transaction_outpoint = compact_utxo_collection.keys().next().expect("expected tx outpoint");
                UtxoEntryAccessKey::new(*script_public_key, *transaction_outpoint)
            }
        );
        
        let added_iter_items = utxo_diff_by_script_public_key.added.iter().map(
            move |(script_public_key, compact_utxo_collection)| {
            let (transaction_outpoint, compact_utxo) = compact_utxo_collection.iter().next().expect("expected tx outpoint / utxo entry");
            (
                UtxoEntryAccessKey::new(*script_public_key, *transaction_outpoint),
                compact_utxo.clone()
            )
            }
        );

        self.access.delete_many(writer, &mut remove_iter_keys)?;
        self.access.write_many(writer, &mut added_iter_items)?;

        Ok(())
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> Result<Arc<UtxoSetByScriptPublicKey>, StoreError> //TODO: chunking
    {
        let mut utxos_by_script_public_keys = Arc::new(UtxoSetByScriptPublicKey::new());
        for script_public_key in script_public_keys {
            let mut utxos_by_script_public_keys_inner = CompactUtxoCollection::new();
            let script_public_key_bucket: ScriptPublicKeyBucket = script_public_key.into();
            utxos_by_script_public_keys_inner.extend(
                self.access.iter_prefix::<TransactionOutpoint, CompactUtxoEntry>(DbKey::new(self.prefix, script_public_key_bucket)).into_iter().map(
                    move |value| {
                        let (k, V) = value.expect("expected key: TransactionOutpoint, value: CompactUtxoEntry pair");
                        (k, V)
                    }
                ),
            );
            utxos_by_script_public_keys.insert(script_public_key, utxos_by_script_public_keys_inner);
        };
        Ok(utxos_by_script_public_keys)
    }
}

impl UtxoSetByScriptPublicKeyStore for DbUtxoSetByScriptPublicKeyStore {
    fn write_diff(&mut self, utxo_diff_by_script_public_key: UtxoSetDiffByScriptPublicKey) -> Result<(), StoreError> {
        
        let mut writer = DirectDbWriter::new(&self.db);
        
        let remove_iter_keys = utxo_diff_by_script_public_key.removed.iter().map(
            move |(script_public_key, compact_utxo_collection)| {
                let transaction_outpoint = compact_utxo_collection.keys().next().expect("expected tx outpoint");
                UtxoEntryAccessKey::new(*script_public_key, *transaction_outpoint)
            }
        );
        
        let added_iter_items = utxo_diff_by_script_public_key.added.iter().map(
            move |(script_public_key, compact_utxo_collection)| {
            let (transaction_outpoint, compact_utxo) = compact_utxo_collection.iter().next().expect("expected tx outpoint / utxo entry");
            (
                UtxoEntryAccessKey::new(*script_public_key, *transaction_outpoint),
                compact_utxo.clone()
            )
            }
        );

        self.access.delete_many(writer, &mut remove_iter_keys)?;
        self.access.write_many(writer, &mut added_iter_items)?;

        Ok(())
    }
}
