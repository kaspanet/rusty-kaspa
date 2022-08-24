use moka::sync::Cache;
use std::{cmp::Reverse, collections::BinaryHeap, sync::Arc};

use crate::processes::ghostdag::ordering::SortableBlock;

use hashes::Hash;

/// Reader API for `BlockWindowCacheStore`.
pub trait BlockWindowCacheReader {
    fn get(&self, hash: &Hash) -> Option<Arc<BinaryHeap<Reverse<SortableBlock>>>>;
}

pub type BlockWindowCacheStore = Cache<Hash, Arc<BinaryHeap<Reverse<SortableBlock>>>>;

impl BlockWindowCacheReader for BlockWindowCacheStore {
    #[inline(always)]
    fn get(&self, hash: &Hash) -> Option<Arc<BinaryHeap<Reverse<SortableBlock>>>> {
        self.get(hash)
    }
}
