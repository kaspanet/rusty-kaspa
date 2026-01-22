use crate::model::transactions::TxAcceptanceData;

use kaspa_consensus_core::{acceptance_data::MergesetIndexType, tx::TransactionId, Hash};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DbWriter, DirectDbWriter, StoreResult, WriteBatch, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use std::mem;
use std::sync::Arc;

// --- Types, Constants, Structs, Enums ---

// Field size constants
pub const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
pub const HASH_SIZE: usize = mem::size_of::<Hash>(); // 32
pub const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
pub const TRANSACTION_STORE_KEY_LEN: usize = TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE; // 72

// TODO (Relaxed): Consider using a KeyBuilder pattern for this more complex key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct AcceptedTransactionStoreKey(pub [u8; TRANSACTION_STORE_KEY_LEN]);

impl AcceptedTransactionStoreKey {
    #[inline(always)]
    pub fn from_parts(txid: TransactionId, blue_score: u64, hash: Hash) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        bytes[TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE]
            .copy_from_slice(&hash.as_bytes());
        Self(bytes)
    }

    #[inline(always)]
    pub fn from_tx_id_maximized(txid: TransactionId) -> Self {
        let mut bytes = [u8::MAX; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    #[inline(always)]
    pub fn from_tx_id_minimized(txid: TransactionId) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    #[inline(always)]
    pub fn from_tx_id_and_blue_score_maximized(txid: TransactionId, blue_score: u64) -> Self {
        let mut bytes = [u8::MAX; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        Self(bytes)
    }

    #[inline(always)]
    pub fn from_tx_id_and_blue_score_minimized(txid: TransactionId, blue_score: u64) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        Self(bytes)
    }
}

impl From<Box<[u8]>> for AcceptedTransactionStoreKey {
    #[inline(always)]
    fn from(data: Box<[u8]>) -> Self {
        let array: [u8; TRANSACTION_STORE_KEY_LEN] = (&*data).try_into().expect("slice with incorrect length");
        Self(array)
    }
}

impl From<(AcceptedTransactionStoreKey, MergesetIndexType)> for TxAcceptanceData {
    #[inline(always)]
    fn from(item: (AcceptedTransactionStoreKey, MergesetIndexType)) -> Self {
        let (key, mergeset_index) = item;
        let bytes = &key.0;
        let blue_score = u64::from_be_bytes(bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].try_into().unwrap());
        let block_hash =
            Hash::from_slice(&bytes[TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE]);
        Self { blue_score, block_hash, mergeset_index }
    }
}

impl MemSizeEstimator for AcceptedTransactionStoreKey {}

impl AsRef<[u8]> for AcceptedTransactionStoreKey {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Type alias for the tuple expected by the acceptance store iterator
pub type TxAcceptedTuple = (TransactionId, u64, Hash, MergesetIndexType);

/// Iterator over [`TxAcceptedTuple`]
pub struct TxAcceptedIter<I>(I);
impl<I> TxAcceptedIter<I> {
    #[inline(always)]
    pub fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I> Iterator for TxAcceptedIter<I>
where
    I: Iterator<Item = TxAcceptedTuple>,
{
    type Item = TxAcceptedTuple;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

// -- Store Traits ---

pub trait TxIndexAcceptedTransactionsStoreReader {
    fn get_transaction_acceptance_data(&self, txid: TransactionId) -> StoreResult<Vec<TxAcceptanceData>>;
}

pub trait TxIndexAcceptedTransactionsStore: TxIndexAcceptedTransactionsStoreReader {
    fn remove_transaction_acceptance_data(&mut self, writer: BatchDbWriter, txid: TransactionId, blue_score: u64) -> StoreResult<()>;
    fn add_accepted_transaction_data<I>(&mut self, writer: BatchDbWriter, to_add: TxAcceptedIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = TxAcceptedTuple>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

// --- Store Implementation ---
#[derive(Clone)]
pub struct DbTxIndexAcceptedTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<AcceptedTransactionStoreKey, MergesetIndexType>,
}

impl DbTxIndexAcceptedTransactionsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TransactionAcceptanceData.into()),
        }
    }
}

impl TxIndexAcceptedTransactionsStoreReader for DbTxIndexAcceptedTransactionsStore {
    fn get_transaction_acceptance_data(&self, txid: TransactionId) -> StoreResult<Vec<TxAcceptanceData>> {
        self.access
            .seek_iterator(
                None,
                Some(AcceptedTransactionStoreKey::from_tx_id_minimized(txid)),
                Some(AcceptedTransactionStoreKey::from_tx_id_maximized(txid)),
                usize::MAX,
                false,
            )
            .map(|res| {
                let (key, mergeset_index) = res.unwrap();
                Ok((AcceptedTransactionStoreKey::from(key), mergeset_index).into())
            })
            .collect()
    }
}

impl TxIndexAcceptedTransactionsStore for DbTxIndexAcceptedTransactionsStore {
    fn remove_transaction_acceptance_data(&mut self, writer: BatchDbWriter, txid: TransactionId, blue_score: u64) -> StoreResult<()> {
        self.access.delete_range(
            writer,
            AcceptedTransactionStoreKey::from_tx_id_and_blue_score_minimized(txid, blue_score),
            AcceptedTransactionStoreKey::from_tx_id_and_blue_score_maximized(txid, blue_score),
        )
    }

