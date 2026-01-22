use crate::model::transactions::TxInclusionData;

use kaspa_consensus_core::{
    tx::{TransactionId, TransactionIndexType},
    Hash,
};
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DirectDbWriter, StoreResult, DB};
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
pub struct IncludedTransactionStoreKey(pub [u8; TRANSACTION_STORE_KEY_LEN]);

impl IncludedTransactionStoreKey {
    #[inline(always)]
    pub fn from_parts(txid: TransactionId, blue_score: u64, hash: Hash) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes()); // to_be_bytes important for rocks db ordering

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

impl From<(IncludedTransactionStoreKey, TransactionIndexType)> for TxInclusionData {
    #[inline(always)]
    fn from(item: (IncludedTransactionStoreKey, TransactionIndexType)) -> Self {
        let (key, index_within_block) = item;
        let bytes = &key.0;
        let blue_score = u64::from_be_bytes(bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].try_into().unwrap());
        let block_hash =
            Hash::from_slice(&bytes[TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE]);
        Self { blue_score, block_hash, index_within_block }
    }
}

impl MemSizeEstimator for IncludedTransactionStoreKey {}

impl AsRef<[u8]> for IncludedTransactionStoreKey {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Box<[u8]>> for IncludedTransactionStoreKey {
    #[inline(always)]
    fn from(data: Box<[u8]>) -> Self {
        let array: [u8; TRANSACTION_STORE_KEY_LEN] = (&*data).try_into().expect("slice with incorrect length");
        Self(array)
    }
}

/// Type alias for the tuple expected by the inclusion store iterator
pub type TxInclusionTuple = (TransactionId, u64, Hash, TransactionIndexType);

/// Iterator over [`TxInclusionTuple`]
pub struct TxInclusionIter<I>(I);
impl<I> TxInclusionIter<I> {
    #[inline(always)]
    pub fn new(inner: I) -> Self {
        Self(inner)
    }
}

impl<I> Iterator for TxInclusionIter<I>
where
    I: Iterator<Item = TxInclusionTuple>,
{
    type Item = TxInclusionTuple;
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

/// -- Store Traits ---

pub trait TxIndexIncludedTransactionsStoreReader {
    fn get_transaction_inclusion_data(&self, txid: TransactionId) -> StoreResult<Vec<TxInclusionData>>;
}

pub trait TxIndexIncludedTransactionsStore: TxIndexIncludedTransactionsStoreReader {
    fn remove_transaction_inclusion_data(&mut self, writer: BatchDbWriter, txid: TransactionId, blue_score: u64) -> StoreResult<()>;
    fn add_included_transaction_data<I>(&mut self, writer: BatchDbWriter, to_add: TxInclusionIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = TxInclusionTuple>;
    fn delete_all(&mut self) -> StoreResult<()>;
}

/// --- Store Implementation ---

#[derive(Clone)]
pub struct DbTxIndexIncludedTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<IncludedTransactionStoreKey, TransactionIndexType>,
}

impl DbTxIndexIncludedTransactionsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TransactionInclusionData.into()),
        }
    }
}

impl TxIndexIncludedTransactionsStoreReader for DbTxIndexIncludedTransactionsStore {
    fn get_transaction_inclusion_data(&self, txid: TransactionId) -> StoreResult<Vec<TxInclusionData>> {
        self.access
            .seek_iterator(
                None,
                Some(IncludedTransactionStoreKey::from_tx_id_minimized(txid)),
                Some(IncludedTransactionStoreKey::from_tx_id_maximized(txid)),
                usize::MAX,
                false,
            )
            .map(|res| {
                let (key, index_within_block) = res.unwrap();
                Ok((IncludedTransactionStoreKey::from(key), index_within_block).into())
            })
            .collect()
    }
}

impl TxIndexIncludedTransactionsStore for DbTxIndexIncludedTransactionsStore {
    fn remove_transaction_inclusion_data(&mut self, writer: BatchDbWriter, txid: TransactionId, blue_score: u64) -> StoreResult<()> {
        self.access.delete_range(
            writer,
            IncludedTransactionStoreKey::from_tx_id_and_blue_score_minimized(txid, blue_score),
            IncludedTransactionStoreKey::from_tx_id_and_blue_score_maximized(txid, blue_score),
        )
    }

