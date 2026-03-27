use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, CachedDbAccess, DB, DirectDbWriter, StoreResult};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use rocksdb::WriteBatch;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Per-block SMT metadata stored alongside the seq_commit.
///
/// `payload_and_ctx_digest` is `H_seq(context_hash, payload_root)` — the inner hash
/// of `seq_state_root`. Combined with `lanes_root` and `parent_seq_commit`,
/// the full seq_commit can be verified without access to block transactions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmtBlockMetadata {
    pub lanes_root: Hash,
    pub payload_and_ctx_digest: Hash,
    pub active_lanes_count: u64,
}

impl SmtBlockMetadata {
    pub fn new(lanes_root: Hash, payload_and_ctx_digest: Hash, active_lanes_count: u64) -> Self {
        Self { lanes_root, payload_and_ctx_digest, active_lanes_count }
    }
}

impl MemSizeEstimator for SmtBlockMetadata {}

/// Block-hash-keyed metadata store with in-memory cache.
#[derive(Clone)]
pub struct DbSmtMetadataStore {
    db: Arc<DB>,
    access: CachedDbAccess<Hash, SmtBlockMetadata, BlockHasher>,
}

impl DbSmtMetadataStore {
    pub fn new(db: Arc<DB>, cache_policy: CachePolicy) -> Self {
        Self { access: CachedDbAccess::new(Arc::clone(&db), cache_policy, DatabaseStorePrefixes::SmtSeqCommitMeta.into()), db }
    }

    pub fn get(&self, block_hash: Hash) -> StoreResult<SmtBlockMetadata> {
        self.access.read(block_hash)
    }

    pub fn has(&self, block_hash: Hash) -> StoreResult<bool> {
        self.access.has(block_hash)
    }

    pub fn insert_batch(&self, batch: &mut WriteBatch, block_hash: Hash, metadata: SmtBlockMetadata) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), block_hash, metadata)
    }

    pub fn delete_all(&self) -> StoreResult<()> {
        self.access.delete_all(DirectDbWriter::new(&self.db))
    }
}