    fn add_accepted_transaction_data<I>(&mut self, writer: BatchDbWriter, to_add: TxAcceptedIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = TxAcceptedTuple>,
    {
        self.access.write_many_without_cache(
            writer,
            &mut to_add.map(|(txid, blue_score, block_hash, mergeset_index)| {
                (AcceptedTransactionStoreKey::from_parts(txid, blue_score, block_hash), mergeset_index)
            }),
        )
    }

    fn delete_all(&mut self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}

/// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use bincode;
    use kaspa_consensus::model::stores::acceptance_data::DbAcceptanceDataStore;
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

    fn random_hash() -> Hash {
        let mut rng = rand::thread_rng();
        let mut bytes = [0u8; 32];
        rng.fill(&mut bytes);
        Hash::from_slice(&bytes)
    }

    #[test]
    fn test_transaction_store_value_inclusion_roundtrip() {
        let value: MergesetIndexType = 42;
        let bytes = bincode::serialize(&value).unwrap();
        assert_eq!(bytes.len(), std::mem::size_of::<MergesetIndexType>());
        let value2: MergesetIndexType = bincode::deserialize(&bytes).unwrap();
        assert_eq!(value, value2);
    }

    #[test]
    fn test_transaction_store_key_inclusion_conversion() {
        let txid = random_txid();
        let blue_score = 987654321u64;
        let block_hash = random_hash();
        let key = AcceptedTransactionStoreKey::from_parts(txid, blue_score, block_hash);
        let mergeset_index = 42 as MergesetIndexType;
        let tx_acceptance_data: TxAcceptanceData = (key.clone(), mergeset_index).into();
        assert_eq!(tx_acceptance_data.blue_score, blue_score);
        assert_eq!(tx_acceptance_data.block_hash, block_hash);
        assert_eq!(tx_acceptance_data.mergeset_index, mergeset_index);
    }

    #[test]
    fn test_accepted_transactions_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut store = DbTxIndexAcceptedTransactionsStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);
        let txid1 = random_txid();
        let txid2 = random_txid();
        let block_hash1 = random_hash();
        let block_hash2 = random_hash();
        let blue_score1 = 100u64;
        let blue_score2 = 200u64;
        let mergeset_index1 = 1u16 as MergesetIndexType;
        let mergeset_index2 = 2u16 as MergesetIndexType;

        // Add included transaction data
        let mut batch = WriteBatch::new();
        store
            .add_accepted_transaction_data(
                BatchDbWriter::new(&mut batch),
                TxAcceptedIter(
                    vec![
                        (txid1, blue_score1, block_hash1, mergeset_index1),
                        (txid1, blue_score2, block_hash2, mergeset_index2),
                        (txid2, blue_score1, block_hash1, mergeset_index1),
                    ]
                    .into_iter(),
                ),
            )
            .unwrap();
        txindex_db.write(batch).unwrap();

        // Retrieve included transaction data for txid1
        let inclusions_txid1 = store.get_transaction_acceptance_data(txid1).unwrap();
        assert_eq!(inclusions_txid1.len(), 2);
        assert!(inclusions_txid1.contains(&TxAcceptanceData {
            blue_score: blue_score1,
            block_hash: block_hash1,
            mergeset_index: mergeset_index1,
        }));

        assert!(inclusions_txid1.contains(&TxAcceptanceData {
            blue_score: blue_score2,
            block_hash: block_hash2,
            mergeset_index: mergeset_index2,
        }));

        // Retrieve included transaction data for txid2
        let inclusions_txid2 = store.get_transaction_acceptance_data(txid2).unwrap();
        assert_eq!(inclusions_txid2.len(), 1);
        assert_eq!(
            inclusions_txid2[0],
            TxAcceptanceData { blue_score: blue_score1, block_hash: block_hash1, mergeset_index: mergeset_index1 }
        );

        let mut batch = WriteBatch::default();
        // Test removal and clean up
        store.remove_transaction_acceptance_data(BatchDbWriter::new(&mut batch), txid1, blue_score1).unwrap();
        txindex_db.write(batch).unwrap();
        assert!(store.get_transaction_acceptance_data(txid1).is_ok_and(|val| val.len() == 1
            && val[0] == TxAcceptanceData { blue_score: blue_score2, block_hash: block_hash2, mergeset_index: mergeset_index2 }));
        let mut batch = WriteBatch::default();
        store.remove_transaction_acceptance_data(BatchDbWriter::new(&mut batch), txid1, blue_score2).unwrap();
        txindex_db.write(batch).unwrap();
        let res = store.get_transaction_acceptance_data(txid1);
        println!("{:?}", res);
        assert!(store.get_transaction_acceptance_data(txid1).is_ok_and(|val| val.is_empty()));
        store.delete_all().unwrap();
        assert!(store.get_transaction_acceptance_data(txid2).is_ok_and(|val| val.is_empty()));
    }
}
