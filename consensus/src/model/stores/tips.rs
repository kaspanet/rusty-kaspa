use std::sync::Arc;

use super::{caching::CachedDbItem, errors::StoreResult, DB};
use consensus_core::BlockHashSet;
use hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `TipsStore`.
pub trait TipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait TipsStore: TipsStoreReader {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<()>;
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

    pub fn add_tip_batch(&mut self, batch: &mut WriteBatch, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<()> {
        let tips = self.read_and_update_tips(new_tip, new_tip_parents)?;
        self.cached_access.write_batch(batch, &Arc::new(tips))
    }

    fn read_and_update_tips(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<BlockHashSet> {
        let mut tips = self.cached_access.read()?.as_ref().clone();
        for parent in new_tip_parents {
            tips.remove(parent);
        }
        tips.insert(new_tip);
        Ok(tips)
    }
}

impl TipsStoreReader for DbTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.cached_access.read()
    }
}

impl TipsStore for DbTipsStore {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<()> {
        let tips = self.read_and_update_tips(new_tip, new_tip_parents)?;
        self.cached_access.write(&Arc::new(tips))
    }
}
