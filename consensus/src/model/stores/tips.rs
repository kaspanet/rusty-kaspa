use std::sync::Arc;

use kaspa_consensus_core::BlockHashSet;
use kaspa_database::prelude::DbWriter;
use kaspa_database::prelude::StoreResult;
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_database::prelude::DB;
use kaspa_database::prelude::{BatchDbWriter, CachedDbItem, DirectDbWriter};
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_hashes::Hash;
use rocksdb::WriteBatch;

/// Reader API for `TipsStore`.
pub trait TipsStoreReader {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>>;
}

pub trait TipsStore: TipsStoreReader {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<BlockHashSet>>;
    fn add_tip_batch(&mut self, batch: &mut WriteBatch, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<BlockHashSet>> {
        self.add_tip_with_writer(BatchDbWriter::new(batch), new_tip, new_tip_parents)
    }
    fn add_tip_with_writer(
        &mut self,
        writer: impl DbWriter,
        new_tip: Hash,
        new_tip_parents: &[Hash],
    ) -> StoreResult<Arc<BlockHashSet>>;
    fn prune_tips_batch(&mut self, batch: &mut WriteBatch, pruned_tips: &[Hash]) -> StoreResult<()> {
        self.prune_tips_with_writer(BatchDbWriter::new(batch), pruned_tips)
    }
    fn prune_tips_with_writer(&mut self, writer: impl DbWriter, pruned_tips: &[Hash]) -> StoreResult<()>;
}

/// A DB + cache implementation of `TipsStore` trait
#[derive(Clone)]
pub struct DbTipsStore {
    db: Arc<DB>,
    access: CachedDbItem<Arc<BlockHashSet>>,
}

impl DbTipsStore {
    pub fn new(db: Arc<DB>) -> Self {
        Self { db: Arc::clone(&db), access: CachedDbItem::new(db, DatabaseStorePrefixes::Tips.into()) }
    }

    pub fn clone_with_new_cache(&self) -> Self {
        Self::new(Arc::clone(&self.db))
    }

    pub fn is_initialized(&self) -> bool {
        self.access.read().unwrap_option().is_some()
    }

    pub fn init_batch(&mut self, batch: &mut WriteBatch, initial_tips: &[Hash]) -> StoreResult<()> {
        self.access.write(BatchDbWriter::new(batch), &Arc::new(BlockHashSet::from_iter(initial_tips.iter().copied())))
    }
}

/// Updates the internal data if possible
fn update_tips(mut current_tips: Arc<BlockHashSet>, new_tip_parents: &[Hash], new_tip: Hash) -> Arc<BlockHashSet> {
    let tips = Arc::make_mut(&mut current_tips);
    for parent in new_tip_parents {
        tips.remove(parent);
    }
    tips.insert(new_tip);
    current_tips
}

impl TipsStoreReader for DbTipsStore {
    fn get(&self) -> StoreResult<Arc<BlockHashSet>> {
        self.access.read()
    }
}

impl TipsStore for DbTipsStore {
    fn add_tip(&mut self, new_tip: Hash, new_tip_parents: &[Hash]) -> StoreResult<Arc<BlockHashSet>> {
        self.access.update(DirectDbWriter::new(&self.db), |tips| update_tips(tips, new_tip_parents, new_tip))
    }

    fn add_tip_with_writer(
        &mut self,
        writer: impl DbWriter,
        new_tip: Hash,
        new_tip_parents: &[Hash],
    ) -> StoreResult<Arc<BlockHashSet>> {
        self.access.update(writer, |tips| update_tips(tips, new_tip_parents, new_tip))
    }

    fn prune_tips_with_writer(&mut self, writer: impl DbWriter, pruned_tips: &[Hash]) -> StoreResult<()> {
        if pruned_tips.is_empty() {
            return Ok(());
        }
        self.access.update(writer, |mut tips| {
            let mut_tips = Arc::make_mut(&mut tips);
            for pruned_tip in pruned_tips {
                mut_tips.remove(pruned_tip);
            }
            tips
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_tips() {
        let mut tips = Arc::new(BlockHashSet::from_iter([1.into(), 3.into(), 5.into()]));
        tips = update_tips(tips, &[3.into(), 5.into()], 7.into());
        assert_eq!(Arc::try_unwrap(tips).unwrap(), BlockHashSet::from_iter([1.into(), 7.into()]));
    }
}
