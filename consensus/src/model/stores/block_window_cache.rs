use crate::processes::ghostdag::ordering::SortableBlock;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::{Cache, CachePolicy};
use kaspa_hashes::Hash;
use kaspa_utils::mem_size::MemSizeEstimator;
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Default)]
pub struct BlockWindowHeap {
    pub blocks: BinaryHeap<Reverse<SortableBlock>>,
}

impl MemSizeEstimator for BlockWindowHeap {}

impl BlockWindowHeap {
    pub fn new() -> Self {
        Self { blocks: Default::default() }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self { blocks: BinaryHeap::with_capacity(capacity) }
    }
}

impl Deref for BlockWindowHeap {
    type Target = BinaryHeap<Reverse<SortableBlock>>;

    fn deref(&self) -> &Self::Target {
        &self.blocks
    }
}

impl DerefMut for BlockWindowHeap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.blocks
    }
}

/// A newtype wrapper over `[Cache]` meant to prevent erroneous reads of windows from different origins
#[derive(Clone)]
pub struct BlockWindowCacheStore {
    inner: Cache<Hash, Arc<BlockWindowHeap>, BlockHasher>,
}

impl BlockWindowCacheStore {
    pub fn new(policy: CachePolicy) -> Self {
        Self { inner: Cache::new(policy) }
    }

    pub fn contains_key(&self, key: &Hash) -> bool {
        self.inner.contains_key(key)
    }

    pub fn remove(&self, key: &Hash) -> Option<Arc<BlockWindowHeap>> {
        self.inner.remove(key)
    }
}

/// Reader API for `BlockWindowCacheStore`.
pub trait BlockWindowCacheReader {
    /// Get the cache entry to this hash conditioned that *it matches the provided origin*.
    /// We demand the origin to be provided in order to prevent reader errors.
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>>;
}

impl BlockWindowCacheReader for BlockWindowCacheStore {
    #[inline(always)]
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>> {
        self.inner.get(hash)
    }
}

impl<U: BlockWindowCacheReader> BlockWindowCacheReader for Option<&Arc<U>> {
    #[inline(always)]
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>> {
        self.and_then(|inner| inner.get(hash))
    }
}

pub trait BlockWindowCacheWriter {
    fn insert(&self, hash: Hash, window: Arc<BlockWindowHeap>);
}

impl BlockWindowCacheWriter for BlockWindowCacheStore {
    fn insert(&self, hash: Hash, window: Arc<BlockWindowHeap>) {
        self.inner.insert(hash, window);
    }
}
