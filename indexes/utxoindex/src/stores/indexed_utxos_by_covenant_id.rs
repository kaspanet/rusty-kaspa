use crate::core::model::{
    CompactUtxoEntry, CovenantUtxoEntriesByGroup, CovenantUtxoPageCursor, OrderedCovenantUtxoEntriesPage, UtxoEntryKeySuffixRecord,
    UtxoSetByCovenantId,
};
use crate::errors::{UtxoIndexError, UtxoIndexResult};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH_SIZE, Hash};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::sync::Arc;

use super::indexed_utxos::{
    DAA_SCORE_KEY_SIZE, DaaScoreKey, DbUtxoSetByScriptPublicKeyStore, ScriptPublicKeyBucket, TRANSACTION_OUTPOINT_KEY_SIZE,
    TransactionOutpointKey,
};

/// Full covenant index key.
/// Consists of 32 bytes of covenant [`Hash`], followed by [DaaScoreKey], a variable amount of bytes of
/// [ScriptPublicKeyBucket], and [TransactionOutpointKey]. Ordering within a covenant id is daa score
/// asc, then script public key bucket bytes asc, then outpoint asc.
#[derive(Eq, Hash, PartialEq, PartialOrd, Ord, Debug, Clone, Serialize, Deserialize)]
pub struct CovenantUtxoEntryDbKey(Arc<Vec<u8>>);

impl CovenantUtxoEntryDbKey {
    /// Creates a new [CovenantUtxoEntryDbKey] from its parts.
    pub fn new(covenant_id: Hash, daa_score_key: DaaScoreKey, spk: ScriptPublicKeyBucket, outpoint: TransactionOutpointKey) -> Self {
        #[allow(clippy::arithmetic_side_effects, reason = "ARITH-SAFETY(LENGTH)")]
        let mut bytes = Vec::with_capacity(HASH_SIZE + DAA_SCORE_KEY_SIZE + spk.as_ref().len() + TRANSACTION_OUTPOINT_KEY_SIZE);
        bytes.extend_from_slice(covenant_id.as_slice());
        bytes.extend_from_slice(daa_score_key.as_ref());
        bytes.extend_from_slice(spk.as_ref());
        bytes.extend_from_slice(outpoint.as_ref());
        Self(Arc::new(bytes))
    }

    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(Arc::new(bytes.into()))
    }

    /// Bottom key of a (covenant id, daa score) prefix: sorts before every full key with that id and daa score.
    pub fn prefix_start(covenant_id: Hash, daa_score: u64) -> Self {
        let mut bytes = Vec::with_capacity(HASH_SIZE + DAA_SCORE_KEY_SIZE);
        bytes.extend_from_slice(covenant_id.as_slice());
        bytes.extend_from_slice(DaaScoreKey::from(daa_score).as_ref());
        Self(Arc::new(bytes))
    }

    /// Upper bound covering every key with this covenant id and daa score <= `to_daa_score`, and no key beyond.
    /// With an open daa bound this is the byte successor of the id; an all-0xFF id has no successor, so a
    /// one-byte-longer 0xFF sentinel is used, which dominates any key extension of the id by the prefix rule.
    pub fn range_end(covenant_id: Hash, to_daa_score: u64) -> Self {
        if to_daa_score < u64::MAX {
            return Self::prefix_start(covenant_id, to_daa_score + 1);
        }
        let mut bytes = covenant_id.as_bytes().to_vec();
        if bytes.iter().all(|b| *b == 0xFF) {
            bytes.push(0xFF);
        } else {
            for b in bytes.iter_mut().rev() {
                if *b == 0xFF {
                    *b = 0x00;
                } else {
                    *b += 1;
                    break;
                }
            }
        }
        Self(Arc::new(bytes))
    }

    pub fn extract_data(&self) -> (Hash, ScriptPublicKey, UtxoEntryKeySuffixRecord) {
        let covenant_id = Hash::from_slice(&self.0[..HASH_SIZE]);
        #[allow(
            clippy::arithmetic_side_effects,
            reason = "HASH_SIZE, DAA_SCORE_KEY_SIZE and TRANSACTION_OUTPOINT_KEY_SIZE are fixed; the spk bucket takes the remainder"
        )]
        let spk_end = self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE;
        let daa_end = HASH_SIZE + DAA_SCORE_KEY_SIZE;
        let daa_score = u64::from_be_bytes(self.0[HASH_SIZE..daa_end].try_into().expect("expected daa score size"));
        let script_public_key = ScriptPublicKey::from(ScriptPublicKeyBucket::from_vec(self.0[daa_end..spk_end].to_vec()));
        let outpoint = TransactionOutpoint::from(TransactionOutpointKey::from_bytes(
            self.0[spk_end..].try_into().expect("expected outpoint size"),
        ));
        (covenant_id, script_public_key, UtxoEntryKeySuffixRecord::new(daa_score, outpoint))
    }
}

