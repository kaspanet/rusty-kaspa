use crate::core::model::{UtxoEntryKeySuffixRecord, UtxoSetByCovenantId};
use kaspa_consensus_core::tx::{ScriptPublicKey, TransactionOutpoint};
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::{HASH_SIZE, Hash};
use kaspa_utils::mem_size::MemSizeEstimator;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::indexed_utxos::{
    DAA_SCORE_KEY_SIZE, DaaScoreKey, ScriptPublicKeyBucket, TRANSACTION_OUTPOINT_KEY_SIZE, TransactionOutpointKey,
};

/// Full covenant index key.
/// Consists of 32 bytes of covenant [`Hash`], followed by [DaaScoreKey], a variable amount of bytes of
/// [ScriptPublicKeyBucket], and [TransactionOutpointKey]. Ordering within a covenant id is daa score
/// asc, then script public key bucket bytes asc, then outpoint asc.
#[derive(Eq, Hash, PartialEq, PartialOrd, Ord, Debug, Clone, Serialize, Deserialize)]
pub struct CovenantUtxoEntryDbKey(Arc<Vec<u8>>);

// Key construction and decoding gain callers with the paginated query and the update path
#[allow(dead_code)]
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

pub trait UtxoSetByCovenantIdStoreReader {}

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

impl UtxoSetByCovenantIdStoreReader for DbUtxoSetByCovenantIdStore {}

#[derive(Clone)]
pub struct DbUtxoSetByCovenantIdStore {
    db: Arc<DB>,
    access: CachedDbAccess<CovenantUtxoEntryDbKey, CovenantIndexEntry>,
}

impl DbUtxoSetByCovenantIdStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::UtxoIndexCovenant.into()) }
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
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty);

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
        let mut store = DbUtxoSetByCovenantIdStore::new(db, CachePolicy::Empty);

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
}
