use crate::model::bluescore_ref::BlueScoreRefData;

use kaspa_consensus_core::tx::TransactionId;
use kaspa_database::{
    prelude::{CachePolicy, CachedDbAccess, DirectDbWriter, StoreResult, DB},
    registry::DatabaseStorePrefixes,
};

use std::{mem, ops::Range, sync::Arc};

// --- Types, Constants, Structs, Enums ---

// Field size constants
pub const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
pub const STORE_IDENT_SIZE: usize = mem::size_of::<DatabaseStorePrefixes>(); // 1
pub const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
pub const PRUNING_STORE_KEY_LEN: usize = BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE; // 41

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StoreIdent {
    Acceptance = DatabaseStorePrefixes::TransactionAcceptanceData as u8,
    Inclusion = DatabaseStorePrefixes::TransactionInclusionData as u8,
}

// Customized serialization and deserialization for StoreIdent, to keep 1 byte
impl serde::Serialize for StoreIdent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(*self as u8)
    }
}

impl<'de> serde::Deserialize<'de> for StoreIdent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let v = u8::deserialize(deserializer)?;
        match v {
            x if x == StoreIdent::Acceptance as u8 => Ok(StoreIdent::Acceptance),
            x if x == StoreIdent::Inclusion as u8 => Ok(StoreIdent::Inclusion),
            _ => Err(serde::de::Error::custom("invalid StoreIdent byte")),
        }
    }
}

