use crate::model::transactions::TxInclusionData;

use kaspa_consensus_core::{
    tx::{TransactionId, TransactionIndexType},
    Hash,
};
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DbWriter, DirectDbWriter, StoreResult, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use std::mem;
use std::sync::Arc;

// --- Types, Constants, Structs, Enums ---

// Field size constants
pub const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
pub const HASH_SIZE: usize = mem::size_of::<Hash>(); // 32
pub const DAA_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
pub const TRANSACTION_STORE_KEY_LEN: usize = TRANSACTION_ID_SIZE + DAA_SCORE_SIZE + HASH_SIZE; // 72

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IncludedTransactionStoreKey(pub [u8; TRANSACTION_STORE_KEY_LEN]);

impl IncludedTransactionStoreKey {
    #[inline(always)]
    pub fn new_minimized() -> Self {
        Self([0u8; TRANSACTION_STORE_KEY_LEN])
    }

    #[inline(always)]
    pub fn new_maximized() -> Self {
        Self([u8::MAX; TRANSACTION_STORE_KEY_LEN])
    }

    #[inline(always)]
    pub fn with_txid(mut self, txid: TransactionId) -> Self {
        self.0[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        self
    }

    #[inline(always)]
    pub fn with_daa_score(mut self, daa_score: u64) -> Self {
        self.0[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE].copy_from_slice(&daa_score.to_be_bytes());
        self
    }

    #[inline(always)]
    pub fn with_block_hash(mut self, hash: Hash) -> Self {
        self.0[TRANSACTION_ID_SIZE + DAA_SCORE_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE + HASH_SIZE]
            .copy_from_slice(&hash.as_bytes());
        self
    }

    #[inline(always)]
    pub fn daa_score(&self) -> u64 {
        u64::from_be_bytes(self.0[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE].try_into().unwrap())
    }

    #[inline(always)]
    pub fn block_hash(&self) -> Hash {
        Hash::from_slice(&self.0[TRANSACTION_ID_SIZE + DAA_SCORE_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE + HASH_SIZE])
    }

    #[inline(always)]
    pub fn txid(&self) -> TransactionId {
        TransactionId::from_slice(&self.0[0..TRANSACTION_ID_SIZE])
    }
}

impl Default for IncludedTransactionStoreKey {
    #[inline(always)]
    fn default() -> Self {
        Self([0u8; TRANSACTION_STORE_KEY_LEN])
    }
}

impl From<(IncludedTransactionStoreKey, TransactionIndexType)> for TxInclusionData {
    #[inline(always)]
    fn from(item: (IncludedTransactionStoreKey, TransactionIndexType)) -> Self {
        let (key, index_within_block) = item;
        let bytes = &key.0;
        let daa_score = u64::from_be_bytes(bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE].try_into().unwrap());
        let block_hash =
            Hash::from_slice(&bytes[TRANSACTION_ID_SIZE + DAA_SCORE_SIZE..TRANSACTION_ID_SIZE + DAA_SCORE_SIZE + HASH_SIZE]);
        Self { daa_score, block_hash, index_within_block }
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
    fn remove_transaction_inclusion_data(
        &mut self,
        writer: &mut impl DbWriter,
        txid: TransactionId,
        daa_score: u64,
    ) -> StoreResult<()>;
    fn add_included_transaction_data<I>(&mut self, writer: &mut impl DbWriter, to_add: TxInclusionIter<I>) -> StoreResult<()>
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
                Some(IncludedTransactionStoreKey::new_minimized().with_txid(txid)),
                Some(IncludedTransactionStoreKey::new_maximized().with_txid(txid)),
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
    fn remove_transaction_inclusion_data(
        &mut self,
        writer: &mut impl DbWriter,
        txid: TransactionId,
        daa_score: u64,
    ) -> StoreResult<()> {
        self.access.delete_range(
            writer,
            IncludedTransactionStoreKey::new_minimized().with_txid(txid).with_daa_score(daa_score),
            IncludedTransactionStoreKey::new_maximized().with_txid(txid).with_daa_score(daa_score),
        )
    }

