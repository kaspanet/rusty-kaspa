use std::{ops::RangeBounds, sync::Arc};

use kaspa_consensus_core::{BlockHashSet, Hash, tx::TransactionId};
use kaspa_core::trace;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, DB, DbWriter, StoreResult};
use rocksdb::WriteBatch;

use crate::{
    model::{
        score_refs::{BlueScoreAcceptingRefData, DaaScoreIncludingRefData},
        {TxAcceptanceData, TxInclusionData},
    },
    reindexer::{
        block_reindexer::{ReindexedBlockAddedState, ReindexedBlockBodyState},
        mergeset_reindexer::{ReindexedMergesetState, ReindexedVirtualChangedState},
    },
    stores::{
        TxIndexIncludingDaaScoreRefReader as _, TxIndexIncludingDaaScoreRefStore,
        acceptance::{
            BlueScoreRefTuple as AcceptingBlueScoreTuple, DbTxIndexAcceptedTransactionsStore, DbTxIndexAcceptingBlueScoreRefStore,
            TxAcceptedTuple, TxIndexAcceptedTransactionsStore, TxIndexAcceptedTransactionsStoreReader,
            TxIndexAcceptingBlueScoreRefReader, TxIndexAcceptingBlueScoreRefStore,
            sink::{DbTxIndexSinkStore, TxIndexSinkStore, TxIndexSinkStoreReader},
        },
        inclusion::{
            DaaScoreRefTuple as IncludingDaaScoreTuple, DbTxIndexIncludedTransactionsStore, DbTxIndexIncludingDaaScoreRefStore,
            TxInclusionTuple, TxIndexIncludedTransactionsStore, TxIndexIncludedTransactionsStoreReader,
            tips::{DbTxIndexTipsStore, TxIndexTipsStore, TxIndexTipsStoreReader},
        },
        pruning_sync::{DbPruningSyncStore, PruningData, PruningSyncStore, PruningSyncStoreReader, ToPruneStore},
    },
};

