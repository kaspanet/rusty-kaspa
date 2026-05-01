use crate::core::model::{CompactUtxoCollection, CompactUtxoEntry, OrderedUtxoSetByScriptPublicKey, UtxoSetByScriptPublicKey};

use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptPublicKeyVersion, ScriptPublicKeys, ScriptVec, TransactionIndexType, TransactionOutpoint,
};
use kaspa_core::debug;
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_index_core::indexed_utxos::{BalanceByScriptPublicKey, UtxoEntryKeyData};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::Arc;

pub const VERSION_TYPE_SIZE: usize = size_of::<ScriptPublicKeyVersion>(); // Const since we need to re-use this a few times.

/// [`ScriptPublicKeyBucket`].
/// Consists of 2 bytes of little endian [VersionType] bytes, followed by the script length (8) and by a variable size of [ScriptVec].
#[derive(Eq, Hash, PartialEq, Debug, Clone)]
struct ScriptPublicKeyBucket(Vec<u8>);

impl From<&ScriptPublicKey> for ScriptPublicKeyBucket {
    fn from(script_public_key: &ScriptPublicKey) -> Self {
        // version (2) + length (8) + dynamic script
        let mut bytes: Vec<u8> = Vec::with_capacity(VERSION_TYPE_SIZE + size_of::<u64>() + script_public_key.script().len());
        bytes.extend_from_slice(&script_public_key.version().to_le_bytes());
        bytes.extend_from_slice(&(script_public_key.script().len() as u64).to_le_bytes()); // TODO: Consider using a smaller integer
        bytes.extend_from_slice(script_public_key.script());
        Self(bytes)
    }
}

impl From<ScriptPublicKeyBucket> for ScriptPublicKey {
    fn from(bucket: ScriptPublicKeyBucket) -> Self {
        let version = ScriptPublicKeyVersion::from_le_bytes(
            <[u8; VERSION_TYPE_SIZE]>::try_from(&bucket.0[..VERSION_TYPE_SIZE]).expect("expected version size"),
        );

        let script_size =
            u64::from_le_bytes(bucket.0[VERSION_TYPE_SIZE..VERSION_TYPE_SIZE + size_of::<u64>()].try_into().unwrap()) as usize;
        let script =
            ScriptVec::from_slice(&bucket.0[VERSION_TYPE_SIZE + size_of::<u64>()..VERSION_TYPE_SIZE + size_of::<u64>() + script_size]);

        Self::new(version, script)
    }
}

impl AsRef<[u8]> for ScriptPublicKeyBucket {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// Keys:

// TransactionOutpoint:
/// Size of the [TransactionOutpointKey] in bytes.
pub const TRANSACTION_OUTPOINT_KEY_SIZE: usize = kaspa_hashes::HASH_SIZE + size_of::<TransactionIndexType>();

/// [TransactionOutpoint] key which references the [CompactUtxoEntry] within a [ScriptPublicKeyBucket]
/// Consists of 32 bytes of [TransactionId], followed by 4 bytes of little endian [TransactionIndexType]
#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct TransactionOutpointKey([u8; TRANSACTION_OUTPOINT_KEY_SIZE]);

impl TransactionOutpointKey {
    pub const EMPTY: Self = TransactionOutpointKey([0; TRANSACTION_OUTPOINT_KEY_SIZE]);
}

impl From<TransactionOutpointKey> for TransactionOutpoint {
    fn from(key: TransactionOutpointKey) -> Self {
        let transaction_id = Hash::from_slice(&key.0[..kaspa_hashes::HASH_SIZE]);
        let index = TransactionIndexType::from_le_bytes(
            <[u8; size_of::<TransactionIndexType>()]>::try_from(&key.0[kaspa_hashes::HASH_SIZE..]).expect("expected index size"),
        );
        Self::new(transaction_id, index)
    }
}

impl From<&TransactionOutpoint> for TransactionOutpointKey {
    fn from(outpoint: &TransactionOutpoint) -> Self {
        let mut bytes = [0; TRANSACTION_OUTPOINT_KEY_SIZE];
        bytes[..kaspa_hashes::HASH_SIZE].copy_from_slice(&outpoint.transaction_id.as_bytes());
        bytes[kaspa_hashes::HASH_SIZE..].copy_from_slice(&outpoint.index.to_le_bytes());
        Self(bytes)
    }
}

impl AsRef<[u8]> for TransactionOutpointKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

pub const DAA_SCORE_KEY_SIZE: usize = size_of::<u64>();

struct DaaScoreKey([u8; DAA_SCORE_KEY_SIZE]);

impl From<u64> for DaaScoreKey {
    fn from(daa_score: u64) -> Self {
        DaaScoreKey(daa_score.to_be_bytes())
    }
}

impl AsRef<[u8]> for DaaScoreKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Eq, Hash, PartialEq, Debug, Copy, Clone)]
struct UtxoEntryInnerKey([u8; DAA_SCORE_KEY_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE]);

