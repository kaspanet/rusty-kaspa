use std::sync::Arc;

use consensus_core::{header::Header, BlockHasher, BlockLevel};
use database::prelude::DB;
use database::prelude::{BatchDbWriter, CachedDbAccess, DirectDbWriter};
use database::prelude::{StoreError, StoreResult};
use hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

pub trait HeaderStoreReader {
    fn get_daa_score(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_timestamp(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_bits(&self, hash: Hash) -> Result<u32, StoreError>;
    fn get_header(&self, hash: Hash) -> Result<Arc<Header>, StoreError>;
    fn get_header_with_block_level(&self, hash: Hash) -> Result<HeaderWithBlockLevel, StoreError>;
    fn get_compact_header_data(&self, hash: Hash) -> Result<CompactHeaderData, StoreError>;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HeaderWithBlockLevel {
    pub header: Arc<Header>,
    pub block_level: BlockLevel,
}

pub trait HeaderStore: HeaderStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, header: Arc<Header>, block_level: BlockLevel) -> Result<(), StoreError>;
}

const HEADERS_STORE_PREFIX: &[u8] = b"headers";
const COMPACT_HEADER_DATA_STORE_PREFIX: &[u8] = b"compact-header-data";

#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct CompactHeaderData {
    pub daa_score: u64,
    pub timestamp: u64,
    pub bits: u32,
    pub blue_score: u64,
}

/// A DB + cache implementation of `HeaderStore` trait, with concurrency support.
#[derive(Clone)]
pub struct DbHeadersStore {
    db: Arc<DB>,
    compact_headers_access: CachedDbAccess<Hash, CompactHeaderData, BlockHasher>,
    headers_access: CachedDbAccess<Hash, HeaderWithBlockLevel, BlockHasher>,
}

impl DbHeadersStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            db: Arc::clone(&db),
            compact_headers_access: CachedDbAccess::new(Arc::clone(&db), cache_size, COMPACT_HEADER_DATA_STORE_PREFIX.to_vec()),
            headers_access: CachedDbAccess::new(db, cache_size, HEADERS_STORE_PREFIX.to_vec()),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.db), cache_size)
    }

    pub fn has(&self, hash: Hash) -> StoreResult<bool> {
        self.headers_access.has(hash)
    }

    pub fn insert_batch(
        &self,
        batch: &mut WriteBatch,
        hash: Hash,
        header: Arc<Header>,
        block_level: BlockLevel,
    ) -> Result<(), StoreError> {
        if self.headers_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.headers_access.write(BatchDbWriter::new(batch), hash, HeaderWithBlockLevel { header: header.clone(), block_level })?;
        self.compact_headers_access.write(
            BatchDbWriter::new(batch),
            hash,
            CompactHeaderData {
                daa_score: header.daa_score,
                timestamp: header.timestamp,
                bits: header.bits,
                blue_score: header.blue_score,
            },
        )?;
        Ok(())
    }
}

impl HeaderStoreReader for DbHeadersStore {
    fn get_daa_score(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.daa_score);
        }
        Ok(self.compact_headers_access.read(hash)?.daa_score)
    }

    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.blue_score);
        }
        Ok(self.compact_headers_access.read(hash)?.blue_score)
    }

    fn get_timestamp(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.timestamp);
        }
        Ok(self.compact_headers_access.read(hash)?.timestamp)
    }

    fn get_bits(&self, hash: Hash) -> Result<u32, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.bits);
        }
        Ok(self.compact_headers_access.read(hash)?.bits)
    }

    fn get_header(&self, hash: Hash) -> Result<Arc<Header>, StoreError> {
        Ok(self.headers_access.read(hash)?.header)
    }

    fn get_header_with_block_level(&self, hash: Hash) -> Result<HeaderWithBlockLevel, StoreError> {
        self.headers_access.read(hash)
    }

    fn get_compact_header_data(&self, hash: Hash) -> Result<CompactHeaderData, StoreError> {
        if let Some(header_with_block_level) = self.headers_access.read_from_cache(hash) {
            return Ok(CompactHeaderData {
                daa_score: header_with_block_level.header.daa_score,
                timestamp: header_with_block_level.header.timestamp,
                bits: header_with_block_level.header.bits,
                blue_score: header_with_block_level.header.blue_score,
            });
        }
        self.compact_headers_access.read(hash)
    }
}

impl HeaderStore for DbHeadersStore {
    fn insert(&self, hash: Hash, header: Arc<Header>, block_level: u8) -> Result<(), StoreError> {
        if self.headers_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.compact_headers_access.write(
            DirectDbWriter::new(&self.db),
            hash,
            CompactHeaderData {
                daa_score: header.daa_score,
                timestamp: header.timestamp,
                bits: header.bits,
                blue_score: header.blue_score,
            },
        )?;
        self.headers_access.write(DirectDbWriter::new(&self.db), hash, HeaderWithBlockLevel { header, block_level })?;
        Ok(())
    }
}
