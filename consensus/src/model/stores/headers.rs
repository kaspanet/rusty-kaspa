use std::sync::Arc;

use super::{
    caching::{CachedDbAccess, CachedDbAccessForCopy},
    errors::StoreError,
    DB,
};
use consensus_core::header::Header;
use hashes::Hash;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};

pub trait HeaderStoreReader {
    fn get_daa_score(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_timestamp(&self, hash: Hash) -> Result<u64, StoreError>;
    fn get_bits(&self, hash: Hash) -> Result<u32, StoreError>;
    fn get_header(&self, hash: Hash) -> Result<Arc<Header>, StoreError>;
    fn get_header_with_block_level(&self, hash: Hash) -> Result<Arc<HeaderWithBlockLevel>, StoreError>;
    fn get_compact_header_data(&self, hash: Hash) -> Result<CompactHeaderData, StoreError>;
}

#[derive(Clone, Serialize, Deserialize)]
pub struct HeaderWithBlockLevel {
    header: Arc<Header>,
    block_level: u8,
}

pub trait HeaderStore: HeaderStoreReader {
    // This is append only
    fn insert(&self, hash: Hash, header: Arc<Header>, block_level: u8) -> Result<(), StoreError>;
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
    raw_db: Arc<DB>,
    // `CachedDbAccess` is shallow cloned so no need to wrap with Arc
    cached_compact_headers_access: CachedDbAccessForCopy<Hash, CompactHeaderData>,
    cached_headers_access: CachedDbAccess<Hash, HeaderWithBlockLevel>,
}

impl DbHeadersStore {
    pub fn new(db: Arc<DB>, cache_size: u64) -> Self {
        Self {
            raw_db: Arc::clone(&db),
            cached_compact_headers_access: CachedDbAccessForCopy::new(Arc::clone(&db), cache_size, COMPACT_HEADER_DATA_STORE_PREFIX),
            cached_headers_access: CachedDbAccess::new(db, cache_size, HEADERS_STORE_PREFIX),
        }
    }

    pub fn clone_with_new_cache(&self, cache_size: u64) -> Self {
        Self::new(Arc::clone(&self.raw_db), cache_size)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, hash: Hash, header: Arc<Header>, block_level: u8) -> Result<(), StoreError> {
        if self.cached_headers_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_headers_access.write_batch(
            batch,
            hash,
            &Arc::new(HeaderWithBlockLevel { header: header.clone(), block_level }),
        )?;
        self.cached_compact_headers_access.write_batch(
            batch,
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
        if let Some(header_with_block_level) = self.cached_headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.daa_score);
        }
        Ok(self.cached_compact_headers_access.read(hash)?.daa_score)
    }

    fn get_blue_score(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.cached_headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.blue_score);
        }
        Ok(self.cached_compact_headers_access.read(hash)?.blue_score)
    }

    fn get_timestamp(&self, hash: Hash) -> Result<u64, StoreError> {
        if let Some(header_with_block_level) = self.cached_headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.timestamp);
        }
        Ok(self.cached_compact_headers_access.read(hash)?.timestamp)
    }

    fn get_bits(&self, hash: Hash) -> Result<u32, StoreError> {
        if let Some(header_with_block_level) = self.cached_headers_access.read_from_cache(hash) {
            return Ok(header_with_block_level.header.bits);
        }
        Ok(self.cached_compact_headers_access.read(hash)?.bits)
    }

    fn get_header(&self, hash: Hash) -> Result<Arc<Header>, StoreError> {
        Ok(self.cached_headers_access.read(hash)?.header.clone())
    }

    fn get_header_with_block_level(&self, hash: Hash) -> Result<Arc<HeaderWithBlockLevel>, StoreError> {
        self.cached_headers_access.read(hash)
    }

    fn get_compact_header_data(&self, hash: Hash) -> Result<CompactHeaderData, StoreError> {
        if let Some(header_with_block_level) = self.cached_headers_access.read_from_cache(hash) {
            return Ok(CompactHeaderData {
                daa_score: header_with_block_level.header.daa_score,
                timestamp: header_with_block_level.header.timestamp,
                bits: header_with_block_level.header.bits,
                blue_score: header_with_block_level.header.blue_score,
            });
        }
        self.cached_compact_headers_access.read(hash)
    }
}

impl HeaderStore for DbHeadersStore {
    fn insert(&self, hash: Hash, header: Arc<Header>, block_level: u8) -> Result<(), StoreError> {
        if self.cached_headers_access.has(hash)? {
            return Err(StoreError::KeyAlreadyExists(hash.to_string()));
        }
        self.cached_compact_headers_access.write(
            hash,
            CompactHeaderData {
                daa_score: header.daa_score,
                timestamp: header.timestamp,
                bits: header.bits,
                blue_score: header.blue_score,
            },
        )?;
        self.cached_headers_access.write(hash, &Arc::new(HeaderWithBlockLevel { header, block_level }))?;
        Ok(())
    }
}