impl From<(u64, TransactionOutpoint)> for UtxoEntryInnerKey {
    fn from(value: (u64, TransactionOutpoint)) -> Self {
        let mut bytes = [0; DAA_SCORE_KEY_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE];
        bytes[..DAA_SCORE_KEY_SIZE].copy_from_slice(&value.0.to_be_bytes());
        bytes[DAA_SCORE_KEY_SIZE..].copy_from_slice(&TransactionOutpointKey::from(&value.1).0);
        UtxoEntryInnerKey(bytes)
    }
}

impl From<UtxoEntryInnerKey> for UtxoEntryKeyData {
    fn from(key: UtxoEntryInnerKey) -> Self {
        let daa_score = u64::from_be_bytes(key.0[..DAA_SCORE_KEY_SIZE].try_into().unwrap());
        let transaction_outpoint = TransactionOutpoint::from(TransactionOutpointKey(key.0[DAA_SCORE_KEY_SIZE..].try_into().unwrap()));
        UtxoEntryKeyData { daa_score, transaction_outpoint }
    }
}

impl From<UtxoEntryInnerKey> for (u64, TransactionOutpoint) {
    fn from(key: UtxoEntryInnerKey) -> Self {
        let daa_score = u64::from_be_bytes(key.0[..DAA_SCORE_KEY_SIZE].try_into().unwrap());
        let transaction_outpoint = TransactionOutpoint::from(TransactionOutpointKey(key.0[DAA_SCORE_KEY_SIZE..].try_into().unwrap()));
        (daa_score, transaction_outpoint)
    }
}

impl AsRef<[u8]> for UtxoEntryInnerKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Full [CompactUtxoEntry] access key.
/// Consists of variable amount of bytes of [ScriptPublicKeyBucket], followed by [DaaScoreKey], and [TransactionOutpointKey].
#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize)]
struct UtxoEntryFullAccessKey(Arc<Vec<u8>>);

impl Display for UtxoEntryFullAccessKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self) // TODO: Deserialize first
    }
}

impl UtxoEntryFullAccessKey {
    /// Creates a new [UtxoEntryFullAccessKey] from a [ScriptPublicKeyBucket] and [TransactionOutpointKey].
    pub fn new(
        script_public_key_bucket: ScriptPublicKeyBucket,
        daa_score_key: DaaScoreKey,
        transaction_outpoint_key: TransactionOutpointKey,
    ) -> Self {
        let mut bytes =
            Vec::with_capacity(DAA_SCORE_KEY_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE + script_public_key_bucket.as_ref().len());
        bytes.extend_from_slice(script_public_key_bucket.as_ref());
        bytes.extend_from_slice(daa_score_key.as_ref());
        bytes.extend_from_slice(transaction_outpoint_key.as_ref());
        Self(Arc::new(bytes))
    }

    pub fn extract_outpoint(&self) -> TransactionOutpoint {
        TransactionOutpoint::from(TransactionOutpointKey(self.0[(self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE)..].try_into().unwrap()))
    }
}

impl AsRef<[u8]> for UtxoEntryFullAccessKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// Traits:

pub trait UtxoSetByScriptPublicKeyStoreReader {
    /// Get [UtxoSetByScriptPublicKey] set by queried [ScriptPublicKeys],
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;
    /// Get ordered UTXOs for multiple script public keys, bounded by an inclusive DAA-score range.
    fn get_utxos_from_script_public_keys_by_daa_score(
        &self,
        script_public_keys: ScriptPublicKeys,
        from_daa_score: Option<u64>,
        to_daa_score: Option<u64>,
    ) -> StoreResult<OrderedUtxoSetByScriptPublicKey>;
    fn get_balance_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey>;
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>>; // This can have a big memory footprint, so it should be used only for tests.
}

pub trait UtxoSetByScriptPublicKeyStore: UtxoSetByScriptPublicKeyStoreReader {
    /// remove [UtxoSetByScriptPublicKey] from the [UtxoSetByScriptPublicKeyStore].
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;