impl AsRef<[u8]> for CovenantUtxoEntryDbKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// The covenant index carries no payload; values exist only to satisfy the db access layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantIndexEntry;

impl MemSizeEstimator for CovenantIndexEntry {}

// Traits:

pub trait UtxoSetByCovenantIdStoreReader {
    /// Get an ordered page of UTXOs for multiple covenant ids with an optional DAA-score range,
    /// optional script public key bounds, and cursor pagination. Ids are traversed in request order;
    /// amount and is_coinbase are resolved per row from the address store.
    fn get_utxos_by_covenant_ids_page(
        &self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        from_spk: Option<ScriptPublicKey>,
        to_spk: Option<ScriptPublicKey>,
        cursor: Option<CovenantUtxoPageCursor>,
        limit: Option<usize>,
    ) -> UtxoIndexResult<OrderedCovenantUtxoEntriesPage>;
}

#[allow(dead_code)]
pub trait UtxoSetByCovenantIdStore: UtxoSetByCovenantIdStoreReader {
    /// Remove [UtxoSetByCovenantId] from the [UtxoSetByCovenantIdStore].
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()>;

    /// Add [UtxoSetByCovenantId] into the [UtxoSetByCovenantIdStore].
    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()>;

    /// Removes all entries in the cache and db, besides prefixes themselves.
    fn delete_all(&mut self) -> StoreResult<()>;
}

// Implementations:

#[derive(Clone)]
pub struct DbUtxoSetByCovenantIdStore {
    db: Arc<DB>,
    access: CachedDbAccess<CovenantUtxoEntryDbKey, CovenantIndexEntry>,
    address_store: DbUtxoSetByScriptPublicKeyStore,
}

impl DbUtxoSetByCovenantIdStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, address_store: DbUtxoSetByScriptPublicKeyStore) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndexCovenant.into()),
            address_store,
        }
    }
}

