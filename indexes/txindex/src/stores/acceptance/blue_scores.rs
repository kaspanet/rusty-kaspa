use crate::model::score_refs::BlueScoreAcceptingRefData;

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DB, DbWriter, DirectDbWriter, StoreResult},
    registry::DatabaseStorePrefixes,
};

use std::{
    // iter::Take,
    mem,
    ops::{Range, RangeBounds},
    sync::Arc,
};

// --- Types, Constants, Structs, Enums ---

// Field size constants
const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
const BLUE_SCORE_STORE_KEY_LEN: usize = BLUE_SCORE_SIZE + TRANSACTION_ID_SIZE; // 40

/// Type alias for the tuple expected by [`BlueScoreRefIter`] iterator
pub type BlueScoreRefTuple = (u64, TransactionId); // (blue_score, transaction_id)

/// Iterator over [`BlueScoreRefTuple`] the type expected to be supplied to the store
pub struct BlueScoreRefIter<I>(I);

impl<I> BlueScoreRefIter<I> {
    #[inline(always)]
    pub fn new(iter: I) -> Self {
        BlueScoreRefIter(iter)
    }
}

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

impl<I> BlueScoreRefIter<I>
where
    I: Iterator<Item = BlueScoreRefTuple>,
{
    #[inline(always)]
    pub fn take(self, n: usize) -> BlueScoreRefIter<std::iter::Take<I>> {
        BlueScoreRefIter(self.0.take(n))
    }
}
/// Iterator over [`BlueScoreRefData`] returned by the store
pub struct BlueScoreRefDataResIter<I>(pub I);

impl<I> BlueScoreRefDataResIter<I> {
    #[inline(always)]
    pub fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I> Iterator for BlueScoreRefDataResIter<I>
where
    I: Iterator,
{
    type Item = I::Item;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlueScoreRefKey(pub [u8; BLUE_SCORE_STORE_KEY_LEN]);

impl std::fmt::Display for BlueScoreRefKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as hex string for readability
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// impl Builder pattern for BlueScoreRefKey
impl BlueScoreRefKey {
    #[inline(always)]
    pub fn new_minimized() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn new_maximized() -> Self {
        Self([u8::MAX; BLUE_SCORE_STORE_KEY_LEN])
    }

    #[inline(always)]
    pub fn with_blue_score(mut self, blue_score: u64) -> Self {
        self.0[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        self
    }

    #[inline(always)]
    pub fn with_transaction_id(mut self, transaction_id: TransactionId) -> Self {
        self.0[BLUE_SCORE_SIZE..BLUE_SCORE_SIZE + TRANSACTION_ID_SIZE].copy_from_slice(&transaction_id.as_bytes());
        self
    }

    #[inline(always)]
    pub fn extract_blue_score(&self) -> u64 {
        u64::from_be_bytes(self.0[0..BLUE_SCORE_SIZE].try_into().unwrap())
    }

    #[inline(always)]
    pub fn extract_transaction_id(&self) -> TransactionId {
        TransactionId::from_slice(&self.0[BLUE_SCORE_SIZE..BLUE_SCORE_SIZE + TRANSACTION_ID_SIZE])
    }
}

impl From<BlueScoreRefKey> for BlueScoreAcceptingRefData {
    #[inline(always)]
    fn from(key: BlueScoreRefKey) -> Self {
        BlueScoreAcceptingRefData { blue_score: key.extract_blue_score(), transaction_id: key.extract_transaction_id() }
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
        let data: [u8; BLUE_SCORE_STORE_KEY_LEN] = (&*data).try_into().expect("slice with incorrect length");
        Self(data)
    }
}

impl Default for BlueScoreRefKey {
    #[inline(always)]
    fn default() -> Self {
        Self([0u8; BLUE_SCORE_STORE_KEY_LEN])
    }
}

// --- Traits ---

pub trait TxIndexAcceptingBlueScoreRefReader {
    fn get_blue_score_refs(
        &self,
        blue_score_range: impl RangeBounds<u64>,
        limit: Option<usize>, // if some, Will stop after limit is reached
    ) -> StoreResult<BlueScoreRefDataResIter<impl Iterator<Item = BlueScoreAcceptingRefData>>>;

    fn is_empty(&self) -> StoreResult<bool> {
        Ok(self.get_blue_score_refs(0u64..=u64::MAX, Some(1))?.next().is_none())
    }
}

pub trait TxIndexAcceptingBlueScoreRefStore: TxIndexAcceptingBlueScoreRefReader {
    fn add_blue_score_refs<I>(&mut self, writer: &mut impl DbWriter, to_add_data: BlueScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = BlueScoreRefTuple>; // BlueScoreRefTuple = (blue_score, transaction_id)
    fn remove_blue_score_refs(&mut self, writer: &mut impl DbWriter, to_remove_blue_score_range: Range<u64>) -> StoreResult<()>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

// --- implementations ---
#[derive(Clone)]
pub struct DbTxIndexAcceptingBlueScoreRefStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlueScoreRefKey, ()>, // No value, only keys matter
}

impl DbTxIndexAcceptingBlueScoreRefStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::AcceptingBlueScoreRefs.into()),
        }
    }
}

