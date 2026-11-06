use crate::core::model::{
    CompactUtxoEntry, CovenantUtxoPageCursor, OrderedCovenantUtxoEntriesPage, UtxoEntryKeySuffixRecord, UtxoSetByCovenantId,
};
use crate::errors::{UtxoIndexError, UtxoIndexResult};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB, DbKey, DirectDbWriter, StoreError, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH_SIZE, Hash};
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::{ReadOptions, WriteBatch};
use serde::{Deserialize, Serialize};
use std::ops::RangeInclusive;
use std::sync::Arc;
use zerocopy::FromBytes;

use super::script_public_keys::{
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

    /// Bottom key of a (covenant id, daa score) prefix: sorts before every full key with that id and daa score.
    pub fn prefix_start(covenant_id: Hash, daa_score: u64) -> Self {
        let mut bytes = Vec::with_capacity(HASH_SIZE + DAA_SCORE_KEY_SIZE);
        bytes.extend_from_slice(covenant_id.as_slice());
        bytes.extend_from_slice(DaaScoreKey::from(daa_score).as_ref());
        Self(Arc::new(bytes))
    }

    /// Key of the first row of a (covenant id, daa score, script public key) group.
    pub fn spk_start(covenant_id: Hash, daa_score: u64, spk: &ScriptPublicKeyBucket) -> Self {
        Self::new(covenant_id, DaaScoreKey::from(daa_score), spk.clone(), TransactionOutpointKey::from(&TransactionOutpoint::EMPTY))
    }
}

impl AsRef<[u8]> for CovenantUtxoEntryDbKey {
    fn as_ref(&self) -> &[u8] {
        self.0.as_slice()
    }
}

/// Borrowed view over raw covenant index key bytes (without the store prefix byte), for
/// comparisons and parsing without allocation.
struct CovenantUtxoKeyView<'a>(&'a [u8]);

impl CovenantUtxoKeyView<'_> {
    fn covenant_id(&self) -> &[u8] {
        &self.0[..HASH_SIZE]
    }

    fn daa_score(&self) -> u64 {
        DaaScoreKey::read_from_bytes(&self.0[HASH_SIZE..HASH_SIZE + DAA_SCORE_KEY_SIZE]).expect("expected daa score size").get()
    }

    fn spk_bucket(&self) -> &[u8] {
        #[allow(
            clippy::arithmetic_side_effects,
            reason = "HASH_SIZE, DAA_SCORE_KEY_SIZE and TRANSACTION_OUTPOINT_KEY_SIZE are fixed; the spk bucket takes the remainder"
        )]
        let daa_end = HASH_SIZE + DAA_SCORE_KEY_SIZE;
        let spk_end = self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE;
        &self.0[daa_end..spk_end]
    }

    fn extract_data(self) -> (Hash, ScriptPublicKey, UtxoEntryKeySuffixRecord) {
        let covenant_id = Hash::from_slice(self.covenant_id());
        let script_public_key = ScriptPublicKey::from(ScriptPublicKeyBucket::from_vec(self.spk_bucket().to_vec()));
        let outpoint = TransactionOutpoint::from(TransactionOutpointKey::from_bytes(
            self.0[self.0.len() - TRANSACTION_OUTPOINT_KEY_SIZE..].try_into().expect("expected outpoint size"),
        ));
        (covenant_id, script_public_key, UtxoEntryKeySuffixRecord::new(self.daa_score(), outpoint))
    }
}

/// A row of the covenant page stream, or the terminal signal carrying the resume position
/// once the page's db-ops budget is spent.
enum CovenantRow {
    Row(Hash, ScriptPublicKey, UtxoEntryKeySuffixRecord),
    BudgetExhausted(CovenantUtxoPageCursor),
}

/// The covenant index carries no payload; values exist only to satisfy the db access layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CovenantIndexEntry;

impl MemSizeEstimator for CovenantIndexEntry {}

// Traits:

pub trait UtxoSetByCovenantIdStoreReader {
    /// Get an ordered page of UTXOs for multiple covenant ids with an inclusive DAA-score range,
    /// an optional script public key filter, and cursor pagination. Ids are traversed in
    /// ascending covenant id order regardless of request order; rows within an id follow store
    /// key order; amount and is_coinbase are resolved per row from the script public key store.
    /// An empty script public key list includes rows of every script public key. A cursor
    /// resumes at its covenant id, skipping ids that sort before it. `budget` bounds the
    /// weighted db-op cost (seeks cost [SEEK_OP_COST] units, advances [ADVANCE_OP_COST])
    /// spent for the page; the number of returned entries never exceeds it.
    fn get_utxos_by_covenant_ids_page(
        &self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        cursor: Option<CovenantUtxoPageCursor>,
        budget: Option<usize>,
    ) -> UtxoIndexResult<OrderedCovenantUtxoEntriesPage>;
}

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
    spk_store: DbUtxoSetByScriptPublicKeyStore,
}

const COVENANT_BACKFILL_BATCH_SIZE: usize = 2048;

/// Page budget of weighted db-op costs when the caller supplies no budget.
const DEFAULT_COVENANT_PAGE_DB_OPS_BUDGET: usize = 5_000;

/// Relative db cost of a seek against an iterator advance: a seek descends the LSM stack
/// across candidate files while an advance stays within the loaded data block.
const SEEK_OP_COST: usize = 10;
const ADVANCE_OP_COST: usize = 1;

impl DbUtxoSetByCovenantIdStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy, spk_store: DbUtxoSetByScriptPublicKeyStore) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndexCovenant.into()),
            spk_store,
        }
    }

    /// Initializes (or re-initializes) the covenant index: rewrites every covenant row from the
    /// current script public key store contents, in batches.
    pub fn backfill_from_spk_store(&mut self) -> StoreResult<()> {
        let mut batch: Vec<(CovenantUtxoEntryDbKey, CovenantIndexEntry)> = Vec::with_capacity(COVENANT_BACKFILL_BATCH_SIZE);
        for res in self.spk_store.iterate_utxo_entries() {
            let (script_public_key, suffix, entry) = res?;
            if let Some(covenant_id) = entry.covenant_id {
                batch.push((
                    CovenantUtxoEntryDbKey::new(
                        covenant_id,
                        DaaScoreKey::from(suffix.daa_score()),
                        ScriptPublicKeyBucket::from(&script_public_key),
                        TransactionOutpointKey::from(suffix.transaction_outpoint()),
                    ),
                    CovenantIndexEntry,
                ));
                if batch.len() == COVENANT_BACKFILL_BATCH_SIZE {
                    let mut iter = batch.into_iter();
                    let mut db_batch = WriteBatch::default();
                    self.access.write_many_without_cache(BatchDbWriter::new(&mut db_batch), &mut iter)?;
                    self.db.write(db_batch)?;
                    batch = Vec::with_capacity(COVENANT_BACKFILL_BATCH_SIZE);
                }
            }
        }
        if !batch.is_empty() {
            let mut iter = batch.into_iter();
            let mut db_batch = WriteBatch::default();
            self.access.write_many_without_cache(BatchDbWriter::new(&mut db_batch), &mut iter)?;
            self.db.write(db_batch)?;
        }
        Ok(())
    }
}