impl UtxoSetByCovenantIdStoreReader for DbUtxoSetByCovenantIdStore {
    fn get_utxos_by_covenant_ids_page(
        &self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        from_spk: Option<ScriptPublicKey>,
        to_spk: Option<ScriptPublicKey>,
        cursor: Option<CovenantUtxoPageCursor>,
        limit: Option<usize>,
    ) -> UtxoIndexResult<OrderedCovenantUtxoEntriesPage> {
        if covenant_ids.is_empty() {
            return Err(UtxoIndexError::QueryingEmptyCovenantIdSet);
        }
        let first = covenant_ids[0];
        let cursor = match cursor {
            Some(c) => {
                if c.covenant_id != first || c.daa_score < *daa_score_range.start() || c.daa_score > *daa_score_range.end() {
                    return Err(UtxoIndexError::InvalidCovenantCursor(c));
                }
                c
            }
            None => CovenantUtxoPageCursor::new(first, *daa_score_range.start(), None, TransactionOutpoint::EMPTY),
        };

        // one key range per covenant id, traversed in request order; the first range resumes at
        // the cursor. Each range gets its own seek pass, since request order need not be ascending.
        let first_start = match &cursor.script_public_key {
            Some(script_public_key) => CovenantUtxoEntryDbKey::new(
                cursor.covenant_id,
                DaaScoreKey::from(cursor.daa_score),
                ScriptPublicKeyBucket::from(script_public_key),
                TransactionOutpointKey::from(&cursor.transaction_outpoint),
            ),
            None => CovenantUtxoEntryDbKey::prefix_start(cursor.covenant_id, cursor.daa_score),
        };
        let key_ranges = std::iter::once(first_start..=CovenantUtxoEntryDbKey::range_end(first, *daa_score_range.end())).chain(
            covenant_ids.into_iter().skip(1).map(|id| {
                CovenantUtxoEntryDbKey::prefix_start(id, *daa_score_range.start())
                    ..=CovenantUtxoEntryDbKey::range_end(id, *daa_score_range.end())
            }),
        );

        let extended_limit = limit.map(|l| l.saturating_add(1)).unwrap_or(usize::MAX);
        // bounds compare on the bucket encoding, matching store key order
        let from_bucket = from_spk.as_ref().map(ScriptPublicKeyBucket::from);
        let to_bucket = to_spk.as_ref().map(ScriptPublicKeyBucket::from);
        let mut rows: Vec<(Hash, ScriptPublicKey, UtxoEntryKeySuffixRecord, CompactUtxoEntry)> = key_ranges
            .flat_map(|key_range| self.access.multi_range_seek_iterator(std::iter::once(key_range)))
            .map(|res| {
                let (key, _) = res.unwrap();
                CovenantUtxoEntryDbKey::from_bytes(key.to_vec()).extract_data()
            })
            .filter(|(_, script_public_key, _)| {
                let bucket = ScriptPublicKeyBucket::from(script_public_key);
                from_bucket.as_ref().is_none_or(|f| bucket.as_ref() >= f.as_ref())
                    && to_bucket.as_ref().is_none_or(|t| bucket.as_ref() <= t.as_ref())
            })
            .take(extended_limit)
            .map(|(covenant_id, script_public_key, suffix)| {
                let entry = self.address_store.get_compact_utxo_entry(
                    &script_public_key,
                    suffix.daa_score(),
                    suffix.transaction_outpoint(),
                )?;
                debug_assert_eq!(entry.covenant_id, Some(covenant_id));
                Ok((covenant_id, script_public_key, suffix, entry))
            })
            .collect::<Result<Vec<_>, UtxoIndexError>>()?;

        // the limit + 1 probe row is dropped and returned as the resume position
        let next_cursor = if rows.len() < extended_limit {
            None
        } else {
            let (covenant_id, script_public_key, suffix, _) = rows.pop().unwrap();
            Some(CovenantUtxoPageCursor::new(covenant_id, suffix.daa_score(), Some(script_public_key), *suffix.transaction_outpoint()))
        };

        let mut entries: CovenantUtxoEntriesByGroup = Vec::new();
        for (covenant_id, script_public_key, suffix, entry) in rows {
            match entries.last_mut() {
                Some((last_id, group)) if *last_id == covenant_id => group.push((script_public_key, suffix, entry)),
                _ => entries.push((covenant_id, vec![(script_public_key, suffix, entry)])),
            }
        }

        Ok(OrderedCovenantUtxoEntriesPage::new(Arc::new(entries), next_cursor))
    }
}

impl UtxoSetByCovenantIdStore for DbUtxoSetByCovenantIdStore {
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_remove = utxo_entries.iter().flat_map(move |(covenant_id, entries)| {
            entries.iter().map(move |(script_public_key, utxo_key_suffix_record)| {
                CovenantUtxoEntryDbKey::new(
                    *covenant_id,
                    DaaScoreKey::from(utxo_key_suffix_record.daa_score()),
                    ScriptPublicKeyBucket::from(script_public_key),
                    TransactionOutpointKey::from(utxo_key_suffix_record.transaction_outpoint()),
                )
            })
        });

        self.access.delete_many(&mut writer, &mut to_remove)?;

