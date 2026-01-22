use crate::model::bluescore_refs::{BlueScoreAcceptingRefData, BlueScoreIncludingRefData, BlueScoreRefData};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DirectDbWriter, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};

use std::{
    mem,
    ops::{Range, RangeBounds},
    sync::Arc,
};

// --- Types, Constants, Structs, Enums ---

// Field size constants
const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
const STORE_IDENT_SIZE: usize = mem::size_of::<RefType>(); // 1
const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
const PRUNING_STORE_KEY_LEN: usize = BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE; // 41

/// Together defines the iterator that is expected for this store, and approprate reindexing.

/// Type alias for the tuple expected by blue score ref iterator
pub type BlueScoreRefTuple = (u64, RefType, TransactionId); // (blue_score, ref_type, txid)

/// Iterator over [`BlueScoreRefTuple`] the type expected to be supplied to the store
pub struct BlueScoreRefIter<I>(I);
impl<I> Iterator for BlueScoreRefIter<I>
where
    I: Iterator<Item = BlueScoreRefTuple>,
{
    type Item = BlueScoreRefTuple;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// Iterator over [`BlueScoreRefData`] returned by the store
pub struct BlueScoreRefDataResIter<I>(I);
impl<I> BlueScoreRefIter<I> {
    #[inline(always)]
    pub fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I> Iterator for BlueScoreRefDataResIter<I>
where
    I: Iterator<Item = BlueScoreRefData>,
{
    type Item = BlueScoreRefData;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

// 1 byte store / ref identifier, used in this store to differentiate between acceptance and inclusion refs
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefType {
    Acceptance = 1,
    Inclusion = 2,
}
impl serde::Serialize for RefType {
    #[inline(always)]
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> serde::Deserialize<'de> for RefType {
    #[inline(always)]
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        match v {
            x if x == RefType::Acceptance as u8 => Ok(RefType::Acceptance),
            x if x == RefType::Inclusion as u8 => Ok(RefType::Inclusion),
            _ => Err(serde::de::Error::custom("invalid RefType byte")),
        }
    }
}

// TODO (Relaxed): Consider using a KeyBuilder pattern for this more complex key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlueScoreRefKey(pub [u8; PRUNING_STORE_KEY_LEN]);

impl BlueScoreRefKey {
    #[inline(always)]
    pub fn from_parts(blue_score: u64, ref_type: RefType, txid: TransactionId) -> Self {
        let mut bytes = [0u8; PRUNING_STORE_KEY_LEN];
        // be important for rocks db ordering
        bytes[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes()); // to_be_bytes important for rocks db ordering
        bytes[BLUE_SCORE_SIZE] = ref_type as u8;
        bytes[BLUE_SCORE_SIZE + STORE_IDENT_SIZE..BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE]
            .copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    #[inline(always)]
    pub fn new_blue_score_and_ref_type_only_minimized(blue_score: u64, ref_type: Option<RefType>) -> Self {
        let mut bytes = [0u8; PRUNING_STORE_KEY_LEN];
        bytes[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        if let Some(ref_type) = ref_type {
            bytes[BLUE_SCORE_SIZE] = ref_type as u8;
        }
        Self(bytes)
    }

    #[inline(always)]
    pub fn new_blue_score_and_ref_type_only_maximized(blue_score: u64, ref_type: Option<RefType>) -> Self {
        let mut bytes = [u8::MAX; PRUNING_STORE_KEY_LEN];
        bytes[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        if let Some(ref_type) = ref_type {
            bytes[BLUE_SCORE_SIZE] = ref_type as u8;
        }
        Self(bytes)
    }
}

impl From<BlueScoreRefKey> for BlueScoreRefData {
    #[inline(always)]
    fn from(key: BlueScoreRefKey) -> Self {
        let bytes = &key.0;
        let blue_score = u64::from_be_bytes(bytes[0..BLUE_SCORE_SIZE].try_into().unwrap());
        let ref_type = match bytes[BLUE_SCORE_SIZE] {
            x if x == RefType::Acceptance as u8 => RefType::Acceptance,
            x if x == RefType::Inclusion as u8 => RefType::Inclusion,
            _ => panic!("Invalid ref type byte in BlueScoreRefKey"),
        };
        let txid = TransactionId::from_slice(
            &bytes[BLUE_SCORE_SIZE + STORE_IDENT_SIZE..BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE],
        );
        match ref_type {
            RefType::Acceptance => {
                BlueScoreRefData::Acceptance(BlueScoreAcceptingRefData { accepting_blue_score: blue_score, tx_id: txid })
            }
            RefType::Inclusion => {
                BlueScoreRefData::Inclusion(BlueScoreIncludingRefData { including_blue_score: blue_score, tx_id: txid })
            }
        }
    }
}

impl AsRef<[u8]> for BlueScoreRefKey {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Box<[u8]>> for BlueScoreRefKey {
    #[inline(always)]
    fn from(data: Box<[u8]>) -> Self {
        let data: [u8; PRUNING_STORE_KEY_LEN] = (&*data).try_into().expect("slice with incorrect length");
        Self(data)
    }
}

impl From<BlueScoreRefData> for BlueScoreRefKey {
    #[inline(always)]
    fn from(data: BlueScoreRefData) -> Self {
        match data {
            BlueScoreRefData::Acceptance(ref d) => {
                return BlueScoreRefKey::from_parts(d.accepting_blue_score, RefType::Acceptance, d.tx_id);
            }
            BlueScoreRefData::Inclusion(ref d) => {
                return BlueScoreRefKey::from_parts(d.including_blue_score, RefType::Inclusion, d.tx_id);
            }
        }
    }
}

pub enum StoreQuery {
    IncludedTransactionStoreKey = 0,
    AcceptedTransactionStoreKey,
    Both,
}

// --- Traits ---

pub trait TxIndexBlueScoreRefReader {
    fn get_blue_score_refs(
        &self,
        blue_score_range: impl RangeBounds<u64>,
        limit: usize, // Will stop after limit is reached
        query: StoreQuery,
    ) -> StoreResult<BlueScoreRefDataResIter<impl Iterator<Item = BlueScoreRefData>>>;
}

pub trait TxIndexBlueScoreRefStore: TxIndexBlueScoreRefReader {
    fn add_blue_score_refs<I>(&mut self, writer: BatchDbWriter, to_add_data: BlueScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = BlueScoreRefTuple>; // (blue_score, store_ident, txid)
    fn remove_blue_score_refs(&mut self, writer: BatchDbWriter, to_remove_blue_score_range: Range<u64>) -> StoreResult<()>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

// --- implementations ---
#[derive(Clone)]
pub struct DbTxIndexBlueScoreRefStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlueScoreRefKey, ()>, // No value, only keys matter
}

impl DbTxIndexBlueScoreRefStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::BlueScoreRefs.into()) }
    }
}

impl TxIndexBlueScoreRefReader for DbTxIndexBlueScoreRefStore {
    /// This is inclusive in regards to the range's end boundry
    fn get_blue_score_refs(
        &self,
        blue_score_range: impl RangeBounds<u64>,
        limit: usize, // Will stop after limit is reached
        query: StoreQuery,
    ) -> StoreResult<BlueScoreRefDataResIter<impl Iterator<Item = BlueScoreRefData>>> {
        Ok(BlueScoreRefDataResIter(
            self.access
                .seek_iterator(
                    None,
                    Some(BlueScoreRefKey::new_blue_score_and_ref_type_only_minimized(
                        match blue_score_range.start_bound() {
                            std::ops::Bound::Included(v) => *v,
                            std::ops::Bound::Excluded(v) => v.saturating_add(1),
                            std::ops::Bound::Unbounded => 0u64,
                        },
                        match query {
                            StoreQuery::Both => None,
                            StoreQuery::AcceptedTransactionStoreKey => Some(RefType::Acceptance),
                            StoreQuery::IncludedTransactionStoreKey => Some(RefType::Inclusion),
                        },
                    )),
                    Some(BlueScoreRefKey::new_blue_score_and_ref_type_only_maximized(
                        match blue_score_range.end_bound() {
                            std::ops::Bound::Included(v) => *v,
                            std::ops::Bound::Excluded(v) => v.saturating_sub(1),
                            std::ops::Bound::Unbounded => u64::MAX,
                        },
                        match query {
                            StoreQuery::Both => None,
                            StoreQuery::AcceptedTransactionStoreKey => Some(RefType::Acceptance),
                            StoreQuery::IncludedTransactionStoreKey => Some(RefType::Inclusion),
                        },
                    )),
                    limit,
                    false, // We follow range boundaries already.
                )
                .filter_map(move |item| {
                    let item = item.unwrap();
                    match query {
                        StoreQuery::Both => Some(BlueScoreRefData::from(BlueScoreRefKey::from(item.0))),
                        StoreQuery::AcceptedTransactionStoreKey => {
                            if item.0.as_ref()[BLUE_SCORE_SIZE] == RefType::Acceptance as u8 {
                                Some(BlueScoreRefData::from(BlueScoreRefKey::from(item.0)))
                            } else {
                                None
                            }
                        }
                        StoreQuery::IncludedTransactionStoreKey => {
                            if item.0.as_ref()[BLUE_SCORE_SIZE] == RefType::Inclusion as u8 {
                                Some(BlueScoreRefData::from(BlueScoreRefKey::from(item.0)))
                            } else {
                                None
                            }
                        }
                    }
                }),
        ))
    }
}

impl TxIndexBlueScoreRefStore for DbTxIndexBlueScoreRefStore {
    fn add_blue_score_refs<I>(&mut self, mut writer: BatchDbWriter, to_add_data: BlueScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = BlueScoreRefTuple>,
    {
        let mut kv_iter = to_add_data
            .into_iter()
            .map(|(blue_score, store_ident, txid)| (BlueScoreRefKey::from_parts(blue_score, store_ident, txid), ()));
        self.access.write_many_without_cache(&mut writer, &mut kv_iter)
    }

    fn remove_blue_score_refs(&mut self, mut writer: BatchDbWriter, to_remove_blue_score_range: Range<u64>) -> StoreResult<()> {
        self.access.delete_range(
            &mut writer,
            BlueScoreRefKey::new_blue_score_and_ref_type_only_minimized(to_remove_blue_score_range.start, None),
            BlueScoreRefKey::new_blue_score_and_ref_type_only_maximized(to_remove_blue_score_range.end, None),
        )
    }

    fn delete_all(&mut self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use bincode;
    use kaspa_database::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, WriteBatch},
    };
    use rand::Rng;

    fn random_txid() -> TransactionId {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        TransactionId::from_slice(&bytes)
    }

    #[test]
    fn test_blue_score_refs_key_roundtrip() {
        let blue_score = 123456789u64;
        let ref_type = RefType::Inclusion;
        let txid = random_txid();
        let key = BlueScoreRefKey::from_parts(blue_score, ref_type, txid);
        let blue_score_ref_data = BlueScoreRefData::from(key.clone());
        if let Some(BlueScoreRefData::Inclusion(data)) = Some(blue_score_ref_data.clone()) {
            assert_eq!(blue_score, data.including_blue_score);
            assert_eq!(txid, data.tx_id);
        } else {
            panic!("Expected Inclusion variant");
        }
        let ref_type = RefType::Acceptance;
        let key = BlueScoreRefKey::from_parts(blue_score, ref_type, txid);
        let blue_score_ref_data = BlueScoreRefData::from(key.clone());
        if let Some(BlueScoreRefData::Acceptance(data)) = Some(blue_score_ref_data.clone()) {
            assert_eq!(blue_score, data.accepting_blue_score);
            assert_eq!(txid, data.tx_id);
        } else {
            panic!("Expected Acceptance variant");
        }
    }

    #[test]
    fn test_blue_score_refs_value_unit_serialization() {
        let value = ();
        let bytes = bincode::serialize(&value).unwrap();
        assert!(bytes.is_empty()); // Unint type should serialize to empty
        let _: () = bincode::deserialize(&bytes).unwrap();
    }

    #[test]
    fn test_store_ident_serialization() {
        let acceptance_ident = RefType::Acceptance;
        let inclusion_ident = RefType::Inclusion;

        let acceptance_bytes = bincode::serialize(&acceptance_ident).unwrap();
        let inclusion_bytes = bincode::serialize(&inclusion_ident).unwrap();

        assert_eq!(acceptance_bytes, vec![RefType::Acceptance as u8]);
        assert_eq!(inclusion_bytes, vec![RefType::Inclusion as u8]);

        let deserialized_acceptance: RefType = bincode::deserialize(&acceptance_bytes).unwrap();
        let deserialized_inclusion: RefType = bincode::deserialize(&inclusion_bytes).unwrap();
        assert_eq!(deserialized_acceptance, acceptance_ident);
        assert_eq!(deserialized_inclusion, inclusion_ident);
    }

    #[test]
    fn test_get_blue_score_refs_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut store = DbTxIndexBlueScoreRefStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);

        // Add some test data
        let to_add = vec![
            (100u64, RefType::Acceptance, random_txid()),
            (150u64, RefType::Inclusion, random_txid()),
            (200u64, RefType::Inclusion, random_txid()),
            (200u64, RefType::Acceptance, random_txid()),
        ];
        let to_add_clone = to_add.clone();

        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.add_blue_score_refs(writer, BlueScoreRefIter(to_add.into_iter())).unwrap();
        txindex_db.write(write_batch).unwrap();

        // Test retrieval with filtering
        let results_acc = store
            .get_blue_score_refs(100u64..200u64, usize::MAX, StoreQuery::AcceptedTransactionStoreKey)
            .unwrap()
            .collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_acc.len(), 1);
        assert_eq!(results_acc[0], BlueScoreRefKey::from_parts(to_add_clone[0].0, to_add_clone[0].1, to_add_clone[0].2).into());

        let results_inc = store
            .get_blue_score_refs(100u64..=200u64, usize::MAX, StoreQuery::IncludedTransactionStoreKey)
            .unwrap()
            .collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_inc.len(), 2);
        assert_eq!(results_inc[0], BlueScoreRefKey::from_parts(to_add_clone[1].0, to_add_clone[1].1, to_add_clone[1].2).into());
        assert_eq!(results_inc[1], BlueScoreRefKey::from_parts(to_add_clone[2].0, to_add_clone[2].1, to_add_clone[2].2).into());

        let results_all = store.get_blue_score_refs(.., usize::MAX, StoreQuery::Both).unwrap().collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_all.len(), 4);
        for data in results_all {
            match data {
                BlueScoreRefData::Acceptance(ref d) => {
                    assert!(
                        (d.accepting_blue_score == to_add_clone[0].0 && d.tx_id == to_add_clone[0].2)
                            || (d.accepting_blue_score == to_add_clone[3].0 && d.tx_id == to_add_clone[3].2)
                    );
                }
                BlueScoreRefData::Inclusion(ref d) => {
                    assert!(
                        (d.including_blue_score == to_add_clone[1].0 && d.tx_id == to_add_clone[1].2)
                            || (d.including_blue_score == to_add_clone[2].0 && d.tx_id == to_add_clone[2].2)
                    );
                }
            }
        }

        // Clean up test
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.remove_blue_score_refs(writer, 0..150u64).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert!(store.get_blue_score_refs(.., usize::MAX, StoreQuery::Both).unwrap().collect::<Vec<_>>().len() == 2);
        store.delete_all().unwrap();
        assert!(store.get_blue_score_refs(.., usize::MAX, StoreQuery::Both).unwrap().collect::<Vec<_>>().is_empty());
    }
}
