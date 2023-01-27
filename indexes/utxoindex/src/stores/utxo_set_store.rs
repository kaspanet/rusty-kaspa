use crate::model::{CompactUtxoCollection, CompactUtxoEntry, UTXOChanges, UtxoSetByScriptPublicKey};

use consensus::model::stores::{
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter, SEP, SEP_SIZE},
    errors::StoreError,
    DB,
};
use consensus_core::tx::{
    ScriptPublicKey, ScriptPublicKeys, ScriptVec, TransactionIndexType, TransactionOutpoint, VersionType, SCRIPT_VECTOR_SIZE,
};
use hashes::Hash;
use rocksdb::WriteBatch;
use std::mem::size_of;
use std::sync::Arc;

// ## Prefixes:

///prefixes the [ScriptPublicKey] indexed utxo set.
pub const UTXO_SET_PREFIX: &[u8] = b"utxoindex:utxo-set";
///prefix for the last sync'd [VirtualParents] (i.e. blockdag tips)
pub const VIRTUAL_PARENTS_PREFIX: &[u8] = b"utxoindex:virtual-parents";
///Prefixes the [CirculatingSupply]
pub const CIRCULATING_SUPPLY_PREFIX: &[u8] = b"utxoindex:circulating-supply";

// ## Buckets:

///Size of the [ScriptPublicKeyBucket] in bytes.
pub const SCRIPT_PUBLIC_KEY_BUCKET_SIZE: usize = size_of::<VersionType>() + SCRIPT_VECTOR_SIZE;

///[ScriptPublicKey] bucket.
///Consists of 2 bytes of little endian [VersionType] bytes, followed by 36 bytes of [ScriptVec].
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct ScriptPublicKeyBucket([u8; SCRIPT_PUBLIC_KEY_BUCKET_SIZE]);

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
            <[u8; std::mem::size_of::<VersionType>()]>::try_from(&bucket.0[..size_of::<VersionType>()])
                .expect("expected version size"),
        );
        let script = ScriptVec::from_slice(&bucket.0[size_of::<VersionType>()..]);
        Self::new(version, script)
    }
}

impl AsRef<[u8]> for ScriptPublicKeyBucket {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

// ## Keys:

// ### TransactionOutpoint:
///Size of the [TransactionOutpointKey] in bytes.
pub const TRANSACTION_OUTPOINT_KEY_SIZE: usize = hashes::HASH_SIZE + size_of::<TransactionIndexType>();

///[TransactionOutpoint] key which references the [CompactUtxoEntry] within a [ScriptPublicKeyBucket]
///Consists of 32 bytes of [TransactionId], followed by 4 bytes of little endian [TransactionIndexType]
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct TransactionOutpointKey([u8; TRANSACTION_OUTPOINT_KEY_SIZE]);

impl From<TransactionOutpointKey> for TransactionOutpoint {
    fn from(key: TransactionOutpointKey) -> Self {
        let transaction_id = Hash::from_slice(&key.0[..hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; std::mem::size_of::<TransactionIndexType>()]>::try_from(&key.0[hashes::HASH_SIZE..]).expect("expected index size"),
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

impl AsRef<[u8]> for TransactionOutpointKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

///Size of the [UtxoEntryFullAccessKey] in bytes.
pub const UTXO_ENTRY_FULL_ACCESS_KEY_SIZE: usize = SCRIPT_PUBLIC_KEY_BUCKET_SIZE + SEP_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE;
///Full [CompactUtxoEntry] access key.
///Consists of  38 bytes of [ScriptPublicKeyBucket], one byte of [SEP], and 36 bytes of [TransactionOutpointKey]
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct UtxoEntryFullAccessKey([u8; UTXO_ENTRY_FULL_ACCESS_KEY_SIZE]);

impl UtxoEntryFullAccessKey {
    ///creates a new [UtxoEntryFullAccessKey] from a [ScriptPublicKeyBucket] and [TransactionOutpointKey].
    pub fn new(script_public_key_bucket: ScriptPublicKeyBucket, transaction_outpoint_key: TransactionOutpointKey) -> Self {
        let mut bytes = [0; UTXO_ENTRY_FULL_ACCESS_KEY_SIZE];
        bytes[..SCRIPT_PUBLIC_KEY_BUCKET_SIZE].copy_from_slice(script_public_key_bucket.as_ref());
        bytes[SCRIPT_PUBLIC_KEY_BUCKET_SIZE] = SEP;
        bytes[SCRIPT_PUBLIC_KEY_BUCKET_SIZE + SEP_SIZE..].copy_from_slice(transaction_outpoint_key.as_ref());
        Self(bytes)
    }
}

impl AsRef<[u8]> for UtxoEntryFullAccessKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub trait UtxoSetByScriptPublicKeyStoreReader {
    ///Get [UtxoSetByScriptPublicKey] set by queried [ScriptPublicKeys],
    fn get_utxos_from_script_public_keys(
        &self,
        script_public_keys: ScriptPublicKeys,
    ) -> Result<Arc<UtxoSetByScriptPublicKey>, StoreError>;
}

pub trait UtxoSetByScriptPublicKeyStore: UtxoSetByScriptPublicKeyStoreReader {
    /// Updates the store according to the UTXO diff via script public key changes-- adding and deleting entries correspondingly.
    /// Note we define `self` as `mut` in order to require write access even though the compiler does not require it.
    /// This is because concurrent readers can interfere with cache consistency.  
    fn write_diff(&mut self, utxo_diff_by_script_public_key: UTXOChanges) -> Result<(), StoreError>;

    /// Insert a [UtxoSetByScriptPublicKey] into the [UtxoSetByScriptPublicKeyStore].
    fn insert_utxo_entries(&mut self, utxo_entries: UtxoSetByScriptPublicKey) -> Result<(), StoreError>;

    fn delete_all(&mut self) -> Result<(), StoreError>;
}

#[derive(Clone)]
pub struct DbUtxoSetByScriptPublicKeyStore {
    db: Arc<DB>,
    prefix: &'static [u8],
    access: CachedDbAccess<UtxoEntryFullAccessKey, CompactUtxoEntry>,
}

impl DbUtxoSetByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, UTXO_SET_PREFIX), prefix: UTXO_SET_PREFIX }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn write_diff_batch(&mut self, batch: &mut WriteBatch, utxo_diff_by_script_public_key: UTXOChanges) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);

