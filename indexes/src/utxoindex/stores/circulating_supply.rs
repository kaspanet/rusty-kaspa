//ToDo: this file is a 1:1 copy paste of 

use std::sync::Arc;
use consensus::model::stores::database::prelude::*;
use consensus::model::stores::errors::*;
use consensus::model::stores::DB;
use consensus_core::tip::Tips;
use rocksdb::WriteBatch;

/// Reader API for `TipsStore`.
pub trait TipsStoreReader {
    fn get(&self) -> StoreResult<Arc<Tips>>;
}

pub trait TipsStore: TipsStoreReader {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<Tips>>;
}

pub const STORE_NAME: &[u8] = b"body-tips";

/// A DB + cache implementation of `TipsStore` trait
#[derive(Clone)]
pub struct DbTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<Tips>>,
}

impl DbTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db.clone(), STORE_NAME) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn init_batch(&mut self, batch: &mut WriteBatch, initial_tips: &[Hash]) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &Arc::new(Tips::from_iter(initial_tips.iter().copied())))
    }

    pub fn add_tip_batch(
        &mut self,
        batch: &mut WriteBatch,
        new_tip: Hash,
        new_tip_parents: &[Hash],
    ) -> StoreResult<Arc<Tips>> {
        self.access.update(BatchDbWriter::new(batch), |tips| update_tips(tips, new_tip_parents, new_tip))
    }
}

/// Updates the internal data if possible
fn update_tips(mut current_tips: Arc<Tips>, new_tip_parents: &[Hash], new_tip: Hash) -> Arc<Tips> {
    let tips = Arc::make_mut(&mut current_tips);
    for parent in new_tip_parents {
        tips.remove(parent);
    }
    tips.insert(new_tip);
    current_tips
}

impl TipsStoreReader for DbTipsStore {
    fn get(&self) -> StoreResult<Arc<Tips>> {
        self.access.read()
    }
}

impl TipsStore for DbTipsStore {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<Tips>> {
        self.access.update(DirectDbWriter::new(&self.db), |tips| update_tips(tips, new_tip_parents, new_tip))
    }
}