    /// add [UtxoSetByScriptPublicKey] into the [UtxoSetByScriptPublicKeyStore].
    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()>;

    /// removes all entries in the cache and db, besides prefixes themselves.
    fn delete_all(&mut self) -> StoreResult<()>;
}

// Implementations:

#[derive(Clone)]
pub struct DbUtxoSetByScriptPublicKeyStore {
    db: Arc<DB>,
    access: CachedDbAccess<UtxoEntryFullAccessKey, CompactUtxoEntry>,
}

impl DbUtxoSetByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndex.into()) }
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    // compared to go-kaspad this gets transaction outpoints from multiple script public keys at once.
    // TODO: probably ideal way to retrieve is to return a chained iterator which can be used to chunk results and propagate utxo entries
    // to the rpc via pagination, this would alleviate the memory footprint of script public keys with large amount of utxos.
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        let script_count = script_public_keys.len();
        let mut utxos_by_script_public_keys = UtxoSetByScriptPublicKey::new();
        let mut entries_count: usize = 0;
        for script_public_key in script_public_keys.into_iter() {
            let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
            let utxos_by_script_public_keys_inner = CompactUtxoCollection::from_iter(
                self.access.seek_iterator(Some(script_public_key_bucket.as_ref()), None, None, usize::MAX, false).map(|res| {
                    let (key, value) = res.unwrap();
                    (
                        UtxoEntryInnerKey(<[u8; DAA_SCORE_KEY_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE]>::try_from(&key[..]).unwrap())
                            .into(),
                        value,
                    )
                }),
            );
            entries_count += utxos_by_script_public_keys_inner.len();
            utxos_by_script_public_keys.insert(script_public_key, utxos_by_script_public_keys_inner);
        }
        debug!("IDXPRC, Executed a query for the utxo set of {} script public keys yielding {} entries", script_count, entries_count);
        Ok(utxos_by_script_public_keys)
    }

    fn get_utxos_from_script_public_keys_by_daa_score(
        &self,
        script_public_keys: ScriptPublicKeys,
        from_daa_score: Option<u64>,
        to_daa_score: Option<u64>,
    ) -> StoreResult<OrderedUtxoSetByScriptPublicKey> {
        let from_daa_score = from_daa_score.unwrap_or(0);
        let to_daa_score = to_daa_score.unwrap_or(u64::MAX);
        if from_daa_score > to_daa_score {
            return Ok(vec![]);
        }

        let script_count = script_public_keys.len();

        let mut script_public_keys = script_public_keys.into_iter().collect::<Vec<_>>();
        script_public_keys.sort_by(|a, b| a.version().cmp(&b.version()).then_with(|| a.script().cmp(b.script())));

        let mut entries_count: usize = 0;
        let mut utxos_by_script_public_keys = Vec::with_capacity(script_public_keys.len());
        for script_public_key in script_public_keys.into_iter() {
            let ordered_entries = {
                let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
                let seek_from = UtxoEntryFullAccessKey::new(
                    script_public_key_bucket.clone(),
                    from_daa_score.into(),
                    TransactionOutpointKey::EMPTY,
                );

                let seek_to = (to_daa_score < u64::MAX).then(|| {
                    UtxoEntryFullAccessKey::new(
                        script_public_key_bucket.clone(),
                        (to_daa_score + 1).into(),
                        TransactionOutpointKey::EMPTY,
                    )
                });

                let mut entries = Vec::new();
                for res in self.access.seek_iterator(None, Some(seek_from), seek_to, usize::MAX, false) {
                    let (key, value) = res.unwrap();
                    let bucket_len = script_public_key_bucket.as_ref().len();
                    let key_data: UtxoEntryKeyData = UtxoEntryInnerKey(
                        <[u8; DAA_SCORE_KEY_SIZE + TRANSACTION_OUTPOINT_KEY_SIZE]>::try_from(&key[bucket_len..]).unwrap(),
                    )
                    .into();
                    entries.push((key_data, value));
                }
                entries
            };
            entries_count += ordered_entries.len();
            utxos_by_script_public_keys.push((script_public_key, ordered_entries));
        }

        debug!(
            "IDXPRC, Executed a DAA-range query for the utxo set of {} script public keys yielding {} entries",
            script_count, entries_count
        );

        Ok(utxos_by_script_public_keys)
    }

    fn get_balance_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey> {
        let script_count = script_public_keys.len();
        let mut entries_count: usize = 0;
        let mut balance_by_script_public_keys = BalanceByScriptPublicKey::new();
        for script_public_key in script_public_keys.into_iter() {
            let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
            let balance: u64 = self
                .access
                .seek_iterator(Some(script_public_key_bucket.as_ref()), None, None, usize::MAX, false)
                .map(|res| {
                    entries_count += 1;
                    let (_, entry) = res.unwrap();
                    entry.amount
                })
                .sum();
            balance_by_script_public_keys.insert(script_public_key, balance);
        }
        debug!("IDXPRC, Executed a query for the balance of {} script public keys involving {} entries", script_count, entries_count);
        Ok(balance_by_script_public_keys)
    }

    // This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        Ok(HashSet::from_iter(
            self.access.iterator().map(|res| UtxoEntryFullAccessKey(Arc::new(res.unwrap().0.to_vec())).extract_outpoint()),
        ))
    }
}

