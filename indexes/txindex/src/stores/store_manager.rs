use std::{collections::HashSet, ops::RangeBounds, sync::Arc};

use kaspa_consensus_core::{
    tx::{ScriptPublicKeys, TransactionId, TransactionIndexType, TransactionOutpoint},
    BlockHashSet, Hash,
};
use kaspa_core::trace;
use kaspa_database::prelude::{BatchDbWriter, CachePolicy, StoreResult, WriteBatch, DB};

use crate::{
    model::{
        bluescore_refs::BlueScoreRefData,
        transactions::{TxAcceptanceData, TxInclusionData},
    },
    reindexer::{block_added_reindexer::ReindexedBlockAddedState, virtual_changed_reindexer::ReindexedVirtualChangedState},
    stores::{
        accepted_transactions::{
            DbTxIndexAcceptedTransactionsStore, TxIndexAcceptedTransactionsStore, TxIndexAcceptedTransactionsStoreReader,
        },
        bluescore_refs::{DbTxIndexBlueScoreRefStore, RefType, StoreQuery, TxIndexBlueScoreRefReader, TxIndexBlueScoreRefStore},
        included_transactions::{
            DbTxIndexIncludedTransactionsStore, TxIndexIncludedTransactionsStore, TxIndexIncludedTransactionsStoreReader,
        },
        pruning_sync::DbPruningSyncStore,
        sink::{DbTxIndexSinkStore, TxIndexSinkStore},
        tips::{DbTxIndexTipsStore, TxIndexTipsStore},
        BlueScoreRefTuple, TxAcceptedTuple, TxInclusionTuple,
    },
};

#[derive(Clone)]
pub struct Store {
    included_transactions_store: DbTxIndexIncludedTransactionsStore,
    accepted_transactions_store: DbTxIndexAcceptedTransactionsStore,
    bluescore_refs_store: DbTxIndexBlueScoreRefStore,
    pruning_sync_store: DbPruningSyncStore,
    sink_store: DbTxIndexSinkStore,
    tips_store: DbTxIndexTipsStore,
    db: Arc<DB>,
}

impl Store {
    pub fn new(db: Arc<DB>) -> Self {
        Self {
            included_transactions_store: DbTxIndexIncludedTransactionsStore::new(
                db.clone(),
                //IF REQUIRED: this store requires iterations, as such a hashmap-based cache here is not suitable,
                //perhaps, if required, we may implement a suitable cache, such as one based on b-tree.
                CachePolicy::Empty,
            ),
            accepted_transactions_store: DbTxIndexAcceptedTransactionsStore::new(
                db.clone(),
                //IF REQUIRED: this store requires iterations, as such a hashmap-based cache here is not suitable,
                //perhaps, if required, we may implement a suitable cache, such as one based on b-tree.
                CachePolicy::Empty,
            ),
            bluescore_refs_store: DbTxIndexBlueScoreRefStore::new(
                db.clone(),
                //IF REQUIRED: this store requires iterations, as such a hashmap-based cache here is not suitable,
                //perhaps, if required, we may implement a suitable cache, such as one based on b-tree.
                CachePolicy::Empty,
            ),
            pruning_sync_store: DbPruningSyncStore::new(db.clone()),
            sink_store: DbTxIndexSinkStore::new(db.clone()),
            tips_store: DbTxIndexTipsStore::new(db.clone()),
            db,
        }
    }

    // -- getters --

    pub fn get_included_transaction_data(&self, tx_id: TransactionId) -> StoreResult<Vec<TxInclusionData>> {
        self.included_transactions_store.get_transaction_inclusion_data(tx_id)
    }

    pub fn get_accepted_transaction_data(&self, tx_id: TransactionId) -> StoreResult<Vec<TxAcceptanceData>> {
        self.accepted_transactions_store.get_transaction_acceptance_data(tx_id)
    }

    pub fn get_transaction_data_by_blue_score_range(&self, range: impl RangeBounds<u64>) -> StoreResult<Vec<BlueScoreRefData>> {
        Ok(self.bluescore_refs_store.get_blue_score_refs(range, usize::MAX, StoreQuery::Both)?.collect())
    }

    pub fn get_transaction_inclusion_data_by_blue_score_range(
        &self,
        range: impl RangeBounds<u64>,
    ) -> StoreResult<Vec<BlueScoreRefData>> {
        Ok(self.bluescore_refs_store.get_blue_score_refs(range, usize::MAX, StoreQuery::IncludedTransactionStoreKey)?.collect())
    }

    pub fn get_transaction_acceptance_data_by_blue_score_range(
        &self,
        range: impl RangeBounds<u64>,
    ) -> StoreResult<Vec<BlueScoreRefData>> {
        Ok(self.bluescore_refs_store.get_blue_score_refs(range, usize::MAX, StoreQuery::AcceptedTransactionStoreKey)?.collect())
    }

    // -- updaters --

    pub fn update_with_new_block_added_state<TxIter, BlueScoreIter>(
        &mut self,
        state: ReindexedBlockAddedState<TxIter, BlueScoreIter>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxInclusionTuple>,
        BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
    {
        trace!("Updating stores with block added: {}", state.block_hash);
        let mut batch = WriteBatch::default();

        self.included_transactions_store.add_included_transaction_data(BatchDbWriter::new(&mut batch), state.tx_iter)?;
        self.bluescore_refs_store.add_blue_score_refs(BatchDbWriter::new(&mut batch), state.blue_score_ref_iter)?;
        self.tips_store.remove_tips(BatchDbWriter::new(&mut batch), state.direct_parents)?;
        self.tips_store.set_tip(BatchDbWriter::new(&mut batch), state.block_hash)?;

        self.commit_batch(batch)?;

        Ok(())
    }

    pub fn update_with_new_virtual_changed_state<TxIter, BlueScoreIter>(
        &mut self,
        state: ReindexedVirtualChangedState<TxIter, BlueScoreIter>,
    ) -> StoreResult<()>
    where
        TxIter: Iterator<Item = TxAcceptedTuple>,
        BlueScoreIter: Iterator<Item = BlueScoreRefTuple>,
    {
        trace!("Updating stores with virtual changed: {}", state.sink_hash);
        let mut batch = WriteBatch::default();

        self.accepted_transactions_store.add_accepted_transaction_data(BatchDbWriter::new(&mut batch), state.tx_iter)?;
        self.bluescore_refs_store.add_blue_score_refs(BatchDbWriter::new(&mut batch), state.blue_score_ref_iter)?;
        self.sink_store.set_sink(BatchDbWriter::new(&mut batch), state.sink_hash, state.sink_blue_score)?;

        self.commit_batch(batch)?;

        Ok(())
    }

    // -- commit --
    fn commit_batch(&self, batch: WriteBatch) -> StoreResult<()> {
        Ok(self.db.write(batch)?)
    }
}