    fn add_included_transaction_data<I>(&mut self, writer: BatchDbWriter, to_add: TxInclusionIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = TxInclusionTuple>,
    {
        self.access.write_many_without_cache(
            writer,
            &mut to_add.map(|(txid, blue_score, block_hash, index_within_block)| {
                (IncludedTransactionStoreKey::from_parts(txid, blue_score, block_hash), index_within_block)
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
    use kaspa_database::{
        create_temp_db,
        prelude::{BatchDbWriter, ConnBuilder, StoreError, WriteBatch},
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
        let value: TransactionIndexType = 42;
        let bytes = bincode::serialize(&value).unwrap();
        assert_eq!(bytes.len(), std::mem::size_of::<TransactionIndexType>());
        let value2: TransactionIndexType = bincode::deserialize(&bytes).unwrap();
        assert_eq!(value, value2);
    }

    #[test]
    fn test_transaction_store_key_inclusion_conversion() {
        let txid = random_txid();
        let blue_score = 987654321u64;
        let block_hash = random_hash();
        let key = IncludedTransactionStoreKey::from_parts(txid, blue_score, block_hash);
        let index_within_block = 42 as TransactionIndexType;
        let tx_inclusion_data: TxInclusionData = (key.clone(), index_within_block).into();
        assert_eq!(tx_inclusion_data.blue_score, blue_score);
        assert_eq!(tx_inclusion_data.block_hash, block_hash);
        assert_eq!(tx_inclusion_data.index_within_block, index_within_block);
    }

    #[test]
    fn test_included_transactions_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut store = DbTxIndexIncludedTransactionsStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);
        let txid1 = random_txid();
        let txid2 = random_txid();
        let block_hash1 = random_hash();
        let block_hash2 = random_hash();
        let blue_score1 = 100u64;
        let blue_score2 = 200u64;
        let index_within_block1 = 1u32;
        let index_within_block2 = 2u32;

        // Add included transaction data
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store
            .add_included_transaction_data(
                writer,
                TxInclusionIter(
                    vec![
                        (txid1, blue_score1, block_hash1, index_within_block1),
                        (txid1, blue_score2, block_hash2, index_within_block2),
                        (txid2, blue_score1, block_hash1, index_within_block1),
                    ]
                    .into_iter(),
                ),
            )
            .unwrap();
        txindex_db.write(write_batch).unwrap();

        // Retrieve included transaction data for txid1
        let inclusions_txid1 = store.get_transaction_inclusion_data(txid1).unwrap();
        assert_eq!(inclusions_txid1.len(), 2);
        assert!(inclusions_txid1.contains(&TxInclusionData {
            blue_score: blue_score1,
            block_hash: block_hash1,
            index_within_block: index_within_block1,
        }));

        assert!(inclusions_txid1.contains(&TxInclusionData {
            blue_score: blue_score2,
            block_hash: block_hash2,
            index_within_block: index_within_block2,
        }));

        // Retrieve included transaction data for txid2
        let inclusions_txid2 = store.get_transaction_inclusion_data(txid2).unwrap();
        assert_eq!(inclusions_txid2.len(), 1);
        assert_eq!(
            inclusions_txid2[0],
            TxInclusionData { blue_score: blue_score1, block_hash: block_hash1, index_within_block: index_within_block1 }
        );

        // Test removal and clean up
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.remove_transaction_inclusion_data(writer, txid1, blue_score1).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert!(store.get_transaction_inclusion_data(txid1).is_ok_and(|val| val.len() == 1
            && val[0]
                == TxInclusionData { blue_score: blue_score2, block_hash: block_hash2, index_within_block: index_within_block2 }));
        let mut write_batch = WriteBatch::new();
        let writer = BatchDbWriter::new(&mut write_batch);
        store.remove_transaction_inclusion_data(writer, txid1, blue_score2).unwrap();
        txindex_db.write(write_batch).unwrap();
        let res = store.get_transaction_inclusion_data(txid1);
        println!("{:?}", res);
        assert!(store.get_transaction_inclusion_data(txid1).is_ok_and(|val| val.is_empty()));
        store.delete_all().unwrap();
        assert!(store.get_transaction_inclusion_data(txid2).is_ok_and(|val| val.is_empty()));
    }
}
