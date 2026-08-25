use crate::core::model::{
    BalanceByScriptPublicKey, CompactUtxoCollection, CompactUtxoEntry, OrderedUtxoEntriesPage, UtxoEntryKeySuffixRecord,
    UtxoPageCursor, UtxoSetByScriptPublicKey,
};
use crate::errors::{UtxoIndexError, UtxoIndexResult};

use indexmap::IndexSet;
use itertools::Itertools;
use kaspa_consensus_core::tx::{
    ScriptPublicKey, ScriptPublicKeyVersion, ScriptPublicKeys, ScriptVec, TransactionIndexType, TransactionOutpoint,
};
use kaspa_core::debug;
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::Display;

use std::ops::RangeInclusive;
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

impl From<&u64> for DaaScoreKey {
    fn from(daa_score: &u64) -> Self {
        DaaScoreKey(daa_score.to_be_bytes())
    }
}

impl AsRef<[u8]> for DaaScoreKey {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Full [CompactUtxoEntry] access key.
/// Consists of variable amount of bytes of [ScriptPublicKeyBucket], followed by [DaaScoreKey], and [TransactionOutpointKey].
#[derive(Eq, Hash, PartialEq, Debug, Clone, Serialize, Deserialize)]
struct UtxoEntryDbKey(Arc<Vec<u8>>);

impl Display for UtxoEntryDbKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self) // TODO: Deserialize first
    }
}

impl UtxoEntryDbKey {
    /// Creates a new [UtxoEntryDbKey] from a [ScriptPublicKeyBucket] and [TransactionOutpointKey].
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

    pub fn extract_data(&self) -> (ScriptPublicKey, UtxoEntryKeySuffixRecord) {
        let script_public_key = ScriptPublicKey::from(ScriptPublicKeyBucket(
            self.0[..(self.0.len() - DAA_SCORE_KEY_SIZE - TRANSACTION_OUTPOINT_KEY_SIZE)].to_vec(),
        ));
        let daa_score = u64::from_be_bytes(
            self.0
                [(self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE - DAA_SCORE_KEY_SIZE)..(self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE)]
                .try_into()
                .unwrap(),
        );
        let transaction_outpoint = TransactionOutpoint::from(TransactionOutpointKey(
            self.0[(self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE)..].try_into().unwrap(),
        ));
        (script_public_key, UtxoEntryKeySuffixRecord::new(daa_score, transaction_outpoint))
    }
}

impl From<UtxoPageCursor> for UtxoEntryDbKey {
    fn from(cursor: UtxoPageCursor) -> Self {
        // Unwrap optionals to their minimal (default) values for the seek operation
        let script_public_key = cursor.script_public_key.unwrap_or_else(ScriptPublicKey::empty);
        let daa_score = cursor.daa_score.unwrap_or(0);
        let transaction_outpoint = cursor.transaction_outpoint.unwrap_or(TransactionOutpoint::EMPTY);

        let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
        let daa_score_key = DaaScoreKey::from(daa_score);
        let transaction_outpoint_key = TransactionOutpointKey::from(&transaction_outpoint);
        Self::new(script_public_key_bucket, daa_score_key, transaction_outpoint_key)
    }
}

impl AsRef<[u8]> for UtxoEntryDbKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

// Traits:

pub trait UtxoSetByScriptPublicKeyStoreReader {
    /// Get [UtxoSetByScriptPublicKey] set by queried [ScriptPublicKeys],
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey>;
    /// Get ordered UTXOs for multiple script public keys with an optional DAA-score range and cursor pagination.
    fn get_utxos_from_script_public_keys_by_daa_score_page(
        &self,
        script_public_keys: IndexSet<ScriptPublicKey>,
        daa_score_range: RangeInclusive<u64>,
        cursor: UtxoPageCursor,
        limit: Option<u64>,
    ) -> UtxoIndexResult<OrderedUtxoEntriesPage>;
    fn get_balance_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<BalanceByScriptPublicKey>;
    /// This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>>;
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
    access: CachedDbAccess<UtxoEntryDbKey, CompactUtxoEntry>,
}

impl DbUtxoSetByScriptPublicKeyStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndex.into()) }
    }
}

