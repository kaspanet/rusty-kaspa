use crate::model::transactions::TxAcceptanceData;

use kaspa_consensus_core::{acceptance_data::MergesetIndexType, tx::TransactionId, Hash};
use kaspa_database::prelude::{CachePolicy, CachedDbAccess, StoreResult, DB};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_utils::mem_size::MemSizeEstimator;
use std::mem;
use std::sync::Arc;

// --- Types, Constants, Structs, Enums ---

pub type TransactionRemovalIter = Box<dyn Iterator<Item = (TransactionId, u64, Hash)>>;
pub type TransactionAcceptedIter = Box<dyn Iterator<Item = (TransactionId, Hash, MergesetIndexType)>>;

// Field size constants
pub const TRANSACTION_ID_SIZE: usize = mem::size_of::<TransactionId>(); // 32
pub const HASH_SIZE: usize = mem::size_of::<Hash>(); // 32
pub const BLUE_SCORE_SIZE: usize = mem::size_of::<u64>(); // 8
pub const TRANSACTION_STORE_KEY_LEN: usize = TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE; // 72

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AcceptedTransactionStoreKey(pub [u8; TRANSACTION_STORE_KEY_LEN]);

impl AcceptedTransactionStoreKey {
    #[inline(always)]
    pub fn from_parts(txid: &TransactionId, blue_score: u64, hash: &Hash) -> Self {
        let mut bytes = [0u8; TRANSACTION_STORE_KEY_LEN];
        bytes[0..TRANSACTION_ID_SIZE].copy_from_slice(&txid.as_bytes());
        bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].copy_from_slice(&blue_score.to_be_bytes()); // to_be_bytes important for rocks db ordering
        bytes[TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE]
            .copy_from_slice(&hash.as_bytes());
        Self(bytes)
    }
}

impl From<(AcceptedTransactionStoreKey, MergesetIndexType)> for TxAcceptanceData {
    #[inline(always)]
    fn from(item: (AcceptedTransactionStoreKey, MergesetIndexType)) -> Self {
        let (key, mergeset_idx) = item;
        // Acceptance: blue_score, block_hash, mergeset_idx
        let bytes = &key.0;
        let blue_score = u64::from_be_bytes(bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE].try_into().unwrap());
        let block_hash = Hash::from_slice(&bytes[TRANSACTION_ID_SIZE..TRANSACTION_ID_SIZE + BLUE_SCORE_SIZE + HASH_SIZE]);
        Self { blue_score, block_hash, mergeset_idx }
    }
}

impl MemSizeEstimator for AcceptedTransactionStoreKey {}

impl AsRef<[u8]> for AcceptedTransactionStoreKey {
    #[inline(always)]
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// -- Store Traits ---

pub trait TxIndexTransactionsStoreReader {
    fn get_transaction_acceptance_data(txid: TransactionId) -> StoreResult<Vec<TxAcceptanceData>>;
}

pub trait TxIndexTransactionsStore: TxIndexTransactionsStoreReader {
    fn remove_transaction_acceptance_data(to_remove: TransactionRemovalIter) -> StoreResult<()>;
    fn add_accepted_transaction_data(to_add: TransactionAcceptedIter) -> StoreResult<()>;
}

// --- Store Implementation ---

pub struct DbTxIndexTransactionsStore {
    db: Arc<DB>,
    access: CachedDbAccess<AcceptedTransactionStoreKey, MergesetIndexType>,
}

impl DbTxIndexTransactionsStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self {
            db: Arc::clone(&db),
            access: CachedDbAccess::new(db, cache_policy, DatabaseStorePrefixes::TransactionAcceptanceData.into()),
        }
    }
}

impl TxIndexTransactionsStoreReader for DbTxIndexTransactionsStore {
    fn get_transaction_acceptance_data(_txid: TransactionId) -> StoreResult<Vec<TxAcceptanceData>> {
        todo!()
    }
}

impl TxIndexTransactionsStore for DbTxIndexTransactionsStore {
    fn remove_transaction_acceptance_data(_to_remove: TransactionRemovalIter) -> StoreResult<()> {
        todo!()
    }

    fn add_accepted_transaction_data(_to_add: TransactionAcceptedIter) -> StoreResult<()> {
        todo!()
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
        let key = AcceptedTransactionStoreKey::from_parts(&txid, blue_score, &block_hash);
        let mergeset_idx = 42 as MergesetIndexType;
        let tx_acceptance_data: TxAcceptanceData = (key.clone(), mergeset_idx).into();
        assert_eq!(tx_acceptance_data.blue_score, blue_score);
        assert_eq!(tx_acceptance_data.block_hash, block_hash);
        assert_eq!(tx_acceptance_data.mergeset_idx, mergeset_idx);
    }
}
