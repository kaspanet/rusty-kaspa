use std::sync::Arc;

use super::{
    caching::{BatchDbWriter, CachedDbItem, DirectDbWriter},
    errors::StoreResult,
    DB,
};
use consensus_core::BlockHashSet;
use hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `TipsStore`.
pub trait TipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait TipsStore: TipsStoreReader {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<BlockHashSet>>;
}

/// A DB + cache implementation of `VirtualStateStore` trait
#[derive(Clone)]
pub struct DbTipsStore {
    raw_db: Arc<DB>,
    prefix: &'static [u8],
    cached_access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbTipsStore {
    pub fn new(db: Arc<DB>, prefix: &'static [u8]) -> Self {
        Self { raw_db: Arc::clone(&db), cached_access: CachedDbItem::new(db.clone(), prefix), prefix }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.raw_db), self.prefix)
    }

    pub fn init_batch(&mut self, batch: &mut WriteBatch, genesis: Hash) -> StoreResult<()> {
        self.cached_access.write_batch(batch, &Arc::new(BlockHashSet::from([genesis])))
    }

    pub fn add_tip_batch(
        &mut self,
        batch: &mut WriteBatch,
        new_tip: Hash,
        new_tip_parents: &[Hash],
    ) -> StoreResult<Arc<BlockHashSet>> {
        self.cached_access.update(&mut BatchDbWriter::new(batch), |tips| update_tips(tips, new_tip_parents, new_tip))
    }
}

/// Updates the internal data if possible
fn update_tips(current_tips: Arc<BlockHashSet>, new_tip_parents: &[Hash], new_tip: Hash) -> Arc<BlockHashSet> {
    let mut tips = Arc::try_unwrap(current_tips).unwrap_or_else(|arc| (*arc).clone());
    for parent in new_tip_parents {
        tips.remove(parent);
    }
    tips.insert(new_tip);
    Arc::new(tips)
}

impl TipsStoreReader for DbTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.cached_access.read()
    }
}

impl TipsStore for DbTipsStore {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<BlockHashSet>> {
        self.cached_access.update(&mut DirectDbWriter::new(&self.raw_db), |tips| update_tips(tips, new_tip_parents, new_tip))
    }
}