impl From<DatabaseStorePrefixes> for StoreIdent {
    fn from(prefix: DatabaseStorePrefixes) -> Self {
        match prefix {
            DatabaseStorePrefixes::TransactionAcceptanceData => StoreIdent::Acceptance,
            DatabaseStorePrefixes::TransactionInclusionData => StoreIdent::Inclusion,
            _ => panic!("Invalid DatabaseStorePrefixes for StoreIdent"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlueScoreRefKey(pub [u8; PRUNING_STORE_KEY_LEN]);

impl BlueScoreRefKey {
    #[inline(always)]
    pub fn from_parts(blue_score: u64, store_ident: StoreIdent, txid: TransactionId) -> Self {
        let mut bytes = [0u8; PRUNING_STORE_KEY_LEN];
        // be important for rocks db ordering
        bytes[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes()); // to_be_bytes important for rocks db ordering
        bytes[BLUE_SCORE_SIZE] = store_ident as u8;
        bytes[BLUE_SCORE_SIZE + STORE_IDENT_SIZE..BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE]
            .copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    pub fn new_blue_score_only(blue_score: u64) -> Self {
        let mut bytes = [0u8; PRUNING_STORE_KEY_LEN];
        bytes[0..BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        Self(bytes)
    }
}

impl From<BlueScoreRefKey> for BlueScoreRefData {
    fn from(key: BlueScoreRefKey) -> Self {
        let bytes = &key.0;
        let blue_score = u64::from_be_bytes(bytes[0..BLUE_SCORE_SIZE].try_into().unwrap());
        let store_ident = match bytes[BLUE_SCORE_SIZE] {
            x if x == StoreIdent::Acceptance as u8 => StoreIdent::Acceptance,
            x if x == StoreIdent::Inclusion as u8 => StoreIdent::Inclusion,
            _ => panic!("Invalid store ident byte in BlueScoreRefKey"),
        };
        let txid = TransactionId::from_slice(
            &bytes[BLUE_SCORE_SIZE + STORE_IDENT_SIZE..BLUE_SCORE_SIZE + STORE_IDENT_SIZE + TRANSACTION_ID_SIZE],
        );
        Self { blue_score, txid, store_ident }
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
    fn from(data: BlueScoreRefData) -> Self {
        BlueScoreRefKey::from_parts(data.blue_score, data.store_ident, data.txid)
    }
}

pub enum StoreQuery {
    IncludedTransactionStoreKey = 0,
    AcceptedTransactionStoreKey,
    Both,
}

// --- Traits ---

trait TxIndexBlueScoreRefReader {
    fn get_blue_score_refs(
        &self,
        blue_score_range: Range<u64>,
        limit: usize, // Will stop after limit is reached
        query: StoreQuery,
        skip_first: bool,
    ) -> StoreResult<Box<dyn Iterator<Item = BlueScoreRefData> + '_>>;
}

trait TxIndexBlueScoreRefStore: TxIndexBlueScoreRefReader {
    fn add_blue_score_refs<T>(&mut self, to_add_data: T) -> StoreResult<()>
    where
        T: IntoIterator<Item = (u64, StoreIdent, Vec<TransactionId>)>;
    fn remove_blue_score_refs(&mut self, to_remove_blue_score_range: Range<u64>) -> StoreResult<()>;
}

// --- implementations ---

pub struct DbTxIndexBlueScoreRefStore {
    db: Arc<DB>,
    access: CachedDbAccess<BlueScoreRefKey, ()>, // No value, only keys matter
}

impl DbTxIndexBlueScoreRefStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::BlueScoreRef.into()) }
    }
}

impl TxIndexBlueScoreRefReader for DbTxIndexBlueScoreRefStore {
    fn get_blue_score_refs(
        &self,
        blue_score_range: Range<u64>,
        limit: usize, // Will stop after limit is reached
        query: StoreQuery,
        skip_first: bool,
    ) -> StoreResult<Box<dyn Iterator<Item = BlueScoreRefData> + '_>> {
        Ok(Box::new(
            self.access
                .seek_iterator(
                    None,
                    Some(BlueScoreRefKey::new_blue_score_only(blue_score_range.start)),
                    Some(BlueScoreRefKey::new_blue_score_only(blue_score_range.end + 1)), // +1 to make the range inclusive
                    limit,
                    skip_first,
                )
                .map(|item| BlueScoreRefData::from(BlueScoreRefKey::from(item.unwrap().0)))
                .filter(move |data| match query {
                    StoreQuery::Both => true,
                    StoreQuery::AcceptedTransactionStoreKey => data.store_ident == StoreIdent::Acceptance,
                    StoreQuery::IncludedTransactionStoreKey => data.store_ident == StoreIdent::Inclusion,
                }),
        ))
    }
}

impl TxIndexBlueScoreRefStore for DbTxIndexBlueScoreRefStore {
    fn add_blue_score_refs<T>(&mut self, to_add_data: T) -> StoreResult<()>
    where
        T: IntoIterator<Item = (u64, StoreIdent, Vec<TransactionId>)>,
    {
        let mut kv_iter = to_add_data.into_iter().flat_map(|(blue_score, store_ident, txids)| {
            txids.into_iter().map(move |txid| (BlueScoreRefKey::from_parts(blue_score, store_ident, txid), ()))
        });
        self.access.write_many_without_cache(DirectDbWriter::new(&self.db), &mut kv_iter)
    }

    fn remove_blue_score_refs(&mut self, to_remove_blue_score_range: Range<u64>) -> StoreResult<()> {
        // remove a range of blue scores by deleting the entire buckets for each blue score in the range
        self.access.delete_range(
            DirectDbWriter::new(&self.db),
            BlueScoreRefKey::new_blue_score_only(to_remove_blue_score_range.start),
            BlueScoreRefKey::new_blue_score_only(to_remove_blue_score_range.end),
        )
    }
}

// --- Tests ---

#[cfg(test)]
mod tests {
    use super::*;
    use bincode;
    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use rand::Rng;

    fn random_txid() -> TransactionId {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        TransactionId::from_slice(&bytes)
    }

    #[test]
    fn test_pruning_store_key_roundtrip() {
        let blue_score = 123456789u64;
        let store_ident = StoreIdent::Inclusion;
        let txid = random_txid();
        let key = BlueScoreRefKey::from_parts(blue_score, store_ident, txid);
        let blue_score_ref_data = BlueScoreRefData::from(key.clone());
        assert_eq!(blue_score, blue_score_ref_data.blue_score);
        assert_eq!(store_ident, blue_score_ref_data.store_ident);
        assert_eq!(txid, blue_score_ref_data.txid);
    }

    #[test]
    fn test_pruning_store_value_unit_serialization() {
        let value = ();
        let bytes = bincode::serialize(&value).unwrap();
        assert!(bytes.is_empty()); // Unint type should serialize to empty
        let _: () = bincode::deserialize(&bytes).unwrap();
    }

    #[test]
    fn test_store_ident_from_database_store_prefixes() {
        let acceptance_prefix = DatabaseStorePrefixes::TransactionAcceptanceData;
        let inclusion_prefix = DatabaseStorePrefixes::TransactionInclusionData;

        let acceptance_ident = StoreIdent::from(acceptance_prefix);
        let inclusion_ident = StoreIdent::from(inclusion_prefix);

        assert_eq!(acceptance_ident, StoreIdent::Acceptance);
        assert_eq!(inclusion_ident, StoreIdent::Inclusion);
    }

    #[test]
    fn test_store_ident_serialization() {
        let acceptance_ident = StoreIdent::Acceptance;
        let inclusion_ident = StoreIdent::Inclusion;

        let acceptance_bytes = bincode::serialize(&acceptance_ident).unwrap();
        let inclusion_bytes = bincode::serialize(&inclusion_ident).unwrap();

        assert_eq!(acceptance_bytes, vec![StoreIdent::Acceptance as u8]);
        assert_eq!(inclusion_bytes, vec![StoreIdent::Inclusion as u8]);

        let deserialized_acceptance: StoreIdent = bincode::deserialize(&acceptance_bytes).unwrap();
        let deserialized_inclusion: StoreIdent = bincode::deserialize(&inclusion_bytes).unwrap();

        assert_eq!(deserialized_acceptance, acceptance_ident);
        assert_eq!(deserialized_inclusion, inclusion_ident);
    }

    #[test]
    fn test_get_blue_score_refs_filtering() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut store = DbTxIndexBlueScoreRefStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);

        // Add some test data
        let to_add = vec![
            (100u64, StoreIdent::Acceptance, vec![random_txid()]),
            (150u64, StoreIdent::Inclusion, vec![random_txid()]),
            (200u64, StoreIdent::Inclusion, vec![random_txid()]),
            (200u64, StoreIdent::Acceptance, vec![random_txid()]),
        ];
        let to_add_clone = to_add.clone();

        store.add_blue_score_refs(to_add).unwrap();

        // Test retrieval with filtering
        let results_acc = store
            .get_blue_score_refs(100u64..200u64, usize::MAX, StoreQuery::AcceptedTransactionStoreKey, true)
            .unwrap()
            .collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_acc.len(), 1);
        assert_eq!(results_acc[0], BlueScoreRefKey::from_parts(to_add_clone[3].0, to_add_clone[3].1, to_add_clone[3].2[0]).into());

        let results_inc = store
            .get_blue_score_refs(100u64..199u64, usize::MAX, StoreQuery::IncludedTransactionStoreKey, false)
            .unwrap()
            .collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_inc.len(), 1);
        assert_eq!(results_inc[0], BlueScoreRefKey::from_parts(to_add_clone[1].0, to_add_clone[1].1, to_add_clone[1].2[0]).into());

        let results_all =
            store.get_blue_score_refs(100u64..200u64, usize::MAX, StoreQuery::Both, false).unwrap().collect::<Vec<BlueScoreRefData>>();
        assert_eq!(results_all.len(), 4);
        for data in results_all {
            assert!(data.blue_score == 100 || data.blue_score == 150 || data.blue_score == 200);
        }

        // Clean up
        store.remove_blue_score_refs(100..201).unwrap();
        drop(store);
    }
}
