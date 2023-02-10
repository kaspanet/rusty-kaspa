use crate::processes::ghostdag::ordering::SortableBlock;
use consensus_core::BlockHasher;
use database::prelude::Cache;
use hashes::Hash;
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

pub type BlockWindowHeap = BinaryHeap<Reverse<SortableBlock>>;

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
