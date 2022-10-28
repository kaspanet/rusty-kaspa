use consensus_core::BlockHasher;
use parking_lot::{RwLock, RwLockWriteGuard};
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use super::{
    database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter},
    errors::{StoreError, StoreResult},
    DB,
};
use hashes::Hash;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum BlockStatus {
    /// StatusInvalid indicates that the block is invalid.
    StatusInvalid,

    /// StatusUTXOValid indicates the block is valid from any UTXO related aspects and has passed all the other validations as well.
    StatusUTXOValid,

    /// StatusUTXOPendingVerification indicates that the block is pending verification against its past UTXO-Set, either
    /// because it was not yet verified since the block was never in the selected parent chain, or if the
    /// block violates finality.
    StatusUTXOPendingVerification,

    /// StatusDisqualifiedFromChain indicates that the block is not eligible to be a selected parent.
    StatusDisqualifiedFromChain,

    /// StatusHeaderOnly indicates that the block transactions are not held (pruned or wasn't added yet)
    StatusHeaderOnly,
}

impl BlockStatus {
    pub fn has_block_body(self) -> bool {
        matches!(self, Self::StatusUTXOValid | Self::StatusUTXOPendingVerification | Self::StatusDisqualifiedFromChain)
    }

    pub fn is_utxo_valid_or_pending(self) -> bool {
        matches!(self, Self::StatusUTXOValid | Self::StatusUTXOPendingVerification)
    }
}

/// Reader API for `StatusesStore`.
pub trait StatusesStoreReader {
    fn get(&self, hash: Hash) -> StoreResult<BlockStatus>;
    fn has(&self, hash: Hash) -> StoreResult<bool>;
}

/// Write API for `StatusesStore`. The set function is deliberately `mut`
/// since status is not append-only and thus needs to be guarded.
/// TODO: can be optimized to avoid the locking if needed.
pub trait StatusesStore: StatusesStoreReader {
    fn set(&mut self, hash: Hash, status: BlockStatus) -> StoreResult<()>;
}

const STORE_PREFIX: &[u8] = b"block-statuses";

/// A DB + cache implementation of `StatusesStore` trait, with concurrent readers support.
#[derive(Clone)]
pub struct DbStatusesStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, BlockStatus, BlockHasher>,
}

impl DbStatusesStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbAccess::new(db, cache_size, STORE_PREFIX) }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }
}

pub trait StatusesStoreBatchExtensions {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError>;
}

impl StatusesStoreBatchExtensions for Arc<RwLock<DbStatusesStore>> {
    fn set_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        status: BlockStatus,
    ) -> Result<RwLockWriteGuard<DbStatusesStore>, StoreError> {
        let write_guard = self.write();
        write_guard.access.write(BatchDbWriter::new(batch), hash, status)?;
        Ok(write_guard)
    }
}

impl StatusesStoreReader for DbStatusesStore {
    fn get(&self, hash: Hash) -> StoreResult<BlockStatus> {
        self.access.read(hash)
    }

    fn has(&self, hash: Hash) -> StoreResult<bool> {
        self.access.has(hash)
    }
}

impl StatusesStore for DbStatusesStore {
    fn set(&mut self, hash: Hash, status: BlockStatus) -> StoreResult<()> {
        self.access.write(DirectDbWriter::new(&self.db), hash, status)
    }
}