        Ok(())
    }

    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut writer = DirectDbWriter::new(&self.db);

        let mut to_add = utxo_entries.iter().flat_map(move |(covenant_id, entries)| {
            entries.iter().map(move |(script_public_key, utxo_key_suffix_record)| {
                (
                    CovenantUtxoEntryDbKey::new(
                        *covenant_id,
                        DaaScoreKey::from(utxo_key_suffix_record.daa_score()),
                        ScriptPublicKeyBucket::from(script_public_key),
                        TransactionOutpointKey::from(utxo_key_suffix_record.transaction_outpoint()),
                    ),
                    CovenantIndexEntry,
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
    fn test_covenant_key_roundtrip() {
        let id = Hash::from_slice(&[7u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![1, 2, 3]);
        let outpoint = create_outpoint(9, 4);
        let key = CovenantUtxoEntryDbKey::new(
            id,
            42u64.into(),
            ScriptPublicKeyBucket::from(&script_public_key),
            TransactionOutpointKey::from(&outpoint),
        );
        let (parsed_id, parsed_script_public_key, suffix) = key.extract_data();
        assert_eq!(parsed_id, id);
        assert_eq!(parsed_script_public_key, script_public_key);
        assert_eq!(suffix.daa_score(), 42);
        assert_eq!(*suffix.transaction_outpoint(), outpoint);
    }

    #[test]
    fn test_covenant_rows_are_ordered_id_daa_spk_outpoint() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByCovenantIdStore::new(
            db.clone(),
            CachePolicy::Empty,
            DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty),
        );

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[2u8; HASH_SIZE]);
        let script_low = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_high = ScriptPublicKey::from_vec(0, vec![0x02]);

        let mut to_add = UtxoSetByCovenantId::new();
        to_add.insert(
            id_a,
            vec![
                (script_high.clone(), UtxoEntryKeySuffixRecord::new(2, create_outpoint(2, 0))),
                (script_high.clone(), UtxoEntryKeySuffixRecord::new(1, create_outpoint(3, 0))),
                (script_low.clone(), UtxoEntryKeySuffixRecord::new(1, create_outpoint(1, 0))),
            ],
        );
        to_add.insert(id_b, vec![(script_low, UtxoEntryKeySuffixRecord::new(0, create_outpoint(4, 0)))]);
        store.add_utxo_entries(&to_add).unwrap();

        let scanned: Vec<(Hash, u64, Vec<u8>)> = store
            .access
            .iterator()
            .map(|res| {
                let key = CovenantUtxoEntryDbKey::from_bytes(res.unwrap().0.to_vec());
                let (id, script_public_key, suffix) = key.extract_data();
                (id, suffix.daa_score(), script_public_key.script().to_vec())
            })
            .collect();

        assert_eq!(scanned.iter().map(|(id, _, _)| *id).collect::<Vec<_>>(), vec![id_a, id_a, id_a, id_b]);
        assert_eq!(scanned.iter().map(|(_, daa, _)| *daa).collect::<Vec<_>>(), vec![1, 1, 2, 0]);
        assert_eq!(
            scanned.iter().map(|(_, _, script)| script.clone()).collect::<Vec<_>>(),
            vec![vec![0x01], vec![0x02], vec![0x02], vec![0x01]]
        );
    }

    #[test]
    fn test_covenant_add_remove_roundtrip() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbUtxoSetByCovenantIdStore::new(
            db.clone(),
            CachePolicy::Empty,
            DbUtxoSetByScriptPublicKeyStore::new(db, CachePolicy::Empty),
        );

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[2u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);

        let mut to_add = UtxoSetByCovenantId::new();
        to_add.insert(id_a, vec![(script_public_key.clone(), UtxoEntryKeySuffixRecord::new(5, create_outpoint(1, 0)))]);
        to_add.insert(id_b, vec![(script_public_key.clone(), UtxoEntryKeySuffixRecord::new(6, create_outpoint(2, 0)))]);
        store.add_utxo_entries(&to_add).unwrap();
        assert_eq!(store.access.iterator().count(), 2);

        let mut to_remove = UtxoSetByCovenantId::new();
        to_remove.insert(id_a, vec![(script_public_key, UtxoEntryKeySuffixRecord::new(5, create_outpoint(1, 0)))]);
        store.remove_utxo_entries(&to_remove).unwrap();
        assert_eq!(store.access.iterator().count(), 1);

        store.delete_all().unwrap();
        assert_eq!(store.access.iterator().count(), 0);
    }

    #[test]
    fn test_covenant_range_bounds() {
        let id = Hash::from_slice(&[3u8; HASH_SIZE]);
        let full_key = |daa: u64, script: Vec<u8>, outpoint: TransactionOutpoint| {
            CovenantUtxoEntryDbKey::new(
                id,
                daa.into(),
                ScriptPublicKeyBucket::from(&ScriptPublicKey::from_vec(0, script)),
                TransactionOutpointKey::from(&outpoint),
            )
        };
        // a start-of-(id, daa) key sorts before every full key with that id and daa
        assert!(CovenantUtxoEntryDbKey::prefix_start(id, 7) < full_key(7, vec![0x00], create_outpoint(0, 0)));
        // the range end covers daa == to_daa and excludes daa == to_daa + 1
        let daa_7 = full_key(7, vec![0xFF], create_outpoint(u64::MAX, u32::MAX));
        let daa_8 = full_key(8, vec![0x00], TransactionOutpoint::EMPTY);
        assert!(daa_7 < CovenantUtxoEntryDbKey::range_end(id, 7));
        assert!(daa_8 > CovenantUtxoEntryDbKey::range_end(id, 7));
        // an open daa bound covers everything under the id, and nothing after it
        let next_id_row = CovenantUtxoEntryDbKey::new(
            Hash::from_slice(&[4u8; HASH_SIZE]),
            0u64.into(),
            ScriptPublicKeyBucket::empty(),
            TransactionOutpointKey::from(&TransactionOutpoint::EMPTY),
        );
        assert!(daa_7 < CovenantUtxoEntryDbKey::range_end(id, u64::MAX));
        assert!(next_id_row > CovenantUtxoEntryDbKey::range_end(id, u64::MAX));
    }

    use crate::core::model::{CompactUtxoEntry, CovenantUtxoPageCursor, UtxoSetByScriptPublicKey};
    use crate::errors::UtxoIndexError;
    use crate::stores::indexed_utxos::UtxoSetByScriptPublicKeyStore as _;
    use std::ops::RangeInclusive;

    type PopulateRow = (Hash, u64, ScriptPublicKey, TransactionOutpoint, u64);

    fn populate(
        address_store: &mut DbUtxoSetByScriptPublicKeyStore,
        covenant_store: &mut DbUtxoSetByCovenantIdStore,
        rows: Vec<PopulateRow>,
    ) {
        let mut address = UtxoSetByScriptPublicKey::new();
        let mut covenant = UtxoSetByCovenantId::new();
        for (covenant_id, daa_score, script_public_key, outpoint, amount) in rows {
            address
                .entry(script_public_key.clone())
                .or_default()
                .insert(UtxoEntryKeySuffixRecord::new(daa_score, outpoint), CompactUtxoEntry::new(amount, false, Some(covenant_id)));
            covenant.entry(covenant_id).or_default().push((script_public_key, UtxoEntryKeySuffixRecord::new(daa_score, outpoint)));
        }
        address_store.add_utxo_entries(&address).unwrap();
        covenant_store.add_utxo_entries(&covenant).unwrap();
    }

    fn page_entries(
        store: &DbUtxoSetByCovenantIdStore,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        from_spk: Option<ScriptPublicKey>,
        to_spk: Option<ScriptPublicKey>,
        cursor: Option<CovenantUtxoPageCursor>,
        limit: Option<usize>,
    ) -> (Vec<(Hash, Vec<u64>)>, Option<CovenantUtxoPageCursor>) {
        let page = store.get_utxos_by_covenant_ids_page(covenant_ids, daa_score_range, from_spk, to_spk, cursor, limit).unwrap();
        let groups = page
            .entries()
            .iter()
            .map(|(covenant_id, rows)| (*covenant_id, rows.iter().map(|(_, suffix, _)| suffix.daa_score()).collect::<Vec<_>>()))
            .collect();
        (groups, page.next_cursor().cloned())
    }

    #[test]
    fn test_page_ordering_within_id_and_request_order() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut address_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&address_store));

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[2u8; HASH_SIZE]);
        let script_x = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_y = ScriptPublicKey::from_vec(0, vec![0x02]);

        populate(
            &mut address_store,
            &mut store,
            vec![
                (id_b, 5, script_x.clone(), create_outpoint(1, 0), 500),
                (id_b, 3, script_y.clone(), create_outpoint(2, 0), 300),
                (id_a, 10, script_x.clone(), create_outpoint(3, 0), 1000),
            ],
        );

        // request order [id_b, id_a] wins over lexicographic; rows within an id sort by daa
        let (groups, next_cursor) = page_entries(&store, vec![id_b, id_a], 0..=u64::MAX, None, None, None, None);
        assert_eq!(groups, vec![(id_b, vec![3, 5]), (id_a, vec![10])]);
        assert!(next_cursor.is_none());

        // amounts resolve from the address store
        let page = store.get_utxos_by_covenant_ids_page(vec![id_b], 0..=u64::MAX, None, None, None, None).unwrap();
        let amounts: Vec<u64> = page.entries()[0].1.iter().map(|(_, _, entry)| entry.amount).collect();
        assert_eq!(amounts, vec![300, 500]);
    }

    #[test]
    fn test_page_limit_and_next_cursor() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut address_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&address_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut address_store,
            &mut store,
            (1..=5u64).map(|daa| (id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa * 100)).collect(),
        );

        let (page1, cursor1) = page_entries(&store, vec![id], 0..=u64::MAX, None, None, None, Some(2));
        assert_eq!(page1, vec![(id, vec![1, 2])]);
        let cursor1 = cursor1.unwrap();
        assert_eq!((cursor1.covenant_id, cursor1.daa_score), (id, 3));

        let (page2, cursor2) = page_entries(&store, vec![id], 0..=u64::MAX, None, None, Some(cursor1), Some(2));
        assert_eq!(page2, vec![(id, vec![3, 4])]);
        let cursor2 = cursor2.unwrap();
        assert_eq!(cursor2.daa_score, 5);

        let (page3, cursor3) = page_entries(&store, vec![id], 0..=u64::MAX, None, None, Some(cursor2), Some(2));
        assert_eq!(page3, vec![(id, vec![5])]);
        assert!(cursor3.is_none());
    }

    #[test]
    fn test_page_daa_and_spk_bounds_filtering() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut address_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&address_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_low = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_high = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut address_store,
            &mut store,
            (1..=7u64)
                .map(|daa| {
                    let script = if daa % 2 == 0 { script_high.clone() } else { script_low.clone() };
                    (id, daa, script, create_outpoint(daa, 0), daa)
                })
                .collect(),
        );

        // daa bounds exclude the outer rows
        let (groups, _) = page_entries(&store, vec![id], 2..=6, None, None, None, None);
        assert_eq!(groups, vec![(id, vec![2, 3, 4, 5, 6])]);

        // spk bounds skip non-matching rows without counting them toward the limit
        let mut cursor = None;
        let mut pages = vec![];
        loop {
            let (groups, next) =
                page_entries(&store, vec![id], 0..=u64::MAX, Some(script_high.clone()), Some(script_high.clone()), cursor, Some(1));
            pages.push(groups[0].1.clone());
            cursor = match next {
                Some(c) => Some(c),
                None => break,
            };
        }
        assert_eq!(pages, vec![vec![2], vec![4], vec![6]]);
    }

    #[test]
    fn test_page_cursor_validation() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut address_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&address_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let other_id = Hash::from_slice(&[9u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(&mut address_store, &mut store, vec![(id, 1, script_public_key.clone(), create_outpoint(1, 0), 100)]);

        let out_of_range = CovenantUtxoPageCursor::new(id, 99, Some(script_public_key.clone()), create_outpoint(1, 0));
        let result = store.get_utxos_by_covenant_ids_page(vec![id], 0..=10, None, None, Some(out_of_range), None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::InvalidCovenantCursor(_)));

        let wrong_id = CovenantUtxoPageCursor::new(other_id, 1, Some(script_public_key), create_outpoint(1, 0));
        let result = store.get_utxos_by_covenant_ids_page(vec![id], 0..=10, None, None, Some(wrong_id), None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::InvalidCovenantCursor(_)));

        let result = store.get_utxos_by_covenant_ids_page(vec![], 0..=10, None, None, None, None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::QueryingEmptyCovenantIdSet));
    }

    #[test]
    fn test_page_spk_bounds_follow_bucket_ordering() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut address_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&address_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        // the bucket length field is little endian, so a 256-byte script sorts before a 255-byte one
        let script_255 = ScriptPublicKey::from_vec(0, vec![0xAA; 255]);
        let script_256 = ScriptPublicKey::from_vec(0, vec![0xAA; 256]);
        populate(
            &mut address_store,
            &mut store,
            vec![(id, 1, script_255.clone(), create_outpoint(1, 0), 100), (id, 2, script_256, create_outpoint(2, 0), 200)],
        );

        // both rows exist unbounded
        let (groups, _) = page_entries(&store, vec![id], 0..=u64::MAX, None, None, None, None);
        assert_eq!(groups, vec![(id, vec![1, 2])]);

        // a bound at the 255-byte script excludes the 256-byte script per store key order
        let (groups, _) = page_entries(&store, vec![id], 0..=u64::MAX, Some(script_255.clone()), Some(script_255), None, None);
        assert_eq!(groups, vec![(id, vec![1])]);
    }
}