impl TxIndexAcceptingBlueScoreRefReader for DbTxIndexAcceptingBlueScoreRefStore {
    fn get_blue_score_refs(
        &self,
        blue_score_range: impl RangeBounds<u64>,
        limit: Option<usize>, // if some, Will stop after limit is reached
    ) -> StoreResult<BlueScoreRefDataResIter<impl Iterator<Item = BlueScoreAcceptingRefData>>> {
        Ok(BlueScoreRefDataResIter(
            self.access
                .seek_iterator(
                    None,
                    Some(BlueScoreRefKey::new_minimized().with_blue_score(match blue_score_range.start_bound() {
                        std::ops::Bound::Included(v) => *v,
                        std::ops::Bound::Excluded(v) => v.saturating_add(1),
                        std::ops::Bound::Unbounded => u64::MIN,
                    })),
                    Some(BlueScoreRefKey::new_maximized().with_blue_score(match blue_score_range.end_bound() {
                        std::ops::Bound::Included(v) => *v,
                        std::ops::Bound::Excluded(v) => v.saturating_sub(1),
                        std::ops::Bound::Unbounded => u64::MAX,
                    })),
                    limit.unwrap_or(usize::MAX),
                    false, // We follow range boundaries already.
                )
                .map(|res| {
                    let (key, _) = res.unwrap();
                    BlueScoreRefKey::from(key).into()
                }),
        ))
    }
}

impl TxIndexAcceptingBlueScoreRefStore for DbTxIndexAcceptingBlueScoreRefStore {
    fn add_blue_score_refs<I>(&mut self, writer: &mut impl DbWriter, to_add_data: BlueScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = BlueScoreRefTuple>,
    {
        let mut kv_iter = to_add_data.into_iter().map(|(blue_score, transaction_id)| {
            (BlueScoreRefKey::new_minimized().with_blue_score(blue_score).with_transaction_id(transaction_id), ())
        });
        self.access.write_many_without_cache(writer, &mut kv_iter)
    }

    fn remove_blue_score_refs(&mut self, writer: &mut impl DbWriter, to_remove_blue_score_range: Range<u64>) -> StoreResult<()> {
        self.access.delete_range(
            writer,
            BlueScoreRefKey::new_minimized().with_blue_score(to_remove_blue_score_range.start),
            BlueScoreRefKey::new_maximized().with_blue_score(to_remove_blue_score_range.end),
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
        prelude::{BatchDbWriter, CachePolicy, ConnBuilder},
    };
    use rocksdb::WriteBatch;

    #[test]
    fn test_blue_score_refs_key_roundtrip() {
        let blue_score = 123456789u64;
        let transaction_id = TransactionId::from_u64_word(1u64);
        let key = BlueScoreRefKey::new_minimized().with_blue_score(blue_score).with_transaction_id(transaction_id);
        let blue_score_ref_data = BlueScoreAcceptingRefData::from(key.clone());
        assert_eq!(blue_score, blue_score_ref_data.blue_score);
        assert_eq!(transaction_id, blue_score_ref_data.transaction_id);
    }

    #[test]
    fn test_blue_score_refs_value_unit_serialization() {
        let value = ();
        let bytes = bincode::serialize(&value).unwrap();
        assert!(bytes.is_empty()); // Unit type should serialize to empty
        let _: () = bincode::deserialize(&bytes).unwrap();
    }

    #[test]
    fn test_get_blue_score_refs_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTxIndexAcceptingBlueScoreRefStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);

        // Add some test data (only acceptance refs)
        let to_add = vec![(100u64, TransactionId::from_u64_word(1u64)), (250u64, TransactionId::from_u64_word(2u64))];
        let to_add_clone = to_add.clone();

        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.add_blue_score_refs(&mut writer, BlueScoreRefIter(to_add_clone.into_iter())).unwrap();
        txindex_db.write(write_batch).unwrap();

        // Test retrieval
        let results = store.get_blue_score_refs(100u64..300u64, None).unwrap().collect::<Vec<BlueScoreAcceptingRefData>>();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].blue_score, to_add[0].0);
        assert_eq!(results[0].transaction_id, to_add[0].1);
        assert_eq!(results[1].blue_score, to_add[1].0);
        assert_eq!(results[1].transaction_id, to_add[1].1);

        // Clean up test
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.remove_blue_score_refs(&mut writer, 0..150u64).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert_eq!(store.get_blue_score_refs(.., None).unwrap().collect::<Vec<_>>().len(), 1);
        store.delete_all().unwrap();
        assert!(store.get_blue_score_refs(.., None).unwrap().collect::<Vec<_>>().is_empty());
    }
}