impl DbUtxoSetByCovenantIdStore {
    /// Ordered rows of the requested covenant ids within the daa score range, traversed in
    /// ascending covenant id order, restricted to `spk_buckets` when non-empty, starting at
    /// `start` (a cursor position or the first id's prefix bottom).
    ///
    /// A single db raw iterator merge-joins the key stream against the bucket list,
    /// preferring seeks over scans: crossing into a new daa score rewinds to the first
    /// bucket, a key past the last bucket skips to the next daa score, and keys below the
    /// current bucket seek up to it. When a covenant id's pass ends, the iterator seeks the
    /// next requested id's prefix bottom. The iterator is bounded to the key space from the
    /// start key to the highest requested id's daa range end, so it never leaves the store's
    /// prefix.
    ///
    /// Seeks and advances consume weighted costs ([SEEK_OP_COST] and [ADVANCE_OP_COST])
    /// from `db_ops_budget`; once the budget is spent, the iterator yields the current
    /// position as [CovenantRow::BudgetExhausted] and ends. The initial positioning seek
    /// does not count.
    fn iterate_covenant_rows<'a>(
        &'a self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        spk_buckets: &'a [ScriptPublicKeyBucket],
        start: CovenantUtxoEntryDbKey,
        db_ops_budget: usize,
    ) -> impl Iterator<Item = StoreResult<CovenantRow>> + 'a {
        assert!(*daa_score_range.end() < u64::MAX);
        let prefix = self.access.prefix().to_vec();
        let from_daa_score = *daa_score_range.start();
        let to_daa_score = *daa_score_range.end();
        let max_id = *covenant_ids.last().expect("at least one covenant id");
        // the start key (cursor position or the first id's prefix bottom) is the lowest key of interest
        let lower = DbKey::new(&prefix, start);
        // the query entry caps the range end at u64::MAX - 1, keeping the + 1 overflow-free
        let upper = DbKey::new(&prefix, CovenantUtxoEntryDbKey::prefix_start(max_id, to_daa_score.saturating_add(1)));
        let mut read_opts = ReadOptions::default();
        read_opts.set_iterate_lower_bound(lower.as_ref());
        read_opts.set_iterate_upper_bound(upper.as_ref());
        let mut iter = self.db.raw_iterator_opt(read_opts);
        iter.seek_to_first();

        let mut pending_covenant_ids = covenant_ids.into_iter();
        let mut covenant_id = pending_covenant_ids.next().expect("at least one covenant id");
        let mut current_daa_score: Option<u64> = None;
        let mut bucket_index = 0usize;
        let mut done = false;
        let mut db_ops = 0usize;

        // the current covenant id's pass is over; continue with the next requested one
        macro_rules! seek_next_covenant_id {
            () => {
                match pending_covenant_ids.next() {
                    Some(next_id) => {
                        covenant_id = next_id;
                        current_daa_score = None;
                        bucket_index = 0;
                        db_ops += SEEK_OP_COST;
                        iter.seek(DbKey::new(&prefix, CovenantUtxoEntryDbKey::prefix_start(covenant_id, from_daa_score)).as_ref());
                        continue;
                    }
                    None => {
                        done = true;
                        return None;
                    }
                }
            };
        }

        std::iter::from_fn(move || {
            loop {
                if done {
                    return None;
                }
                if !iter.valid() {
                    if let Some(e) = iter.status().err() {
                        done = true;
                        return Some(Err(StoreError::DbError(e)));
                    }
                    // past the exclusive upper bound: requested ids sort ascending, so no
                    // remaining one can hold rows below it
                    done = true;
                    return None;
                }
                let raw_key = iter.key().expect("a valid iterator holds a key");
                let key = CovenantUtxoKeyView(&raw_key[prefix.len()..]);
                // a different covenant id ends the pass
                if key.covenant_id() != covenant_id.as_slice() {
                    seek_next_covenant_id!();
                }
                let daa_score = key.daa_score();
                if daa_score > to_daa_score {
                    seek_next_covenant_id!();
                }
                // the budget is spent at a validated position: hand it over as the resume point
                if db_ops >= db_ops_budget {
                    done = true;
                    let (covenant_id, script_public_key, suffix) = key.extract_data();
                    return Some(Ok(CovenantRow::BudgetExhausted(CovenantUtxoPageCursor::new(
                        covenant_id,
                        suffix.daa_score(),
                        script_public_key,
                        *suffix.transaction_outpoint(),
                    ))));
                }
                if current_daa_score != Some(daa_score) {
                    current_daa_score = Some(daa_score);
                    bucket_index = 0;
                    // rewind to the first bucket unless the stream is already past it
                    if let Some(first_bucket) = spk_buckets.first()
                        && key.spk_bucket() < first_bucket.as_ref()
                    {
                        db_ops += SEEK_OP_COST;
                        iter.seek(
                            DbKey::new(&prefix, CovenantUtxoEntryDbKey::spk_start(covenant_id, daa_score, first_bucket)).as_ref(),
                        );
                        continue;
                    }
                }
                if spk_buckets.is_empty() {
                    let (_, script_public_key, suffix) = key.extract_data();
                    db_ops += ADVANCE_OP_COST;
                    iter.next();
                    return Some(Ok(CovenantRow::Row(covenant_id, script_public_key, suffix)));
                }
                if bucket_index < spk_buckets.len() && spk_buckets[bucket_index].as_ref() < key.spk_bucket() {
                    bucket_index += spk_buckets[bucket_index..].partition_point(|bucket| bucket.as_ref() < key.spk_bucket());
                }
                let Some(bucket) = spk_buckets.get(bucket_index) else {
                    // past the last bucket, no match remains at this daa score: skip to the next one
                    db_ops += SEEK_OP_COST;
                    iter.seek(DbKey::new(&prefix, CovenantUtxoEntryDbKey::prefix_start(covenant_id, daa_score + 1)).as_ref());
                    continue;
                };
                if bucket.as_ref() != key.spk_bucket() {
                    // keys below the current bucket remain at this daa score: seek up to it
                    db_ops += SEEK_OP_COST;
                    iter.seek(DbKey::new(&prefix, CovenantUtxoEntryDbKey::spk_start(covenant_id, daa_score, bucket)).as_ref());
                    continue;
                }
                let (_, script_public_key, suffix) = key.extract_data();
                db_ops += ADVANCE_OP_COST;
                iter.next();
                return Some(Ok(CovenantRow::Row(covenant_id, script_public_key, suffix)));
            }
        })
    }
}

