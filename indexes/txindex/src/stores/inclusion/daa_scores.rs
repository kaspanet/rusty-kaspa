use crate::model::bluescore_refs::DaaScoreIncludingRefData;

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DbWriter, DirectDbWriter, StoreResult, DB},
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
const DAA_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
const DAA_SCORE_STORE_KEY_LEN: usize = DAA_SCORE_SIZE + TRANSACTION_ID_SIZE; // 40

/// Type alias for the tuple expected by [`DaaScoreRefIter`]iterator
pub type DaaScoreRefTuple = (u64, TransactionId); // (daa_score, ref_type, txid)

/// Iterator over [`DaaScoreRefTuple`] the type expected to be supplied to the store
pub struct DaaScoreRefIter<I>(I);

impl<I> DaaScoreRefIter<I> {
    #[inline(always)]
    pub fn new(iter: I) -> Self {
        DaaScoreRefIter(iter)
    }
}

impl<I> Iterator for DaaScoreRefIter<I>
where
    I: Iterator<Item = DaaScoreRefTuple>,
{
    type Item = DaaScoreRefTuple;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<I> DaaScoreRefIter<I>
where
    I: Iterator<Item = DaaScoreRefTuple>,
{
    #[inline(always)]
    pub fn take(self, n: usize) -> DaaScoreRefIter<std::iter::Take<I>> {
        DaaScoreRefIter(self.0.take(n))
    }
}
/// Iterator over [`DaaScoreRefData`] returned by the store
pub struct DaaScoreRefDataResIter<I>(pub I);

impl<I> DaaScoreRefDataResIter<I> {
    #[inline(always)]
    pub fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I> Iterator for DaaScoreRefDataResIter<I>
where
    I: Iterator,
{
    type Item = I::Item;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

// TODO (Relaxed): Consider using a KeyBuilder pattern for this more complex key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DaaScoreRefKey(pub [u8; DAA_SCORE_STORE_KEY_LEN]);

impl std::fmt::Display for DaaScoreRefKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as hex string for readability
        for byte in &self.0 {
            write!(f, "{:02x}", byte)?;
        }
        Ok(())
    }
}

// impl Builder pattern for BlueScoreRefKey
impl DaaScoreRefKey {
    #[inline(always)]
    pub fn new_minimized() -> Self {
        Self::default()
    }

    #[inline(always)]
    pub fn new_maximized() -> Self {
        Self([u8::MAX; DAA_SCORE_STORE_KEY_LEN])
    }

    #[inline(always)]
    pub fn with_daa_score(mut self, daa_score: u64) -> Self {
        self.0[0..DAA_SCORE_SIZE].copy_from_slice(&daa_score.to_be_bytes());
        self
    }

    #[inline(always)]
    pub fn with_txid(mut self, txid: TransactionId) -> Self {
        self.0[DAA_SCORE_SIZE..DAA_SCORE_SIZE + TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        self
    }

    #[inline(always)]
    pub fn get_daa_score(&self) -> u64 {
        u64::from_be_bytes(self.0[0..DAA_SCORE_SIZE].try_into().unwrap())
    }

    #[inline(always)]
    pub fn get_txid(&self) -> TransactionId {
        TransactionId::from_slice(&self.0[DAA_SCORE_SIZE..DAA_SCORE_SIZE + TRANSACTION_ID_SIZE])
    }
}

impl From<DaaScoreRefKey> for DaaScoreIncludingRefData {
    #[inline(always)]
    fn from(key: DaaScoreRefKey) -> Self {
        DaaScoreIncludingRefData { including_daa_score: key.get_daa_score(), tx_id: key.get_txid() }
    }
}

impl AsRef<[u8]> for DaaScoreRefKey {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Box<[u8]>> for DaaScoreRefKey {
    #[inline(always)]
    fn from(data: Box<[u8]>) -> Self {
        let data: [u8; DAA_SCORE_STORE_KEY_LEN] = (&*data).try_into().expect("slice with incorrect length");
        Self(data)
    }
}

impl Default for DaaScoreRefKey {
    #[inline(always)]
    fn default() -> Self {
        Self([0u8; DAA_SCORE_STORE_KEY_LEN])
    }
}

// --- Traits ---

pub trait TxIndexIncludingDaaScoreRefReader {
    fn get_daa_score_refs(
        &self,
        daa_score_range: impl RangeBounds<u64>,
        limit: Option<usize>, // if some, Will stop after limit is reached
    ) -> StoreResult<DaaScoreRefDataResIter<impl Iterator<Item = DaaScoreIncludingRefData>>>;

    /// Retrieves all remaining daa score refs at a specific daa score defined from an explicit daa score ref data point.
    /// Required for pagination over daa score boundaries.
    fn get_remaining_daa_score_refs(
        &self,
        daa_score_ref_data: DaaScoreIncludingRefData,
    ) -> StoreResult<DaaScoreRefDataResIter<impl Iterator<Item = DaaScoreIncludingRefData>>>;
    fn get_lowest_daa_score_ref(&self) -> StoreResult<Option<DaaScoreIncludingRefData>>;
}

pub trait TxIndexIncludingDaaScoreRefStore: TxIndexIncludingDaaScoreRefReader {
    fn add_daa_score_refs<I>(&mut self, writer: &mut impl DbWriter, to_add_data: DaaScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = DaaScoreRefTuple>; // (daa_score, store_ident, txid)
    fn remove_daa_score_refs(&mut self, writer: &mut impl DbWriter, to_remove_daa_score_range: Range<u64>) -> StoreResult<()>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

// --- implementations ---
#[derive(Clone)]
pub struct DbTxIndexIncludingDaaScoreRefStore {
    db: Arc<DB>,
    access: CachedDbAccess<DaaScoreRefKey, ()>, // No value, only keys matter
}

impl DbTxIndexIncludingDaaScoreRefStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::IncludingDaaScoreRefs.into()),
        }
    }
}

impl TxIndexIncludingDaaScoreRefReader for DbTxIndexIncludingDaaScoreRefStore {
    /// This is inclusive in regards to the range's end boundry
    fn get_daa_score_refs(
        &self,
        daa_score_range: impl RangeBounds<u64>,
        limit: Option<usize>, // if some, Will stop after limit is reached
    ) -> StoreResult<DaaScoreRefDataResIter<impl Iterator<Item = DaaScoreIncludingRefData>>> {
        Ok(DaaScoreRefDataResIter(
            self.access
                .seek_iterator(
                    None,
                    Some(DaaScoreRefKey::new_minimized().with_daa_score(match daa_score_range.start_bound() {
                        std::ops::Bound::Included(v) => *v,
                        std::ops::Bound::Excluded(v) => v.saturating_add(1),
                        std::ops::Bound::Unbounded => u64::MIN,
                    })),
                    Some(DaaScoreRefKey::new_maximized().with_daa_score(match daa_score_range.end_bound() {
                        std::ops::Bound::Included(v) => *v,
                        std::ops::Bound::Excluded(v) => v.saturating_sub(1),
                        std::ops::Bound::Unbounded => u64::MAX,
                    })),
                    limit.unwrap_or(usize::MAX),
                    false, // We follow range boundaries already.
                )
                .map(|res| {
                    let (key, _) = res.unwrap();
                    DaaScoreRefKey::from(key).into()
                }),
        ))
    }

    fn get_remaining_daa_score_refs(
        &self,
        daa_score_ref_data: DaaScoreIncludingRefData,
    ) -> StoreResult<DaaScoreRefDataResIter<impl Iterator<Item = DaaScoreIncludingRefData>>> {
        Ok(DaaScoreRefDataResIter(
            self.access
                .seek_iterator(
                    None,
                    Some(
                        DaaScoreRefKey::new_minimized()
                            .with_txid(daa_score_ref_data.tx_id)
                            .with_daa_score(daa_score_ref_data.including_daa_score),
                    ),
                    Some(DaaScoreRefKey::new_maximized().with_daa_score(daa_score_ref_data.including_daa_score)),
                    usize::MAX,
                    true, // We already know about start point
                )
                .map(|res| {
                    let (key, _) = res.unwrap();
                    DaaScoreRefKey::from(key).into()
                }),
        ))
    }

    fn get_lowest_daa_score_ref(&self) -> StoreResult<Option<DaaScoreIncludingRefData>> {
        Ok(self.access.iterator().next().map(|res| DaaScoreIncludingRefData::from(DaaScoreRefKey::from(res.unwrap().0))))
    }
}

impl TxIndexIncludingDaaScoreRefStore for DbTxIndexIncludingDaaScoreRefStore {
    fn add_daa_score_refs<I>(&mut self, writer: &mut impl DbWriter, to_add_data: DaaScoreRefIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = DaaScoreRefTuple>,
    {
        let mut kv_iter = to_add_data
            .into_iter()
            .map(|(daa_score, txid)| (DaaScoreRefKey::new_minimized().with_daa_score(daa_score).with_txid(txid), ()));
        self.access.write_many_without_cache(writer, &mut kv_iter)
    }

    fn remove_daa_score_refs(&mut self, writer: &mut impl DbWriter, to_remove_daa_score_range: Range<u64>) -> StoreResult<()> {
        self.access.delete_range(
            writer,
            DaaScoreRefKey::new_minimized().with_daa_score(to_remove_daa_score_range.start),
            DaaScoreRefKey::new_maximized().with_daa_score(to_remove_daa_score_range.end),
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
        prelude::{BatchDbWriter, CachePolicy, ConnBuilder, WriteBatch},
    };
    use rand::Rng;

    fn random_txid() -> TransactionId {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        TransactionId::from_slice(&bytes)
    }

    #[test]
    fn test_daa_score_refs_key_roundtrip() {
        let daa_score = 123456789u64;
        let txid = random_txid();
        let key = DaaScoreRefKey::new_minimized().with_daa_score(daa_score).with_txid(txid);
        let daa_score_ref_data = DaaScoreIncludingRefData::from(key.clone());
        assert_eq!(daa_score, daa_score_ref_data.including_daa_score);
        assert_eq!(txid, daa_score_ref_data.tx_id);
    }

    #[test]
    fn test_blue_score_refs_value_unit_serialization() {
        let value = ();
        let bytes = bincode::serialize(&value).unwrap();
        assert!(bytes.is_empty()); // Unit type should serialize to empty
        let _: () = bincode::deserialize(&bytes).unwrap();
    }

    #[test]
    fn test_get_daa_score_refs_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let mut store = DbTxIndexIncludingDaaScoreRefStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);

        // Add some test data (only inclusion refs)
        let to_add = vec![(100u64, random_txid()), (250u64, random_txid())];
        let to_add_clone = to_add.clone();

        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.add_daa_score_refs(&mut writer, DaaScoreRefIter(to_add_clone.into_iter())).unwrap();
        txindex_db.write(write_batch).unwrap();

        // Test retrieval
        let results = store.get_daa_score_refs(100u64..300u64, None).unwrap().collect::<Vec<DaaScoreIncludingRefData>>();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].including_daa_score, to_add[0].0);
        assert_eq!(results[0].tx_id, to_add[0].1);
        assert_eq!(results[1].including_daa_score, to_add[1].0);
        assert_eq!(results[1].tx_id, to_add[1].1);

        // Clean up test
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.remove_daa_score_refs(&mut writer, 0..150u64).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert_eq!(store.get_daa_score_refs(.., None).unwrap().collect::<Vec<_>>().len(), 1);
        store.delete_all().unwrap();
        assert!(store.get_daa_score_refs(.., None).unwrap().collect::<Vec<_>>().is_empty());
    }
}