    fn add_included_transaction_data<I>(&mut self, writer: &mut impl DbWriter, to_add: TxInclusionIter<I>) -> StoreResult<()>
    where
        I: Iterator<Item = TxInclusionTuple>,
    {
        let kv_iter =
            to_add.map(|(a, b, c, d)| (IncludedTransactionStoreKey::default().with_txid(a).with_daa_score(b).with_block_hash(c), d));

        self.access.write_many_without_cache(writer, &mut kv_iter.into_iter())
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
        prelude::{BatchDbWriter, ConnBuilder, WriteBatch},
    };

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
        let txid = TransactionId::from_u64_word(1);
        let daa_score = 987654321u64;
        let block_hash = Hash::from_u64_word(2);
        let key = IncludedTransactionStoreKey::default().with_txid(txid).with_daa_score(daa_score).with_block_hash(block_hash);
        let index_within_block = 42 as TransactionIndexType;
        let tx_inclusion_data: TxInclusionData = (key.clone(), index_within_block).into();
        assert_eq!(tx_inclusion_data.daa_score, daa_score);
        assert_eq!(tx_inclusion_data.block_hash, block_hash);
        assert_eq!(tx_inclusion_data.index_within_block, index_within_block);
    }

    #[test]
    fn test_included_transactions_store() {
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let mut store = DbTxIndexIncludedTransactionsStore::new(Arc::clone(&txindex_db), CachePolicy::Empty);
        let txid1 = TransactionId::from_u64_word(1);
        let txid2 = TransactionId::from_u64_word(2);
        let block_hash1 = Hash::from_u64_word(3);
        let block_hash2 = Hash::from_u64_word(4);
        let daa_score1 = 100u64;
        let daa_score2 = 200u64;
        let index_within_block1 = 1 as TransactionIndexType;
        let index_within_block2 = 2 as TransactionIndexType;

        // Add included transaction data
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store
            .add_included_transaction_data(
                &mut writer,
                TxInclusionIter(
                    vec![
                        (txid1, daa_score1, block_hash1, index_within_block1),
                        (txid1, daa_score2, block_hash2, index_within_block2),
                        (txid2, daa_score1, block_hash1, index_within_block1),
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
            daa_score: daa_score1,
            block_hash: block_hash1,
            index_within_block: index_within_block1,
        }));

        assert!(inclusions_txid1.contains(&TxInclusionData {
            daa_score: daa_score2,
            block_hash: block_hash2,
            index_within_block: index_within_block2,
        }));

        // Retrieve included transaction data for txid2
        let inclusions_txid2 = store.get_transaction_inclusion_data(txid2).unwrap();
        assert_eq!(inclusions_txid2.len(), 1);
        assert_eq!(
            inclusions_txid2[0],
            TxInclusionData { daa_score: daa_score1, block_hash: block_hash1, index_within_block: index_within_block1 }
        );

        // Test removal and clean up
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.remove_transaction_inclusion_data(&mut writer, txid1, daa_score1).unwrap();
        txindex_db.write(write_batch).unwrap();
        assert!(store.get_transaction_inclusion_data(txid1).is_ok_and(|val| val.len() == 1
            && val[0] == TxInclusionData { daa_score: daa_score2, block_hash: block_hash2, index_within_block: index_within_block2 }));
        let mut write_batch = WriteBatch::new();
        let mut writer = BatchDbWriter::new(&mut write_batch);
        store.remove_transaction_inclusion_data(&mut writer, txid1, daa_score2).unwrap();
        txindex_db.write(write_batch).unwrap();
        let res = store.get_transaction_inclusion_data(txid1);
        println!("{:?}", res);
        assert!(store.get_transaction_inclusion_data(txid1).is_ok_and(|val| val.is_empty()));
        store.delete_all().unwrap();
        assert!(store.get_transaction_inclusion_data(txid2).is_ok_and(|val| val.is_empty()));
    }
}
