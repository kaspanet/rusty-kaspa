use crate::processes::ghostdag::ordering::SortableBlock;
use kaspa_consensus_core::BlockHasher;
use kaspa_database::prelude::Cache;
use kaspa_hashes::Hash;
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    ops::{Deref, DerefMut},
    sync::Arc,
};

#[derive(Clone, Copy)]
pub enum WindowOrigin {
    Full,
    Sampled,
}

#[derive(Clone)]
pub struct BlockWindowHeap {
    pub blocks: BinaryHeap<Reverse<SortableBlock>>,
    origin: WindowOrigin,
}

impl BlockWindowHeap {
    pub fn new(origin: WindowOrigin) -> Self {
        Self { blocks: Default::default(), origin }
    }

    pub fn with_capacity(origin: WindowOrigin, capacity: usize) -> Self {
        Self { blocks: BinaryHeap::with_capacity(capacity), origin }
    }

    #[inline]
    #[must_use]
    pub fn origin(&self) -> WindowOrigin {
        self.origin
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

/// Reader API for `BlockWindowCacheStore`.
pub trait BlockWindowCacheReader {
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>>;
}

pub type BlockWindowCacheStore = Cache<Hash, Arc<BlockWindowHeap>, BlockHasher>;

impl BlockWindowCacheReader for BlockWindowCacheStore {
    #[inline(always)]
    fn get(&self, hash: &Hash) -> Option<Arc<BlockWindowHeap>> {
        self.get(hash)
    }
}