impl UtxoSetByCovenantIdStoreReader for DbUtxoSetByCovenantIdStore {
    fn get_utxos_by_covenant_ids_page(
        &self,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        cursor: Option<CovenantUtxoPageCursor>,
        budget: Option<usize>,
    ) -> UtxoIndexResult<OrderedCovenantUtxoEntriesPage> {
        // daa scores never reach u64::MAX in practice, so the effective range end is capped at
        // u64::MAX - 1; this keeps the + 1 of the exclusive upper iterator bound overflow-free
        let daa_score_range = *daa_score_range.start()..=(*daa_score_range.end()).min(u64::MAX - 1);

        // ids and buckets are seeked in ascending store key order
        let mut covenant_ids = covenant_ids;
        covenant_ids.sort_unstable();
        covenant_ids.dedup();
        let Some(first) = covenant_ids.first().copied() else {
            return Err(UtxoIndexError::QueryingEmptyCovenantIdSet);
        };
        // ids sorting before the cursor's id were consumed by earlier pages
        let (resume, start) = match cursor {
            Some(c) => {
                if !covenant_ids.contains(&c.covenant_id) || !daa_score_range.contains(&c.daa_score) {
                    return Err(UtxoIndexError::InvalidCovenantCursor(c));
                }
                let resume = covenant_ids.iter().position(|id| *id == c.covenant_id).expect("cursor id membership validated");
                (
                    resume,
                    CovenantUtxoEntryDbKey::new(
                        c.covenant_id,
                        DaaScoreKey::from(c.daa_score),
                        ScriptPublicKeyBucket::from(&c.script_public_key),
                        TransactionOutpointKey::from(&c.transaction_outpoint),
                    ),
                )
            }
            None => (0, CovenantUtxoEntryDbKey::prefix_start(first, *daa_score_range.start())),
        };
        let covenant_ids = covenant_ids.split_off(resume);

        let mut spk_buckets: Vec<ScriptPublicKeyBucket> = script_public_keys.iter().map(ScriptPublicKeyBucket::from).collect();
        spk_buckets.sort_unstable();
        spk_buckets.dedup();

        let db_ops_budget = budget.unwrap_or(DEFAULT_COVENANT_PAGE_DB_OPS_BUDGET);
        let mut rows: Vec<(Hash, ScriptPublicKey, UtxoEntryKeySuffixRecord, CompactUtxoEntry)> = Vec::new();
        let mut next_cursor = None;
        for res in self.iterate_covenant_rows(covenant_ids, daa_score_range, &spk_buckets, start, db_ops_budget) {
            match res? {
                CovenantRow::Row(covenant_id, script_public_key, suffix) => {
                    let entry = self.spk_store.get_compact_utxo_entry(
                        &script_public_key,
                        suffix.daa_score(),
                        suffix.transaction_outpoint(),
                    )?;
                    debug_assert_eq!(entry.covenant_id, Some(covenant_id));
                    rows.push((covenant_id, script_public_key, suffix, entry));
                }
                CovenantRow::BudgetExhausted(cursor) => {
                    next_cursor = Some(cursor);
                    break;
                }
            }
        }

        Ok(OrderedCovenantUtxoEntriesPage::new(Arc::new(rows), next_cursor))
    }
}

impl UtxoSetByCovenantIdStore for DbUtxoSetByCovenantIdStore {
    fn remove_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

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
        self.db.write(batch)?;

