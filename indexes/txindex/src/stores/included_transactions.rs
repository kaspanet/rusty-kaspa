use crate::model::transactions::TxInclusionData;

use kaspa_consensus_core::{
    tx::{TransactionId, TransactionIndexType},
    Hash,
};
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, DirectDbWriter, StoreResult, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use std::mem;
use std::sync::Arc;

// --- Types, Constants, Structs, Enums ---

pub type TransactionRemovalIter = Box<dyn Iterator<Item = (TransactionId, u64)>>;
pub type TransactionIncludedIter = Box<dyn Iterator<Item = (TransactionId, u64, Hash, TransactionIndexType)>>;

// Field size constants
pub const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
pub const HASH_SIZE: usize = mem::size_of::<Hash>(); // 32
pub const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
pub const TRANSACTION_STORE_KEY_LEN: usize = TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE; // 72

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

    pub fn from_tx_id_maximized(txid: TransactionId) -> Self {
        let mut bytes = [u8::MAX; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    pub fn from_tx_id_minimized(txid: TransactionId) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        Self(bytes)
    }

    pub fn from_tx_id_and_blue_score_maximized(txid: TransactionId, blue_score: u64) -> Self {
        let mut bytes = [u8::MAX; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes());
        Self(bytes)
    }

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

/// -- Store Traits ---

pub trait TxIndexIncludedTransactionsStoreReader {
    fn get_transaction_inclusion_data(&self, txid: TransactionId) -> StoreResult<Vec<TxInclusionData>>;
}

pub trait TxIndexIncludedTransactionsStore: TxIndexIncludedTransactionsStoreReader {
    fn remove_transaction_inclusion_data(&mut self, txid: TransactionId, blue_score: u64) -> StoreResult<()>;
    fn add_included_transaction_data<I>(&mut self, to_add: I) -> StoreResult<()>
    where
        I: Iterator<Item = (TransactionId, u64, Hash, TransactionIndexType)>;
}

/// --- Store Implementation ---

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
    fn remove_transaction_inclusion_data(&mut self, txid: TransactionId, blue_score: u64) -> StoreResult<()> {
        self.access.delete_range(
            DirectDbWriter::new(&self.db),
            IncludedTransactionStoreKey::from_tx_id_and_blue_score_maximized(txid, blue_score),
            IncludedTransactionStoreKey::from_tx_id_and_blue_score_minimized(txid, blue_score),
        )
    }

    fn add_included_transaction_data<I>(&mut self, to_add: I) -> StoreResult<()>
    where
        I: Iterator<Item = (TransactionId, u64, Hash, TransactionIndexType)>,
    {
        self.access.write_many_without_cache(
            DirectDbWriter::new(&self.db),
            &mut to_add.map(|(txid, blue_score, block_hash, index_within_block)| {
                (IncludedTransactionStoreKey::from_parts(txid, blue_score, block_hash), index_within_block)
            }),
        )
    }
}

/// --- Tests ---
#[cfg(test)]
mod tests {
    use super::*;
    use bincode;
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
}