impl UtxoSetByScriptPublicKeyStoreReader for DbUtxoSetByScriptPublicKeyStore {
    // compared to go-kaspad this gets transaction outpoints from multiple script public keys at once.
    fn get_utxos_from_script_public_keys(&self, script_public_keys: ScriptPublicKeys) -> StoreResult<UtxoSetByScriptPublicKey> {
        let script_count = script_public_keys.len();
        let mut utxos_by_script_public_keys = UtxoSetByScriptPublicKey::new();
        let mut entries_count: usize = 0;
        for script_public_key in script_public_keys.into_iter() {
            let script_public_key_bucket = ScriptPublicKeyBucket::from(&script_public_key);
            let utxos_by_script_public_keys_inner = CompactUtxoCollection::from_iter(
                // TODO: consider re-writing this with the multi_range_seek_iterator.
                self.access.seek_iterator(Some(script_public_key_bucket.as_ref()), None, None, usize::MAX, false).map(|res| {
                    let (key, value) = res.unwrap();
                    (
                        UtxoEntryKeySuffixRecord::new(
                            u64::from_be_bytes(
                                key[(key.len() - TRANSACTION_OUTPOINT_KEY_SIZE - DAA_SCORE_KEY_SIZE)
                                    ..(key.len() - TRANSACTION_OUTPOINT_KEY_SIZE)]
                                    .try_into()
                                    .unwrap(),
                            ),
                            TransactionOutpoint::from(TransactionOutpointKey(
                                key[(key.len() - TRANSACTION_OUTPOINT_KEY_SIZE)..].try_into().unwrap(),
                            )),
                        ),
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

    fn get_utxos_from_script_public_keys_by_daa_score_page(
        &self,
        mut script_public_keys: IndexSet<ScriptPublicKey>,
        daa_score_range: RangeInclusive<u64>,
        mut cursor: UtxoPageCursor,
        limit: Option<u64>,
    ) -> UtxoIndexResult<OrderedUtxoEntriesPage> {
        if script_public_keys.is_empty() {
            // user queried for no script public keys, we define this as an invalid query.
            // Potential TODO: We might also want to define this as "all" script public keys, in some future,
            // this would allow for callers to query all utxos within daa-score range-bounds.
            return Err(UtxoIndexError::QueryingEmptyAddressSet);
        }

        // cursor daa_score is outside of the specified range, thus it is invalid.
        if cursor.daa_score.is_some_and(|s| s < *daa_score_range.start() || s > *daa_score_range.end()) {
            return Err(UtxoIndexError::InvalidCursor(cursor));
        }

        let mut is_cursor_valid = cursor.script_public_key.is_none(); // if no cursor is provided, we consider it valid (start from beginning)

        // Filter out script public keys which are less than the cursor script public key, if it exists
        script_public_keys.retain(|spk| {
            // Check to find if a specified cursor script public key is within the queried script public keys, if it exists.
            if !is_cursor_valid && spk == cursor.script_public_key.as_ref().unwrap() {
                // if none, is_cursor_valid is already true.
                is_cursor_valid = true;
                return true;
            }

            // we filter out script public keys which are less than the cursor script public key, if it exists
            // since the cursor has pointed past this point we can assume that any caller has already seen these script public keys and thus we can skip them.
            cursor.script_public_key.is_none() || spk >= cursor.script_public_key.as_ref().unwrap()
        });

        // cursor is not pointing into the script public key set.
        if !is_cursor_valid {
            return Err(UtxoIndexError::InvalidCursor(cursor));
        }

        // sort the script public keys in order to return them in a deterministic order
        script_public_keys.sort_unstable();

        let spk_max = script_public_keys.last().unwrap().clone();

        let key_ranges = script_public_keys.into_iter().map(|script_public_key| {
            let start_key = UtxoEntryDbKey::new(
                ScriptPublicKeyBucket::from(&script_public_key),
                DaaScoreKey::from(*daa_score_range.start()),
                TransactionOutpointKey::from(&TransactionOutpoint::EMPTY),
            );
            let end_key = UtxoEntryDbKey::new(
                ScriptPublicKeyBucket::from(&script_public_key),
                DaaScoreKey::from(*daa_score_range.end()),
                TransactionOutpointKey::from(&TransactionOutpoint::MAX),
            );
            RangeInclusive::new(start_key, end_key)
        });

        // +1 in order to return the next cursor
        let extended_limit = limit.map(|l| l.saturating_add(1)).unwrap_or(u64::MAX).try_into().unwrap_or(usize::MAX);

        // set the cursor daa_score to the start of the range if it is not already set.
        cursor.daa_score = cursor.daa_score.or(Some(*daa_score_range.start()));

        let mut number_of_entries: usize = 0;
        let mut entries = self
            .access
            .multi_range_seek_iterator(
                key_ranges,
                Some(cursor.into()),
                Some(UtxoEntryDbKey::new(
                    ScriptPublicKeyBucket::from(&spk_max),
                    DaaScoreKey::from(*daa_score_range.end()),
                    TransactionOutpointKey::from(&TransactionOutpoint::MAX),
                )),
                extended_limit,
            )
            .map(|res| {
                let (key, value) = res.unwrap();
                let db_key = UtxoEntryDbKey(Arc::new(key.to_vec()));
                let (script_public_key, utxo_key_suffix_record) = db_key.extract_data();
                number_of_entries += 1;
                (script_public_key, utxo_key_suffix_record, value)
            })
            .chunk_by(|(spk, _, _)| spk.clone())
            .into_iter()
            .map(|(spk, chunk)| (spk, chunk.map(|(_, key_data, value)| (key_data, value)).collect::<Vec<_>>()))
            .collect::<Vec<_>>();

        let next_cursor = if (number_of_entries < extended_limit) || entries.is_empty() {
            // we have exhausted the search space, thus we return no next cursor
            None
        } else {
            let (spk, mut spk_entries) = entries.pop().unwrap();
            let last_entry = spk_entries.pop().unwrap();
            if !spk_entries.is_empty() {
                // we have more entries for this script public key, thus we push it back to the entries list
                entries.push((spk.clone(), spk_entries));
            };
            Some(UtxoPageCursor::new(Some(spk), Some(last_entry.0.daa_score()), Some(*last_entry.0.transaction_outpoint())))
        };

        Ok(OrderedUtxoEntriesPage::new(Arc::new(entries), next_cursor))
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

    /// This can have a big memory footprint, so it should be used only for tests.
    fn get_all_outpoints(&self) -> StoreResult<HashSet<TransactionOutpoint>> {
        Ok(HashSet::from_iter(
            self.access
                .iterator()
                .map(|res| *UtxoEntryDbKey(Arc::new(res.unwrap().0.to_vec())).extract_data().1.transaction_outpoint()),
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
            compact_utxo_collection.keys().map(move |utxo_key_suffix_record| {
                UtxoEntryDbKey::new(
                    ScriptPublicKeyBucket::from(script_public_key),
                    DaaScoreKey::from(utxo_key_suffix_record.daa_score()),
                    TransactionOutpointKey::from(utxo_key_suffix_record.transaction_outpoint()),
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
            compact_utxo_collection.iter().map(move |(utxo_key_suffix_record, compact_utxo)| {
                (
                    UtxoEntryDbKey::new(
                        ScriptPublicKeyBucket::from(script_public_key),
                        DaaScoreKey::from(utxo_key_suffix_record.daa_score()),
                        TransactionOutpointKey::from(utxo_key_suffix_record.transaction_outpoint()),
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

    fn create_outpoint(word: u64, index: u32) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash::from_u64_word(word), index)
    }

    #[test]
    fn test_result_ordering_and_filtering() {
        // Tests that results are ordered by script public key and filtered by DAA score range
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty);

        let script_a = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x01]);

        let mut to_add = UtxoSetByScriptPublicKey::new();
        to_add.insert(
            script_a.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeySuffixRecord::new(10, create_outpoint(10, 0)), CompactUtxoEntry::new(100, false, None)),
                (UtxoEntryKeySuffixRecord::new(20, create_outpoint(20, 0)), CompactUtxoEntry::new(200, false, None)),
                (UtxoEntryKeySuffixRecord::new(30, create_outpoint(30, 0)), CompactUtxoEntry::new(300, false, None)),
            ]),
        );
        to_add.insert(
            script_b.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeySuffixRecord::new(15, create_outpoint(15, 0)), CompactUtxoEntry::new(150, false, None)),
                (UtxoEntryKeySuffixRecord::new(25, create_outpoint(25, 0)), CompactUtxoEntry::new(250, false, None)),
            ]),
        );

        store.add_utxo_entries(&to_add).unwrap();

        // Query with DAA range 12..=22: should match script_b[15] and script_a[20]
        // With limit specified, we return exactly limit entries (when we have them)
        let page = store
            .get_utxos_from_script_public_keys_by_daa_score_page(
                IndexSet::<ScriptPublicKey>::from_iter([script_a.clone(), script_b.clone()]),
                12..=22,
                UtxoPageCursor::new(None, None, None),
                Some(2), // Specify limit to ensure pagination behavior
            )
            .unwrap();

        // Results should be sorted by script public key (script_b=0x01 comes before script_a=0x02)
        assert_eq!(page.entries().len(), 2);
        assert_eq!(page.entries()[0].0, script_b); // 0x01
        assert_eq!(page.entries()[1].0, script_a); // 0x02

        // With limit=2, we fetch 3, but only get 2 matching entries total
        // So we get both: [script_b[15], script_a[20]], no cursor
        assert_eq!(page.entries()[0].1.len(), 1);
        assert_eq!(page.entries()[0].1[0].0.daa_score(), 15);
        assert_eq!(page.entries()[1].1.len(), 1);
        assert_eq!(page.entries()[1].1[0].0.daa_score(), 20);
        assert!(page.next_cursor().is_none());
    }

    #[test]
    fn test_pagination_with_limit() {
        // Tests cursor behavior: fetch limit+1
        // When number_of_entries == extended_limit-1: no cursor (exactly at limit)
        // Otherwise: pop last entry and create cursor
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty);

        let script = ScriptPublicKey::from_vec(0, vec![0x01]);

        let mut to_add = UtxoSetByScriptPublicKey::new();
        to_add.insert(
            script.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeySuffixRecord::new(10, create_outpoint(10, 0)), CompactUtxoEntry::new(100, false, None)),
                (UtxoEntryKeySuffixRecord::new(20, create_outpoint(20, 0)), CompactUtxoEntry::new(200, false, None)),
                (UtxoEntryKeySuffixRecord::new(30, create_outpoint(30, 0)), CompactUtxoEntry::new(300, false, None)),
                (UtxoEntryKeySuffixRecord::new(40, create_outpoint(40, 0)), CompactUtxoEntry::new(400, false, None)),
            ]),
        );

        store.add_utxo_entries(&to_add).unwrap();

        // First page with limit=2: fetch 3
        // Get entries [10, 20, 30]
        // number_of_entries=3, extended_limit=3, so 3 != 2 → pop last, return 2 + cursor
        let page1 = store
            .get_utxos_from_script_public_keys_by_daa_score_page(
                IndexSet::<ScriptPublicKey>::from_iter([script.clone()]),
                0..=u64::MAX,
                UtxoPageCursor::new(None, None, None),
                Some(2),
            )
            .unwrap();

        assert_eq!(page1.entries()[0].1.len(), 2); // We got 3, pop 1, return 2
        assert_eq!(page1.entries()[0].1[0].0.daa_score(), 10);
        assert_eq!(page1.entries()[0].1[1].0.daa_score(), 20);
        assert!(page1.next_cursor().is_some());

        let cursor1 = page1.next_cursor().unwrap();
        assert_eq!(cursor1.script_public_key, Some(script.clone()));
        assert_eq!(cursor1.daa_score, Some(30));
        assert_eq!(cursor1.transaction_outpoint, Some(create_outpoint(30, 0)));

        // Second page using cursor from first
        let page2 = store
            .get_utxos_from_script_public_keys_by_daa_score_page(
                IndexSet::<ScriptPublicKey>::from_iter([script.clone()]),
                0..=u64::MAX,
                cursor1.clone(),
                Some(2),
            )
            .unwrap();

        // Starting from cursor(30), we get [30, 40], which is 2 entries
        // number_of_entries=2, extended_limit=3, so 2 == 2 → no cursor
        assert_eq!(page2.entries()[0].1.len(), 2);
        assert_eq!(page2.entries()[0].1[0].0.daa_score(), 30);
        assert_eq!(page2.entries()[0].1[1].0.daa_score(), 40);
        assert!(page2.next_cursor().is_none()); // Exhausted
    }

    #[test]
    fn test_daa_score_range_filtering() {
        // Tests that DAA score range filtering works correctly
        // When we get fewer entries than extended_limit, cursor logic still applies
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty);

        let script = ScriptPublicKey::from_vec(0, vec![0x01]);

        let mut to_add = UtxoSetByScriptPublicKey::new();
        to_add.insert(
            script.clone(),
            CompactUtxoCollection::from_iter([
                (UtxoEntryKeySuffixRecord::new(10, create_outpoint(10, 0)), CompactUtxoEntry::new(100, false, None)),
                (UtxoEntryKeySuffixRecord::new(20, create_outpoint(20, 0)), CompactUtxoEntry::new(200, false, None)),
                (UtxoEntryKeySuffixRecord::new(30, create_outpoint(30, 0)), CompactUtxoEntry::new(300, false, None)),
                (UtxoEntryKeySuffixRecord::new(40, create_outpoint(40, 0)), CompactUtxoEntry::new(400, false, None)),
            ]),
        );

        store.add_utxo_entries(&to_add).unwrap();

        // Query with range 15..=35 and limit=5: fetch 6, get [20, 30], only 2 entries
        // number_of_entries=2, extended_limit=6, so 2 != 5 → pop last and create cursor
        let page = store
            .get_utxos_from_script_public_keys_by_daa_score_page(
                IndexSet::<ScriptPublicKey>::from_iter([script.clone()]),
                15..=35,
                UtxoPageCursor::new(None, None, None),
                Some(5),
            )
            .unwrap();

        assert_eq!(page.entries().len(), 1);
        // We get 2 entries total, pop last one → 1 entry returned + 1 cursor
        assert_eq!(page.entries()[0].1.len(), 2);
        assert_eq!(page.entries()[0].1[0].0.daa_score(), 20);
        assert_eq!(page.entries()[0].1[1].0.daa_score(), 30);
        assert!(page.next_cursor().is_none());
    }
}