        Ok(())
    }

    fn add_utxo_entries(&mut self, utxo_entries: &UtxoSetByCovenantId) -> StoreResult<()> {
        if utxo_entries.is_empty() {
            return Ok(());
        }

        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

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
        self.db.write(batch)?;

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
    use kaspa_consensus_core::HashMapCustomHasher;
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
        let (parsed_id, parsed_script_public_key, suffix) = CovenantUtxoKeyView(key.as_ref()).extract_data();
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
                let key_bytes = res.unwrap().0;
                let (id, script_public_key, suffix) = CovenantUtxoKeyView(key_bytes.as_ref()).extract_data();
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
    fn test_covenant_prefix_start_bounds() {
        let id = Hash::from_slice(&[3u8; HASH_SIZE]);
        let full_key = |daa: u64, script: Vec<u8>, outpoint: TransactionOutpoint| {
            CovenantUtxoEntryDbKey::new(
                id,
                daa.into(),
                ScriptPublicKeyBucket::from(&ScriptPublicKey::from_vec(0, script)),
                TransactionOutpointKey::from(&outpoint),
            )
        };
        // a start-of-(id, daa) key sorts before every full key with that id and daa, and past the previous daa
        assert!(CovenantUtxoEntryDbKey::prefix_start(id, 7) < full_key(7, vec![0x00], create_outpoint(0, 0)));
        assert!(CovenantUtxoEntryDbKey::prefix_start(id, 7) > full_key(6, vec![0xFF], create_outpoint(u64::MAX, u32::MAX)));
    }

    use crate::core::model::{CompactUtxoEntry, CovenantUtxoPageCursor, UtxoSetByScriptPublicKey};
    use crate::errors::UtxoIndexError;
    use crate::stores::script_public_keys::UtxoSetByScriptPublicKeyStore as _;
    use std::ops::RangeInclusive;

    type PopulateRow = (Hash, u64, ScriptPublicKey, TransactionOutpoint, u64);

    fn populate(
        spk_store: &mut DbUtxoSetByScriptPublicKeyStore,
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
        spk_store.add_utxo_entries(&address).unwrap();
        covenant_store.add_utxo_entries(&covenant).unwrap();
    }

    fn page_entries(
        store: &DbUtxoSetByCovenantIdStore,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        cursor: Option<CovenantUtxoPageCursor>,
        budget: Option<usize>,
    ) -> (Vec<(Hash, u64)>, Option<CovenantUtxoPageCursor>) {
        let page = store.get_utxos_by_covenant_ids_page(covenant_ids, daa_score_range, script_public_keys, cursor, budget).unwrap();
        let rows = page.entries().iter().map(|(covenant_id, _, suffix, _)| (*covenant_id, suffix.daa_score())).collect();
        (rows, page.next_cursor().cloned())
    }

    /// Pages a query until the cursor runs out, returning the daa scores of each page's rows.
    fn drain_pages(
        store: &DbUtxoSetByCovenantIdStore,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        budget: Option<usize>,
    ) -> Vec<Vec<u64>> {
        let mut cursor = None;
        let mut pages = vec![];
        loop {
            let (rows, next) = page_entries(store, covenant_ids.clone(), daa_score_range.clone(), script_public_keys, cursor, budget);
            pages.push(rows.iter().map(|(_, daa)| *daa).collect());
            cursor = match next {
                Some(c) => Some(c),
                None => break,
            };
        }
        pages
    }

    #[test]
    fn test_page_ordering_within_id_and_ascending_id_order() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[2u8; HASH_SIZE]);
        let script_x = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_y = ScriptPublicKey::from_vec(0, vec![0x02]);

        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_b, 5, script_x.clone(), create_outpoint(1, 0), 500),
                (id_b, 3, script_y.clone(), create_outpoint(2, 0), 300),
                (id_a, 10, script_x.clone(), create_outpoint(3, 0), 1000),
            ],
        );

        // ascending id order regardless of request order; rows within an id sort by daa
        let (rows, next_cursor) = page_entries(&store, vec![id_b, id_a], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_a, 10), (id_b, 3), (id_b, 5)]);
        assert!(next_cursor.is_none());

        // amounts resolve from the script public key store
        let page = store.get_utxos_by_covenant_ids_page(vec![id_b], 0..=u64::MAX, &[], None, None).unwrap();
        let amounts: Vec<u64> = page.entries().iter().map(|(_, _, _, entry)| entry.amount).collect();
        assert_eq!(amounts, vec![300, 500]);
    }

    #[test]
    fn test_page_db_ops_budget_and_next_cursor() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=5u64).map(|daa| (id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa * 100)).collect(),
        );

        // a budget of 2 ops yields 2 unfiltered rows, one advance each
        let (page1, cursor1) = page_entries(&store, vec![id], 0..=u64::MAX, &[], None, Some(2));
        assert_eq!(page1, vec![(id, 1), (id, 2)]);
        let cursor1 = cursor1.unwrap();
        assert_eq!((cursor1.covenant_id, cursor1.daa_score), (id, 3));

        let (page2, cursor2) = page_entries(&store, vec![id], 0..=u64::MAX, &[], Some(cursor1), Some(2));
        assert_eq!(page2, vec![(id, 3), (id, 4)]);
        let cursor2 = cursor2.unwrap();
        assert_eq!(cursor2.daa_score, 5);

        let (page3, cursor3) = page_entries(&store, vec![id], 0..=u64::MAX, &[], Some(cursor2), Some(2));
        assert_eq!(page3, vec![(id, 5)]);
        assert!(cursor3.is_none());
    }

    #[test]
    fn test_page_db_ops_budget_drains_exactly_once() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=50u64).map(|daa| (id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa)).collect(),
        );

        // a budget of 7 ops returns 7 unfiltered rows per page; draining collects every row once
        let pages = drain_pages(&store, vec![id], 0..=u64::MAX, &[], Some(7));
        let collected: Vec<u64> = pages.iter().flatten().copied().collect();
        assert_eq!(collected, (1..=50u64).collect::<Vec<_>>());
        assert_eq!(pages.iter().map(Vec::len).collect::<Vec<_>>(), vec![7, 7, 7, 7, 7, 7, 7, 1]);
    }

    #[test]
    fn test_page_db_ops_budget_empty_pages_advance_cursor() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_low = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_high = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=9u64)
                .map(|daa| {
                    let script = if daa % 4 == 1 { script_high.clone() } else { script_low.clone() };
                    (id, daa, script, create_outpoint(daa, 0), daa)
                })
                .collect(),
        );

        // a budget of 1 op cannot both seek past the low rows and return a row: pages come
        // back empty with cursors advancing to the next high row
        let pages = drain_pages(&store, vec![id], 0..=u64::MAX, std::slice::from_ref(&script_high), Some(1));
        assert_eq!(pages, vec![vec![1], vec![], vec![], vec![], vec![5], vec![], vec![], vec![], vec![9]]);
    }

    #[test]
    fn test_covenant_iterator_budget_signal() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_low = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_high = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=7u64)
                .map(|daa| {
                    let script = if daa % 2 == 0 { script_high.clone() } else { script_low.clone() };
                    (id, daa, script, create_outpoint(daa, 0), daa)
                })
                .collect(),
        );

        // one advance per unfiltered row: budget 4 stops with a cursor at the fifth row
        let mut iter = store.iterate_covenant_rows(vec![id], 0..=(u64::MAX - 1), &[], CovenantUtxoEntryDbKey::prefix_start(id, 0), 4);
        let mut daa_scores = vec![];
        loop {
            match iter.next().unwrap().unwrap() {
                CovenantRow::Row(_, _, suffix) => daa_scores.push(suffix.daa_score()),
                CovenantRow::BudgetExhausted(cursor) => {
                    assert_eq!((cursor.covenant_id, cursor.daa_score), (id, 5));
                    break;
                }
            }
        }
        assert_eq!(daa_scores, vec![1, 2, 3, 4]);
        assert!(iter.next().is_none());

        // a seek costs 10 units: budget 20 covers one seek-plus-advance pair and ends
        // mid-gap with a cursor at the next candidate
        let high_buckets = [ScriptPublicKeyBucket::from(&script_high)];
        let mut iter =
            store.iterate_covenant_rows(vec![id], 0..=(u64::MAX - 1), &high_buckets, CovenantUtxoEntryDbKey::prefix_start(id, 0), 20);
        let mut daa_scores = vec![];
        loop {
            match iter.next().unwrap().unwrap() {
                CovenantRow::Row(_, _, suffix) => daa_scores.push(suffix.daa_score()),
                CovenantRow::BudgetExhausted(cursor) => {
                    assert_eq!((cursor.covenant_id, cursor.daa_score), (id, 4));
                    break;
                }
            }
        }
        assert_eq!(daa_scores, vec![2]);
        assert!(iter.next().is_none());
    }

    #[test]
    fn test_page_daa_bounds_and_spk_list_filtering() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_low = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_high = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=7u64)
                .map(|daa| {
                    let script = if daa % 2 == 0 { script_high.clone() } else { script_low.clone() };
                    (id, daa, script, create_outpoint(daa, 0), daa)
                })
                .collect(),
        );

        // daa bounds exclude the outer rows
        let (rows, _) = page_entries(&store, vec![id], 2..=6, &[], None, None);
        assert_eq!(rows, vec![(id, 2), (id, 3), (id, 4), (id, 5), (id, 6)]);

        // an spk list keeps only the matching rows; seeks past non-matching rows consume
        // budget, yielding empty pages whose cursors advance to the next candidate
        let pages = drain_pages(&store, vec![id], 0..=u64::MAX, std::slice::from_ref(&script_high), Some(1));
        assert_eq!(pages, vec![vec![], vec![2], vec![], vec![4], vec![], vec![6], vec![]]);
    }

    #[test]
    fn test_page_cursor_validation() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let other_id = Hash::from_slice(&[9u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(&mut spk_store, &mut store, vec![(id, 1, script_public_key.clone(), create_outpoint(1, 0), 100)]);

        let out_of_range = CovenantUtxoPageCursor::new(id, 99, script_public_key.clone(), create_outpoint(1, 0));
        let result = store.get_utxos_by_covenant_ids_page(vec![id], 0..=10, &[], Some(out_of_range), None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::InvalidCovenantCursor(_)));

        let wrong_id = CovenantUtxoPageCursor::new(other_id, 1, script_public_key, create_outpoint(1, 0));
        let result = store.get_utxos_by_covenant_ids_page(vec![id], 0..=10, &[], Some(wrong_id), None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::InvalidCovenantCursor(_)));

        let result = store.get_utxos_by_covenant_ids_page(vec![], 0..=10, &[], None, None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::QueryingEmptyCovenantIdSet));
    }

    #[test]
    fn test_page_spk_list_follows_bucket_ordering() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        // the bucket length field is little endian, so a 256-byte script sorts before a 255-byte one
        let script_255 = ScriptPublicKey::from_vec(0, vec![0xAA; 255]);
        let script_256 = ScriptPublicKey::from_vec(0, vec![0xAA; 256]);
        populate(
            &mut spk_store,
            &mut store,
            vec![(id, 1, script_255.clone(), create_outpoint(1, 0), 100), (id, 2, script_256, create_outpoint(2, 0), 200)],
        );

        // both rows exist unfiltered
        let (rows, _) = page_entries(&store, vec![id], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id, 1), (id, 2)]);

        // a list of one script returns only its rows, regardless of request order in the list
        let (rows, _) = page_entries(&store, vec![id], 0..=u64::MAX, std::slice::from_ref(&script_255), None, None);
        assert_eq!(rows, vec![(id, 1)]);
    }

    #[test]
    fn test_all_0xff_covenant_id_queries() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[0xFFu8; HASH_SIZE]);
        let other_id = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id, u64::MAX / 2, script_public_key.clone(), create_outpoint(2, 0), 200),
                (other_id, 5, script_public_key.clone(), create_outpoint(3, 0), 300),
            ],
        );

        // open daa bound under the maximum id, requested ahead of a lexicographically smaller id
        let (rows, next_cursor) = page_entries(&store, vec![id, other_id], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(other_id, 5), (id, 1), (id, u64::MAX / 2)]);
        assert!(next_cursor.is_none());

        // pagination resumes under the maximum id
        let (page1, cursor1) = page_entries(&store, vec![id], 0..=u64::MAX, &[], None, Some(1));
        assert_eq!(page1, vec![(id, 1)]);
        let cursor1 = cursor1.unwrap();
        assert_eq!(cursor1.covenant_id, id);
        let (page2, next2) = page_entries(&store, vec![id], 0..=u64::MAX, &[], Some(cursor1), Some(1));
        assert_eq!(page2, vec![(id, u64::MAX / 2)]);
        assert!(next2.is_none());
    }

    type PageRow = (Hash, u64, Vec<u8>);

    fn page_rows(
        store: &DbUtxoSetByCovenantIdStore,
        covenant_ids: Vec<Hash>,
        daa_score_range: RangeInclusive<u64>,
        script_public_keys: &[ScriptPublicKey],
        cursor: Option<CovenantUtxoPageCursor>,
        budget: Option<usize>,
    ) -> (Vec<PageRow>, Option<CovenantUtxoPageCursor>) {
        let page = store.get_utxos_by_covenant_ids_page(covenant_ids, daa_score_range, script_public_keys, cursor, budget).unwrap();
        let rows = page
            .entries()
            .iter()
            .map(|(covenant_id, script_public_key, suffix, _)| (*covenant_id, suffix.daa_score(), script_public_key.script().to_vec()))
            .collect();
        (rows, page.next_cursor().cloned())
    }

    #[test]
    fn test_page_multi_spk_list_pairs_every_daa_with_each_listed_spk() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_n = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x03]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_n.clone(), create_outpoint(2, 0), 200),
                (id, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id, 1, script_a.clone(), create_outpoint(3, 0), 101),
                (id, 1, script_b.clone(), create_outpoint(4, 0), 400),
                (id, 2, script_a.clone(), create_outpoint(5, 0), 500),
                (id, 2, script_b.clone(), create_outpoint(6, 0), 600),
                (id, 3, script_a.clone(), create_outpoint(7, 0), 700),
            ],
        );

        // every listed (daa, spk) pair in daa-then-spk order, non-listed rows absent
        let (rows, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_a, script_b], None, None);
        assert_eq!(
            rows,
            vec![
                (id, 1, vec![0x01]),
                (id, 1, vec![0x01]),
                (id, 1, vec![0x03]),
                (id, 2, vec![0x01]),
                (id, 2, vec![0x03]),
                (id, 3, vec![0x01]),
            ]
        );
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_skips_daa_scores_without_any_listed_spk() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_c = ScriptPublicKey::from_vec(0, vec![0x03]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id, 2, script_b.clone(), create_outpoint(2, 0), 200),
                (id, 3, script_c.clone(), create_outpoint(3, 0), 300),
                (id, 4, script_a.clone(), create_outpoint(4, 0), 400),
                (id, 4, script_c.clone(), create_outpoint(5, 0), 500),
                (id, 5, script_b.clone(), create_outpoint(6, 0), 600),
            ],
        );

        // daa scores 1, 3 and 4 hold no listed spk, matches at 2 and 5 survive
        let (rows, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_b], None, None);
        assert_eq!(rows, vec![(id, 2, vec![0x02]), (id, 5, vec![0x02])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_skips_daa_scores_with_rows_only_past_last_listed_spk() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_m = ScriptPublicKey::from_vec(0, vec![0x05]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_m.clone(), create_outpoint(1, 0), 100),
                (id, 1, script_m.clone(), create_outpoint(2, 0), 200),
                (id, 2, script_a.clone(), create_outpoint(3, 0), 300),
                (id, 3, script_m.clone(), create_outpoint(4, 0), 400),
                (id, 4, script_a.clone(), create_outpoint(5, 0), 500),
            ],
        );

        // daa scores 1 and 3 hold only rows past the last listed spk, later matches returned
        let (rows, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_a], None, None);
        assert_eq!(rows, vec![(id, 2, vec![0x01]), (id, 4, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_seeks_up_when_daa_starts_below_first_listed_spk() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_c = ScriptPublicKey::from_vec(0, vec![0x03]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id, 2, script_b.clone(), create_outpoint(2, 0), 200),
                (id, 3, script_a.clone(), create_outpoint(3, 0), 300),
                (id, 3, script_c.clone(), create_outpoint(4, 0), 400),
                (id, 4, script_c.clone(), create_outpoint(5, 0), 500),
                (id, 5, script_c.clone(), create_outpoint(6, 0), 600),
            ],
        );

        // daa scores 1 and 3 start below the first listed spk, listed rows at and after them survive
        let (rows, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_b, script_c], None, None);
        assert_eq!(rows, vec![(id, 2, vec![0x02]), (id, 3, vec![0x03]), (id, 4, vec![0x03]), (id, 5, vec![0x03])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_spk_filtered_pagination_concatenates_to_single_query() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_n = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x03]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id, 1, script_a.clone(), create_outpoint(2, 0), 200),
                (id, 1, script_a.clone(), create_outpoint(3, 0), 300),
                (id, 1, script_n.clone(), create_outpoint(4, 0), 400),
                (id, 1, script_b.clone(), create_outpoint(5, 0), 500),
                (id, 1, script_b.clone(), create_outpoint(6, 0), 600),
                (id, 2, script_n.clone(), create_outpoint(7, 0), 700),
                (id, 2, script_a.clone(), create_outpoint(8, 0), 800),
                (id, 3, script_b.clone(), create_outpoint(9, 0), 900),
            ],
        );

        let (full, _) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_a.clone(), script_b.clone()], None, None);
        assert_eq!(full.len(), 7);

        // budget-2 pages concatenate to the unpaginated query, no duplicates, no skips
        let mut collected = vec![];
        let mut cursor = None;
        let mut page_count = 0;
        loop {
            let (rows, next) = page_rows(&store, vec![id], 0..=u64::MAX, &[script_a.clone(), script_b.clone()], cursor, Some(2));
            collected.extend(rows);
            page_count += 1;
            cursor = match next {
                Some(c) => Some(c),
                None => break,
            };
        }
        assert_eq!(collected, full);
        assert!(page_count > 1);
    }

    #[test]
    fn test_page_duplicate_unordered_spk_list_matches_sorted_deduped() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_n = ScriptPublicKey::from_vec(0, vec![0x02]);
        let script_b = ScriptPublicKey::from_vec(0, vec![0x03]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id, 1, script_n.clone(), create_outpoint(2, 0), 200),
                (id, 1, script_b.clone(), create_outpoint(3, 0), 300),
                (id, 2, script_b.clone(), create_outpoint(4, 0), 400),
                (id, 3, script_a.clone(), create_outpoint(5, 0), 500),
            ],
        );

        // duplicate unordered list matches the sorted deduped list
        let deduped = page_rows(&store, vec![id], 0..=u64::MAX, &[script_a.clone(), script_b.clone()], None, None).0;
        let messy = page_rows(
            &store,
            vec![id],
            0..=u64::MAX,
            &[script_b.clone(), script_a.clone(), script_b, script_a.clone(), script_a],
            None,
            None,
        )
        .0;
        assert_eq!(messy, deduped);
        assert_eq!(deduped.len(), 4);
    }

    #[test]
    fn test_page_empty_spk_list_multi_id_full_rows_in_ascending_id_order() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[2u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_n = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id_a, 2, script_n.clone(), create_outpoint(2, 0), 200),
                (id_a, 5, script_n.clone(), create_outpoint(3, 0), 300),
                (id_b, 3, script_a.clone(), create_outpoint(4, 0), 400),
                (id_b, 4, script_n.clone(), create_outpoint(5, 0), 500),
            ],
        );

        // every spk, ascending id order regardless of request order, daa range 0..=4 excludes the daa 5 row
        let (rows, next_cursor) = page_rows(&store, vec![id_b, id_a], 0..=4, &[], None, None);
        assert_eq!(rows, vec![(id_a, 1, vec![0x01]), (id_a, 2, vec![0x02]), (id_b, 3, vec![0x01]), (id_b, 4, vec![0x02])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_stops_at_daa_range_end_with_rows_beyond() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[1u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=5u64).map(|daa| (id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa * 100)).collect(),
        );

        // rows at daa 4 and 5 exist past the range end and are excluded
        let (rows, next_cursor) = page_rows(&store, vec![id], 1..=3, &[], None, None);
        assert_eq!(rows, vec![(id, 1, vec![0x01]), (id, 2, vec![0x01]), (id, 3, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_budget_on_id_boundary_resumes_at_next_covenant_id() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[1u8; HASH_SIZE]);
        // keys of id_b directly follow id_a, ending the id_a pass by the covenant id check
        let mut id_b_bytes = [1u8; HASH_SIZE];
        id_b_bytes[HASH_SIZE - 1] = 2;
        let id_b = Hash::from_slice(&id_b_bytes);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_a, 2, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_b, 3, script_public_key.clone(), create_outpoint(3, 0), 300),
                (id_b, 4, script_public_key.clone(), create_outpoint(4, 0), 400),
            ],
        );

        let (full, _) = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], None, None);
        assert_eq!(full.len(), 4);

        // the budget ends past the id_a boundary, orienting the cursor at id_b
        let (page1, cursor1) = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], None, Some(2));
        assert_eq!(page1, vec![(id_a, 1, vec![0x01]), (id_a, 2, vec![0x01])]);
        let cursor1 = cursor1.unwrap();
        assert_eq!((cursor1.covenant_id, cursor1.daa_score), (id_b, 3));

        // resume with the id list rebased at the cursor id, boundary row returned once in total
        let (page2, cursor2) = page_rows(&store, vec![id_b], 0..=u64::MAX, &[], Some(cursor1), None);
        assert_eq!(page2, vec![(id_b, 3, vec![0x01]), (id_b, 4, vec![0x01])]);
        assert!(cursor2.is_none());

        let mut concatenated = page1;
        concatenated.extend(page2);
        assert_eq!(concatenated, full);
    }

    #[test]
    fn test_page_bucket_exhaustion_at_max_daa_score_terminates() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_1 = Hash::from_slice(&[5u8; HASH_SIZE]);
        let id_2 = Hash::from_slice(&[6u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_m = ScriptPublicKey::from_vec(0, vec![0x05]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_1, u64::MAX - 1, script_m.clone(), create_outpoint(1, 0), 100),
                (id_2, u64::MAX - 1, script_a.clone(), create_outpoint(2, 0), 200),
            ],
        );

        // the id_1 pass exhausts the list at daa u64::MAX - 1 and stops, the id_2 row at u64::MAX - 1 is in range
        let (rows, next_cursor) = page_rows(&store, vec![id_1, id_2], 0..=u64::MAX, &[script_a], None, None);
        assert_eq!(rows, vec![(id_2, u64::MAX - 1, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_unsorted_duplicated_ids_match_sorted_deduped() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_a, 2, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_b, 3, script_public_key, create_outpoint(3, 0), 300),
            ],
        );

        // request [id_b, id_a, id_b] returns the sorted deduped [id_a, id_b] rows
        let sorted = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], None, None).0;
        let messy = page_rows(&store, vec![id_b, id_a, id_b], 0..=u64::MAX, &[], None, None).0;
        assert_eq!(messy, vec![(id_a, 1, vec![0x01]), (id_a, 2, vec![0x01]), (id_b, 3, vec![0x01])]);
        assert_eq!(messy, sorted);
    }

    #[test]
    fn test_page_untrimmed_id_list_resume_concatenates() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let id_c = Hash::from_slice(&[0x03u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=6u64)
                .map(|daa| {
                    let covenant_id = if daa <= 2 {
                        id_a
                    } else if daa <= 4 {
                        id_b
                    } else {
                        id_c
                    };
                    (covenant_id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa * 100)
                })
                .collect(),
        );

        // budget-2 pages resubmitted with the full original unsorted list concatenate to the single query
        let original_request = vec![id_c, id_a, id_b];
        let (full, _) = page_rows(&store, original_request.clone(), 0..=u64::MAX, &[], None, None);
        assert_eq!(full.len(), 6);

        let mut collected = vec![];
        let mut cursor_ids = vec![];
        let mut cursor = None;
        loop {
            let (rows, next) = page_rows(&store, original_request.clone(), 0..=u64::MAX, &[], cursor, Some(2));
            collected.extend(rows);
            match &next {
                Some(c) => cursor_ids.push(c.covenant_id),
                None => break,
            }
            cursor = next;
        }
        assert_eq!(collected, full);
        assert_eq!(cursor_ids, vec![id_b, id_c]);
    }

    #[test]
    fn test_page_cursor_at_last_id_mid_daa_spk_filter_returns_tail() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let script_x = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_y = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_x.clone(), create_outpoint(1, 0), 100),
                (id_b, 1, script_x.clone(), create_outpoint(9, 0), 900),
                (id_b, 2, script_x.clone(), create_outpoint(10, 0), 1000),
                (id_b, 2, script_x.clone(), create_outpoint(11, 0), 1100),
                (id_b, 2, script_y, create_outpoint(13, 0), 1300),
                (id_b, 3, script_x.clone(), create_outpoint(12, 0), 1200),
            ],
        );

        // resume at the last sorted id, mid-daa 2, keeps only the filtered id_b tail
        let cursor = CovenantUtxoPageCursor::new(id_b, 2, script_x.clone(), create_outpoint(10, 0));
        let (rows, next_cursor) = page_entries(&store, vec![id_a, id_b], 0..=u64::MAX, &[script_x], Some(cursor), None);
        assert_eq!(rows, vec![(id_b, 2), (id_b, 2), (id_b, 3)]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_non_requested_id_between_requested_ids_is_skipped() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_m = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x03u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_m, 4, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_m, 5, script_public_key.clone(), create_outpoint(3, 0), 300),
                (id_b, 9, script_public_key, create_outpoint(4, 0), 400),
            ],
        );

        // ids sorting between requested ones are skipped, both requested ids fully returned
        let (rows, next_cursor) = page_rows(&store, vec![id_b, id_a], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_a, 1, vec![0x01]), (id_b, 9, vec![0x01])]);
        assert!(next_cursor.is_none());

        // the skipped id's rows exist and surface once it is requested
        let (all_rows, _) = page_rows(&store, vec![id_a, id_m, id_b], 0..=u64::MAX, &[], None, None);
        assert_eq!(all_rows.len(), 4);
    }

    #[test]
    fn test_page_store_end_pass_with_pending_id_continues() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_z = Hash::from_slice(&[0x05u8; HASH_SIZE]);
        let id_ghost = Hash::from_slice(&[0x07u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_z, 5, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_z, 6, script_public_key, create_outpoint(3, 0), 300),
            ],
        );

        // the largest store id requested first ends at store end, remaining ids still contribute
        let (rows, next_cursor) = page_rows(&store, vec![id_z, id_a, id_ghost], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_a, 1, vec![0x01]), (id_z, 5, vec![0x01]), (id_z, 6, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_non_existent_id_gap_seek_through_concatenates() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let id_c = Hash::from_slice(&[0x03u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_a, 2, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_c, 5, script_public_key.clone(), create_outpoint(3, 0), 300),
                (id_c, 6, script_public_key, create_outpoint(4, 0), 400),
            ],
        );

        // the id without rows contributes nothing, its neighbors are returned
        let request = vec![id_a, id_b, id_c];
        let (rows, _) = page_entries(&store, request.clone(), 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_a, 1), (id_a, 2), (id_c, 5), (id_c, 6)]);

        // budget-2 pages across the gap concatenate to the single query
        let (full, _) = page_rows(&store, request.clone(), 0..=u64::MAX, &[], None, None);
        assert_eq!(full.len(), 4);
        let mut collected = vec![];
        let mut cursor = None;
        loop {
            let (rows, next) = page_rows(&store, request.clone(), 0..=u64::MAX, &[], cursor, Some(2));
            collected.extend(rows);
            cursor = match next {
                Some(c) => Some(c),
                None => break,
            };
        }
        assert_eq!(collected, full);
    }

    #[test]
    fn test_page_budget_spans_multiple_id_boundaries() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let id_c = Hash::from_slice(&[0x03u8; HASH_SIZE]);
        let id_d = Hash::from_slice(&[0x04u8; HASH_SIZE]);
        let script_1 = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_2 = ScriptPublicKey::from_vec(0, vec![0x02]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_1.clone(), create_outpoint(1, 0), 100),
                (id_b, 2, script_2.clone(), create_outpoint(2, 0), 200),
                (id_c, 3, script_1, create_outpoint(3, 0), 300),
                (id_d, 4, script_2.clone(), create_outpoint(4, 0), 400),
            ],
        );

        // a budget-25 page holds three ids' rows: three advances plus two id-boundary
        // seeks at 10 units each; the cursor orients at the fourth id's row
        let (rows, next_cursor) = page_rows(&store, vec![id_d, id_b, id_a, id_c], 0..=u64::MAX, &[], None, Some(25));
        assert_eq!(rows, vec![(id_a, 1, vec![0x01]), (id_b, 2, vec![0x02]), (id_c, 3, vec![0x01])]);
        let cursor = next_cursor.unwrap();
        assert_eq!((cursor.covenant_id, cursor.daa_score), (id_d, 4));
        assert_eq!(cursor.script_public_key, script_2);

        let (tail, next_cursor) = page_rows(&store, vec![id_d, id_b, id_a, id_c], 0..=u64::MAX, &[], Some(cursor), None);
        assert_eq!(tail, vec![(id_d, 4, vec![0x02])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_max_daa_bucket_exhaustion_after_match_continues() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_1 = Hash::from_slice(&[0x05u8; HASH_SIZE]);
        let id_2 = Hash::from_slice(&[0x06u8; HASH_SIZE]);
        let script_a = ScriptPublicKey::from_vec(0, vec![0x01]);
        let script_m = ScriptPublicKey::from_vec(0, vec![0x05]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_1, 1, script_a.clone(), create_outpoint(1, 0), 100),
                (id_1, u64::MAX - 1, script_m, create_outpoint(2, 0), 200),
                (id_2, 2, script_a.clone(), create_outpoint(3, 0), 300),
            ],
        );

        // a matched row then a past-list row at daa u64::MAX - 1 end the id_1 pass, id_2 still returns
        let (rows, next_cursor) = page_rows(&store, vec![id_2, id_1], 0..=u64::MAX, std::slice::from_ref(&script_a), None, None);
        assert_eq!(rows, vec![(id_1, 1, vec![0x01]), (id_2, 2, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_empty_store_with_foreign_prefix_keys() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let store =
            DbUtxoSetByCovenantIdStore::new(db.clone(), CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        // no covenant rows exist; the first seek lands directly in foreign prefix space
        db.put(vec![197u8], vec![]).unwrap();
        db.put(vec![197u8, 0xDE, 0xAD], vec![0xC0]).unwrap();
        let id = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let (rows, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_foreign_prefix_keys_end_pass_and_pending_ids_continue() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db.clone(), CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_z = Hash::from_slice(&[0x05u8; HASH_SIZE]);
        let id_ghost = Hash::from_slice(&[0x07u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_z, 5, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_z, 6, script_public_key, create_outpoint(2, 0), 200),
            ],
        );

        // the id_z pass and the ghost id's seek both end on foreign prefix keys
        db.put(vec![197u8], vec![]).unwrap();
        db.put(vec![197u8, 0xDE, 0xAD], vec![0xC0]).unwrap();
        let (rows, next_cursor) = page_rows(&store, vec![id_z, id_ghost], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_z, 5, vec![0x01]), (id_z, 6, vec![0x01])]);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_keys_of_later_store_prefix_are_ignored() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db.clone(), CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_a, 1, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_a, 2, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_b, 3, script_public_key, create_outpoint(3, 0), 300),
            ],
        );

        // raw keys of a store prefix sorting after the covenant prefix return exactly the covenant rows
        db.put(vec![197u8], vec![]).unwrap();
        db.put(vec![197u8, 0xDE, 0xAD], vec![0xC0]).unwrap();
        let (full, next_cursor) = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], None, None);
        assert_eq!(full, vec![(id_a, 1, vec![0x01]), (id_a, 2, vec![0x01]), (id_b, 3, vec![0x01])]);
        assert!(next_cursor.is_none());

        // pagination crossing the covenant-to-foreign boundary concatenates to the single query
        let (page1, cursor1) = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], None, Some(2));
        assert_eq!(page1, vec![(id_a, 1, vec![0x01]), (id_a, 2, vec![0x01])]);
        let (page2, next_cursor) = page_rows(&store, vec![id_a, id_b], 0..=u64::MAX, &[], Some(cursor1.unwrap()), None);
        let mut concatenated = page1;
        concatenated.extend(page2);
        assert_eq!(concatenated, full);
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_keys_of_neighbour_prefixes_stay_out_of_bounds() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store =
            DbUtxoSetByCovenantIdStore::new(db.clone(), CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_min = Hash::from_slice(&[0x00u8; HASH_SIZE]);
        let id_max = Hash::from_slice(&[0xFFu8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id_min, 0, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id_max, 1, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id_max, u64::MAX - 1, script_public_key.clone(), create_outpoint(3, 0), 300),
            ],
        );

        // keys under the adjacent store prefixes (195 sorts before the covenant prefix 196, 197
        // after it) and the bare covenant prefix itself, all sorting directly around the range
        db.put([195u8].into_iter().chain([0xFF; 40]).collect::<Vec<_>>(), vec![]).unwrap();
        db.put(vec![196u8], vec![]).unwrap();
        db.put(vec![197u8], vec![]).unwrap();
        db.put(vec![197u8, 0x00], vec![]).unwrap();

        let (rows, next_cursor) = page_rows(&store, vec![id_max, id_min], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_min, 0, vec![0x01]), (id_max, 1, vec![0x01]), (id_max, u64::MAX - 1, vec![0x01])]);
        assert!(next_cursor.is_none());

        // a single all-0x00 id spans its own bounds from daa 0
        let (rows, _) = page_rows(&store, vec![id_min], 0..=u64::MAX, &[], None, None);
        assert_eq!(rows, vec![(id_min, 0, vec![0x01])]);
    }

    #[test]
    fn test_page_daa_range_edges_are_inclusive_for_min_mid_max_ids() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id_a = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let id_m = Hash::from_slice(&[0x02u8; HASH_SIZE]);
        let id_b = Hash::from_slice(&[0x03u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        let mut to_populate = vec![];
        for (i, id) in [id_a, id_m, id_b].into_iter().enumerate() {
            for daa in [1u64, 2, 5, 6] {
                to_populate.push((id, daa, script_public_key.clone(), create_outpoint(i as u64 * 10 + daa, 0), daa));
            }
        }
        populate(&mut spk_store, &mut store, to_populate);

        // rows exactly at from_daa 2 and to_daa 5 return, the one-off neighbours 1 and 6 do not
        let (rows, next_cursor) = page_rows(&store, vec![id_b, id_a, id_m], 2..=5, &[], None, None);
        assert_eq!(
            rows,
            vec![
                (id_a, 2, vec![0x01]),
                (id_a, 5, vec![0x01]),
                (id_m, 2, vec![0x01]),
                (id_m, 5, vec![0x01]),
                (id_b, 2, vec![0x01]),
                (id_b, 5, vec![0x01]),
            ]
        );
        assert!(next_cursor.is_none());
    }

    #[test]
    fn test_page_open_daa_end_caps_at_max_minus_one() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            vec![
                (id, u64::MAX - 2, script_public_key.clone(), create_outpoint(1, 0), 100),
                (id, u64::MAX - 1, script_public_key.clone(), create_outpoint(2, 0), 200),
                (id, u64::MAX, script_public_key.clone(), create_outpoint(3, 0), 300),
            ],
        );

        // the range end u64::MAX (the omitted-end default) caps at u64::MAX - 1: the top two
        // rows return, a row at u64::MAX does not
        let (open, next_cursor) = page_rows(&store, vec![id], 0..=u64::MAX, &[], None, None);
        assert_eq!(open, vec![(id, u64::MAX - 2, vec![0x01]), (id, u64::MAX - 1, vec![0x01])]);
        assert!(next_cursor.is_none());
        let (explicit, _) = page_rows(&store, vec![id], 0..=u64::MAX - 1, &[], None, None);
        assert_eq!(explicit, open);

        // budget-1 pages resume through the capped boundary
        let mut collected = vec![];
        let mut cursor = None;
        loop {
            let (rows, next) = page_rows(&store, vec![id], 0..=u64::MAX, &[], cursor, Some(1));
            collected.extend(rows);
            cursor = match next {
                Some(c) => Some(c),
                None => break,
            };
        }
        assert_eq!(collected, open);
    }

    #[test]
    fn test_page_cursor_boundaries_under_capped_daa_range() {
        let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut spk_store = DbUtxoSetByScriptPublicKeyStore::new(db.clone(), CachePolicy::Empty);
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty, DbUtxoSetByScriptPublicKeyStore::clone(&spk_store));

        let id = Hash::from_slice(&[0x01u8; HASH_SIZE]);
        let script_public_key = ScriptPublicKey::from_vec(0, vec![0x01]);
        populate(
            &mut spk_store,
            &mut store,
            (1..=3u64).map(|daa| (id, daa, script_public_key.clone(), create_outpoint(daa, 0), daa * 100)).collect(),
        );

        // a cursor exactly at the range start resumes from the first row
        let cursor = CovenantUtxoPageCursor::new(id, 1, script_public_key.clone(), create_outpoint(1, 0));
        let (rows, next_cursor) = page_entries(&store, vec![id], 1..=3, &[], Some(cursor), None);
        assert_eq!(rows, vec![(id, 1), (id, 2), (id, 3)]);
        assert!(next_cursor.is_none());

        // a cursor at daa u64::MAX lies past the capped range end and is rejected
        let cursor = CovenantUtxoPageCursor::new(id, u64::MAX, script_public_key, create_outpoint(3, 0));
        let result = store.get_utxos_by_covenant_ids_page(vec![id], 0..=u64::MAX, &[], Some(cursor), None);
        assert!(matches!(result.unwrap_err(), UtxoIndexError::InvalidCovenantCursor(_)));
    }
}