impl UtxoSetByScriptPublicKeyStore for DbUtxoSetByScriptPublicKeyStore {
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_remove = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.keys().map(move |utxo_entry_key_data| {
                UtxoEntryFullAccessKey::new(
                    ScriptPublicKeyBucket::from(script_public_key),
                    DaaScoreKey::from(utxo_entry_key_data.daa_score),
                    TransactionOutpointKey::from(&utxo_entry_key_data.transaction_outpoint),
                )
            })
        });

        self.access.delete_many(&mut writer, &mut to_remove)?;

        Ok(())
    }

    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByScriptPublicKey) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_add = utxo_entries.iter().flat_map(move |(script_public_key, compact_utxo_collection)| {
            compact_utxo_collection.iter().map(move |(utxo_entry_key_data, compact_utxo)| {
                (
                    UtxoEntryFullAccessKey::new(
                        ScriptPublicKeyBucket::from(script_public_key),
                        DaaScoreKey::from(utxo_entry_key_data.daa_score),
                        TransactionOutpointKey::from(&utxo_entry_key_data.transaction_outpoint),
                    ),
                    *compact_utxo,
                )
            })
        });

        self.access.write_many(&mut writer, &mut to_add)?;

        Ok(())
    }

    /// Removes all entries in the cache and db, besides prefixes themselves.
    fn delete_all(&mut self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};

    fn outpoint(word: u64, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_u64_word(word), index)
    }

    #[test]
    fn test_get_utxos_from_script_public_keys_by_daa_score_is_ordered_and_range_filtered() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty);

        // Intentionally unsorted by script bytes so output ordering can be asserted.
        let script_public_key_a = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_public_key_b = ScriptPublicKey::from_vec(0, vec![0x01]);

        let mut to_add = UtxoSetByScriptPublicKey::new();
        to_add.insert(
            script_public_key_a.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeyData::new(10, outpoint(10, 0)), CompactUtxoEntry::new(100, false)),
                (UtxoEntryKeyData::new(20, outpoint(20, 0)), CompactUtxoEntry::new(200, false)),
            ]),
        );
        to_add.insert(
            script_public_key_b.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeyData::new(15, outpoint(15, 0)), CompactUtxoEntry::new(150, false)),
                (UtxoEntryKeyData::new(25, outpoint(25, 0)), CompactUtxoEntry::new(250, false)),
            ]),
        );

        store.add_utxo_entries(&to_add).unwrap();

        let ordered = store
            .get_utxos_from_script_public_keys_by_daa_score(
                ScriptPublicKeys::from_iter([script_public_key_a.clone(), script_public_key_b.clone()]),
                Some(12),
                Some(22),
            )
            .unwrap();

        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[0].0, script_public_key_b);
        assert_eq!(ordered[1].0, script_public_key_a);

        assert_eq!(ordered[0].1.len(), 1);
        assert_eq!(ordered[0].1[0].0.daa_score, 15);
        assert_eq!(ordered[1].1.len(), 1);
        assert_eq!(ordered[1].1[0].0.daa_score, 20);

        // Sanity check for the non-range API.
        let all = store
            .get_utxos_from_script_public_keys(ScriptPublicKeys::from_iter([script_public_key_a.clone(), script_public_key_b.clone()]))
            .unwrap();
        assert_eq!(all.get(&script_public_key_a).unwrap().len(), 2);
        assert_eq!(all.get(&script_public_key_b).unwrap().len(), 2);
    }
}