        let mut remove_iter_keys =
            utxo_diff_by_script_public_key.removed.iter().map(move |(script_public_key, compact_utxo_collection)| {
                let transaction_outpoint = compact_utxo_collection.keys().next().expect("expected tx outpoint");
                UtxoEntryFullAccessKey::new(
                    ScriptPublicKeyBucket::from(script_public_key.clone()),
                    TransactionOutpointKey::from(*transaction_outpoint),
                )
            });

        let mut added_iter_items =
            utxo_diff_by_script_public_key.added.iter().map(move |(script_public_key, compact_utxo_collection)| {
                let (transaction_outpoint, compact_utxo) =
                    compact_utxo_collection.iter().next().expect("expected tx outpoint / utxo entry");
                (
                    UtxoEntryFullAccessKey::new(
                        ScriptPublicKeyBucket::from(script_public_key.clone()), //TODO: change ScriptVec to own struct to implement copy.
                        TransactionOutpointKey::from(*transaction_outpoint),
                    ),
                    *compact_utxo,
                )
            });

        self.access.delete_many(&mut writer, &mut remove_iter_keys)?;
        self.access.write_many(&mut writer, &mut added_iter_items)?;

        Ok(())
    }

    fn delete_all(&mut self, batch: &mut WriteBatch) -> Result<(), StoreError> {
        let mut writer = BatchDbWriter::new(batch);
        self.access.delete_all(&mut writer)
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    fn get_utxos_from_script_public_keys(
        &self,
        script_public_keys: ScriptPublicKeys,
    ) -> Result<Arc<UtxoSetByScriptPublicKey>, StoreError> //TODO: chunking
    {
        let mut utxos_by_script_public_keys = UtxoSetByScriptPublicKey::new();
        for script_public_key in script_public_keys {
            let mut utxos_by_script_public_keys_inner = CompactUtxoCollection::new();
            let script_public_key_bucket: ScriptPublicKeyBucket = script_public_key.clone().into();
            utxos_by_script_public_keys_inner.extend(
                self.access
                    .seek_iterator::<TransactionOutpoint, CompactUtxoEntry>(Some(vec![script_public_key_bucket.as_ref()]), None)
                    .into_iter()
                    .map(move |value| {
                        let (k, v) = value.expect("expected key: TransactionOutpoint, value: CompactUtxoEntry pair");
                        (k, v)
                    }),
            );
            utxos_by_script_public_keys.insert(script_public_key, utxos_by_script_public_keys_inner);
        }
        Ok(Arc::new(utxos_by_script_public_keys))
    }
}

impl UtxoSetByScriptPublicKeyStore for DbUtxoSetByScriptPublicKeyStore {
    fn write_diff(&mut self, utxo_diff_by_script_public_key: UTXOChanges) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);

        let mut remove_iter_keys =
            utxo_diff_by_script_public_key.removed.iter().map(move |(script_public_key, compact_utxo_collection)| {
                let transaction_outpoint = compact_utxo_collection.keys().next().expect("expected transaction outpoint");
                UtxoEntryFullAccessKey::new(
                    ScriptPublicKeyBucket::from(script_public_key.clone()),
                    TransactionOutpointKey::from(*transaction_outpoint),
                )
            });

        let mut added_iter_items =
            utxo_diff_by_script_public_key.added.iter().map(move |(script_public_key, compact_utxo_collection)| {
                let (transaction_outpoint, compact_utxo) = compact_utxo_collection.iter().next().expect("expected utxo entry");
                (
                    UtxoEntryFullAccessKey::new(
                        ScriptPublicKeyBucket::from(script_public_key.clone()),
                        TransactionOutpointKey::from(*transaction_outpoint),
                    ),
                    *compact_utxo,
                )
            });

        self.access.delete_many(&mut writer, &mut remove_iter_keys)?;
        self.access.write_many(&mut writer, &mut added_iter_items)?;

        Ok(())
    }

    fn insert_utxo_entries(&mut self, utxo_entries: UtxoSetByScriptPublicKey) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);

        let mut utxo_entry_iterator = utxo_entries.iter().map(move |(script_public_key, compact_utxo_collection)| {
            let (transaction_outpoint, compact_utxo) = compact_utxo_collection.iter().next().expect("expected utxo entry");
            (
                UtxoEntryFullAccessKey::new(
                    ScriptPublicKeyBucket::from(script_public_key.clone()),
                    TransactionOutpointKey::from(*transaction_outpoint),
                ),
                *compact_utxo,
            )
        });

        self.access.write_many(&mut writer, &mut utxo_entry_iterator)?;

        Ok(())
    }

    fn delete_all(&mut self) -> Result<(), StoreError> {
        let mut writer = DirectDbWriter::new(&self.db);
        self.access.delete_all(&mut writer)
    }
}