#[derive(Clone)]
pub struct Store {
    included_transactions_store: DbTxIndexIncludedTransactionsStore,
    accepted_transactions_store: DbTxIndexAcceptedTransactionsStore,
    accepting_bluescore_refs_store: DbTxIndexAcceptingBlueScoreRefStore,
    including_daascore_refs_store: DbTxIndexIncludingDaaScoreRefStore,
    pruning_sync_store: DbPruningSyncStore,
    sink_store: DbTxIndexSinkStore,
    tips_store: DbTxIndexTipsStore,
    db: Arc<DB>,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            included_transactions_store: DbTxIndexIncludedTransactionsStore::new(db.clone(), CachePolicy::Empty),
            accepted_transactions_store: DbTxIndexAcceptedTransactionsStore::new(db.clone(), CachePolicy::Empty),
            accepting_bluescore_refs_store: DbTxIndexAcceptingBlueScoreRefStore::new(db.clone(), CachePolicy::Empty),
            including_daascore_refs_store: DbTxIndexIncludingDaaScoreRefStore::new(db.clone(), CachePolicy::Empty),
            pruning_sync_store: DbPruningSyncStore::new(db.clone()),
            sink_store: DbTxIndexSinkStore::new(db.clone()),
            tips_store: DbTxIndexTipsStore::new(db.clone()),
            db,
        }
    }

    // -- getters --
    pub fn get_sink_with_blue_score(&self) -> StoreResult<Option<(Hash, u64)>> {
        self.sink_store.get_sink_with_blue_score()
    }

    pub fn get_tips(&self) -> StoreResult<Option<Arc<BlockHashSet>>> {
        self.tips_store.get_tips()
    }

    pub fn get_retention_root_blue_score(&self) -> StoreResult<Option<u64>> {
        self.pruning_sync_store.get_retention_root_blue_score()
    }

    pub fn get_retention_root_daa_score(&self) -> StoreResult<Option<u64>> {
        self.pruning_sync_store.get_retention_root_daa_score()
    }

    pub fn get_retention_root(&self) -> StoreResult<Option<Hash>> {
        self.pruning_sync_store.get_retention_root()
    }

    pub fn get_next_to_prune_blue_score(&self) -> StoreResult<Option<u64>> {
        self.pruning_sync_store.get_next_to_prune_blue_score()
    }

    pub fn get_next_to_prune_daa_score(&self) -> StoreResult<Option<u64>> {
        self.pruning_sync_store.get_next_to_prune_daa_score()
    }

    pub fn get_included_transaction_data(&self, transaction_id: TransactionId) -> StoreResult<Vec<TxInclusionData>> {
        self.included_transactions_store.get_transaction_inclusion_data(transaction_id)
    }

    pub fn get_accepted_transaction_data(&self, transaction_id: TransactionId) -> StoreResult<Vec<TxAcceptanceData>> {
        self.accepted_transactions_store.get_transaction_acceptance_data(transaction_id)
    }

    pub fn get_next_to_prune_store(&self) -> StoreResult<Option<ToPruneStore>> {
        self.pruning_sync_store.get_next_to_prune_store()
    }

    pub fn is_inclusion_pruning_done(&self) -> StoreResult<bool> {
        Ok(
            self.pruning_sync_store.is_inclusion_pruning_done()? || self.including_daascore_refs_store.is_empty()?, // in cases where store is empty, we also consider pruning to be "done"
        )
    }

    pub fn is_acceptance_pruning_done(&self) -> StoreResult<bool> {
        Ok(
            self.pruning_sync_store.is_acceptance_pruning_done()? || self.accepting_bluescore_refs_store.is_empty()?, // in cases where store is empty, we also consider pruning to be "done"
        )
    }

    // --- scan ops ---
    pub fn scan_blue_score_range(
        &self,
        range: impl RangeBounds<u64>,
        limit: Option<usize>,
    ) -> StoreResult<Vec<BlueScoreAcceptingRefData>> {
        Ok(self.accepting_bluescore_refs_store.get_blue_score_refs(range, limit)?.collect())
    }

    pub fn scan_daa_score_range(
        &self,
        range: impl RangeBounds<u64>,
        limit: Option<usize>,
    ) -> StoreResult<Vec<DaaScoreIncludingRefData>> {
        Ok(self.including_daascore_refs_store.get_daa_score_refs(range, limit)?.collect())
    }

    // -- updaters --

    pub fn update_via_reindexed_block_added_state<TxIter, BlueScoreIter>(
        &mut self,
        reindexed_block_added_state: ReindexedBlockAddedState<TxIter, BlueScoreIter>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxInclusionTuple>,
        BlueScoreIter: Iterator<Item = AcceptingBlueScoreTuple>,
    {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.update_with_reindexed_block_body_states_with_writer(&mut writer, reindexed_block_added_state.body)?;
        self.tips_store.remove_tips(&mut writer, reindexed_block_added_state.direct_parents)?;
        self.tips_store.set_tip(&mut writer, reindexed_block_added_state.block_hash)?;

        self.commit_batch(batch)?;

        Ok(())
    }
    pub fn update_with_reindexed_block_body_states<TxIter, BlueScoreIter>(
        &mut self,
        states: Vec<ReindexedBlockBodyState<TxIter, BlueScoreIter>>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxInclusionTuple>,
        BlueScoreIter: Iterator<Item = IncludingDaaScoreTuple>,
    {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        for state in states {
            self.update_with_reindexed_block_body_states_with_writer(&mut writer, state)?;
        }

        self.commit_batch(batch)?;

        Ok(())
    }

    pub fn update_with_reindexed_block_body_states_with_writer<TxIter, DaaScoreIter>(
        &mut self,
        writer: &mut impl DbWriter,
        state: ReindexedBlockBodyState<TxIter, DaaScoreIter>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxInclusionTuple>,
        DaaScoreIter: Iterator<Item = IncludingDaaScoreTuple>,
    {
        self.included_transactions_store.add_included_transaction_data(writer, state.tx_iter)?;
        self.including_daascore_refs_store.add_daa_score_refs(writer, state.daa_score_ref_iter)?;

        Ok(())
    }

    pub fn update_via_reindexed_virtual_chain_changed_state<TxIter, BlueScoreIter>(
        &mut self,
        state: ReindexedVirtualChangedState<TxIter, BlueScoreIter>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxAcceptedTuple>,
        BlueScoreIter: Iterator<Item = AcceptingBlueScoreTuple>,
    {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

        self.update_reindexed_mergeset_states_with_writer(&mut writer, state.reindexed_mergeset_state)?;
        self.sink_store.set_sink(&mut writer, state.sink_hash, state.sink_blue_score)?;

        self.commit_batch(batch)?;

        Ok(())
    }

    pub fn update_with_reindexed_mergeset_states<TxIter, BlueScoreIter>(
        &mut self,
        states: Vec<ReindexedMergesetState<TxIter, BlueScoreIter>>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxAcceptedTuple>,
        BlueScoreIter: Iterator<Item = AcceptingBlueScoreTuple>,
    {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

        self.update_reindexed_mergeset_states_with_writer(&mut writer, states)?;

        self.commit_batch(batch)?;

        Ok(())
    }

    fn update_reindexed_mergeset_states_with_writer<TxIter, BlueScoreIter>(
        &mut self,
        mut writer: &mut impl DbWriter,
        states: Vec<ReindexedMergesetState<TxIter, BlueScoreIter>>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxAcceptedTuple>,
        BlueScoreIter: Iterator<Item = AcceptingBlueScoreTuple>,
    {
        for state in states.into_iter() {
            self.accepted_transactions_store.add_accepted_transaction_data(&mut writer, state.tx_iter)?;
            self.accepting_bluescore_refs_store.add_blue_score_refs(&mut writer, state.blue_score_ref_iter)?;
        }
        Ok(())
    }

    pub fn update_to_new_retention_root(
        &mut self,
        retention_root: Hash,
        retention_root_blue_score: u64,
        retention_root_daa_score: u64,
    ) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.pruning_sync_store.update_to_new_retention_root(
            &mut writer,
            retention_root,
            retention_root_blue_score,
            retention_root_daa_score,
        )?;

        self.commit_batch(batch)
    }

    /// -- prune ops --
    pub fn prune_inclusion_data_from_daa_score(
        &mut self,
        from_daa_score: u64,
        max_daa_score: u64,
        limit: Option<usize>,
    ) -> StoreResult<bool> {
        trace!("Pruning inclusion stores below daa score: {}", max_daa_score);
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

        let mut next_to_prune_daa_score = 0u64;
        let mut is_inclusion_store_empty = true;

        for data in self.scan_daa_score_range(from_daa_score..=u64::MAX, limit)? {
            is_inclusion_store_empty = false;
            next_to_prune_daa_score = data.daa_score;
            if data.daa_score >= max_daa_score {
                next_to_prune_daa_score = data.daa_score;
                break;
            };
            self.included_transactions_store.remove_transaction_inclusion_data(&mut writer, data.transaction_id, data.daa_score)?;
        }

        self.pruning_sync_store.set_new_next_to_prune_daa_score(&mut writer, next_to_prune_daa_score)?;

        self.commit_batch(batch)?;

        let is_done = is_inclusion_store_empty || next_to_prune_daa_score >= max_daa_score;

        Ok(is_done)
    }

    pub fn prune_acceptance_data_from_blue_score(
        &mut self,
        from_blue_score: u64,
        max_blue_score: u64,
        limit: Option<usize>,
    ) -> StoreResult<bool> {
        trace!("Pruning acceptance stores below blue score: {}", max_blue_score);
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);

        let mut next_to_prune_blue_score = 0u64;
        let mut is_acceptance_store_empty = true;

        for data in self.scan_blue_score_range(from_blue_score..=u64::MAX, limit)? {
            is_acceptance_store_empty = false;
            next_to_prune_blue_score = data.blue_score;
            if data.blue_score >= max_blue_score {
                next_to_prune_blue_score = data.blue_score;
                break;
            };
            self.accepted_transactions_store.remove_transaction_acceptance_data(&mut writer, data.transaction_id, data.blue_score)?;
        }

        self.pruning_sync_store.set_new_next_to_prune_blue_score(&mut writer, next_to_prune_blue_score)?;

        self.commit_batch(batch)?;

        let is_done = is_acceptance_store_empty || next_to_prune_blue_score >= max_blue_score;

        Ok(is_done)
    }

    // -- set / init
    pub fn set_sink(&mut self, sink: Hash, blue_score: u64) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.sink_store.set_sink(&mut writer, sink, blue_score)?;

        self.commit_batch(batch)
    }

    pub fn init_tips(&mut self, tips: BlockHashSet) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.tips_store.init_tips(&mut writer, tips)?;

        self.commit_batch(batch)
    }

    pub fn set_next_to_prune_blue_score(&mut self, blue_score: u64) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.pruning_sync_store.set_new_next_to_prune_blue_score(&mut writer, blue_score)?;

        self.commit_batch(batch)
    }

    pub fn set_new_pruning_data(&mut self, pruning_data: PruningData) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.pruning_sync_store.set_new_pruning_data(&mut writer, pruning_data)?;

        self.commit_batch(batch)
    }

    pub fn set_next_to_prune_store(&mut self, to_prune_store: ToPruneStore) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.pruning_sync_store.set_next_to_prune_store(&mut writer, to_prune_store)?;

        self.commit_batch(batch)
    }

    pub fn remove_tip(&mut self, tip: Hash) -> StoreResult<()> {
        let mut batch = WriteBatch::default();
        let mut writer = BatchDbWriter::new(&mut batch);
        self.tips_store.remove_tips(&mut writer, vec![tip])?;

        self.commit_batch(batch)
    }

    // -- commit ---
    fn commit_batch(&self, batch: WriteBatch) -> StoreResult<()> {
        Ok(self.db.write(batch)?)
    }
}
