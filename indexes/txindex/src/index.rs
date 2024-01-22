use std::{fmt::Debug, sync::Arc};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{
    ChainAcceptanceDataPrunedNotification as ConsensusChainAcceptanceDataPrunedNotification,
    VirtualChainChangedNotification as ConsensusVirtualChainChangedNotification,
};
use kaspa_consensusmanager::{ConsensusManager, ConsensusSessionBlocking};
use kaspa_core::{error, info, trace};
use kaspa_database::prelude::DB;
use kaspa_hashes::Hash;
use kaspa_index_core::{
    models::txindex::{BlockAcceptanceOffset, TxOffset},
    reindexers::txindex::TxIndexReindexer,
};
use parking_lot::RwLock;
use rocksdb::WriteBatch;

use crate::{
    config::Config as TxIndexConfig,
    core::api::TxIndexApi,
    core::errors::TxIndexResult,
    stores::{
        TxIndexAcceptedTxOffsetsReader, TxIndexAcceptedTxOffsetsStore, TxIndexBlockAcceptanceOffsetsReader,
        TxIndexBlockAcceptanceOffsetsStore, TxIndexHistoryRootReader, TxIndexHistoryRootStore, TxIndexSinkReader, TxIndexSinkStore,
        TxIndexStores,
    },
    IDENT,
};
pub struct TxIndex {
    stores: TxIndexStores,
    consensus_manager: Arc<ConsensusManager>,
    config: Arc<TxIndexConfig>, // move into config, once txindex is configurable.
}

impl TxIndex {
    pub fn new(consensus_manager: Arc<ConsensusManager>, db: Arc<DB>, config: Arc<TxIndexConfig>) -> TxIndexResult<Arc<RwLock<Self>>> {
        info!("[{0}] Initializing", IDENT);
        let mut txindex = Self { stores: TxIndexStores::new(db, &config)?, consensus_manager: consensus_manager.clone(), config };

        if !txindex.is_synced()? {
            info!("[{0}] Resyncing", IDENT);
            match txindex.resync() {
                Ok(_) => {
                    info!("[{0}] Resync Successful", IDENT);
                }
                Err(e) => {
                    error!("[{0}] Failed to resync: {1}", IDENT, e);
                    txindex.stores.delete_all()?; // we try and delete all, in order to remove any partial data that may have been written.
                    return Err(e);
                }
            };
        };
        let txindex = Arc::new(RwLock::new(txindex));
        info!("[{0}] Initialized Succefully", IDENT);
        //consensus_manager.register_consensus_reset_handler(Arc::new(TxIndexConsensusResetHandler::new(Arc::downgrade(&txindex))));
        Ok(txindex)
    }

    fn update_with_reindexed_changes(&mut self, reindexer: TxIndexReindexer) -> TxIndexResult<()> {
        trace!(
            "[{0}] Updating: Reindex Added: {1} Blocks with {2} Txs, with sink => {3:?}",
            IDENT,
            reindexer.block_acceptance_offsets_changes.added.len(),
            reindexer.tx_offset_changes.added.len(),
            reindexer.sink,
        );

        trace!(
            "[{0}] Updating: Reindex Removed: {1} Blocks with {2} Txs, with history root => {3:?}",
            IDENT,
            reindexer.block_acceptance_offsets_changes.removed.len(),
            reindexer.tx_offset_changes.removed.len(),
            reindexer.history_root,
        );

        let mut batch = WriteBatch::default();

        self.stores.accepted_tx_offsets_store.write_diff_batch(&mut batch, reindexer.tx_offset_changes)?;
        self.stores.block_acceptance_offsets_store.write_diff_batch(&mut batch, reindexer.block_acceptance_offsets_changes)?;

        if let Some(history_root) = reindexer.history_root {
            self.stores.history_root_store.set_if_new(&mut batch, history_root)?;
        }
        if let Some(sink) = reindexer.sink {
            self.stores.sink_store.set(&mut batch, sink)?;
        }

        self.stores.write_batch(batch)
    }

    fn sync_segement(
        &mut self,
        mut sync_from: Hash,
        sync_to: Hash,
        remove_segment: bool,
        session: &ConsensusSessionBlocking<'_>,
    ) -> TxIndexResult<()> {
        trace!("[{0}] {1}: {2} => {3}", IDENT, if remove_segment { "Unsyncing" } else { "Syncing" }, sync_from, sync_to);
        let total_blocks_to_process =
            session.get_compact_header(sync_to)?.daa_score - session.get_compact_header(sync_from)?.daa_score;
        info!("[{0}] {1}: {2} Blocks", IDENT, if remove_segment { "Unsyncing" } else { "Syncing" }, total_blocks_to_process);
        let mut total_blocks_processed = (0u64, 0u64); // .0 holds the value of the former display
        let mut total_txs_processed = (0u64, 0u64); // .0 holds the value of the former display
        let mut percent_completed = (0f64, 0f64); // .0 holds the value of the former display
        let percent_display_granularity = 1.0; // in percent
        let mut instant = std::time::Instant::now();

        while sync_from != sync_to {
            let to_process_hashes =
                session.get_virtual_chain_from_block(sync_from, Some(sync_to), self.config.perf.resync_chunksize)?.added;

            let acceptance_data = Arc::new(
                to_process_hashes.iter().filter_map(|hash| session.get_block_acceptance_data(*hash).ok()).collect::<Vec<_>>(),
            );

            sync_from = *to_process_hashes.last().unwrap();

            //TODO: make txindex reindex accept a hash and acceptance data vec, for now use a pseudo notification.
            let vspcc_notification = if remove_segment {
                ConsensusVirtualChainChangedNotification {
                    added_chain_block_hashes: Arc::new(vec![]),
                    removed_chain_block_hashes: to_process_hashes.into(),
                    added_chain_blocks_acceptance_data: Arc::new(vec![]),
                    removed_chain_blocks_acceptance_data: acceptance_data,
                }
            } else {
                ConsensusVirtualChainChangedNotification {
                    added_chain_block_hashes: to_process_hashes.into(),
                    removed_chain_block_hashes: Arc::new(vec![]),
                    added_chain_blocks_acceptance_data: acceptance_data,
                    removed_chain_blocks_acceptance_data: Arc::new(vec![]),
                }
            };

            let txindex_reindexer = TxIndexReindexer::from(vspcc_notification);

            total_blocks_processed.1 += (txindex_reindexer.block_acceptance_offsets_changes.removed.len()
                + txindex_reindexer.block_acceptance_offsets_changes.added.len()) as u64;
            total_txs_processed.1 +=
                (txindex_reindexer.tx_offset_changes.removed.len() + txindex_reindexer.tx_offset_changes.added.len()) as u64;
            percent_completed.1 = (total_blocks_processed.1 as f64 / total_blocks_to_process as f64) * 100.0;

            self.update_with_reindexed_changes(txindex_reindexer)?;

            let is_end = sync_from == sync_to;

            if percent_completed.0 + percent_display_granularity <= percent_completed.1 || is_end {
                let total_txs_processed_diff = total_txs_processed.1 - total_txs_processed.0;
                let total_blocks_processed_diff = total_blocks_processed.1 - total_blocks_processed.0;

                info!(
                    "[{0}] {1} - Txs: {2} ({3:.0}/s); Blocks: {4} ({5:.0}/s); {6:.0}%",
                    IDENT,
                    if remove_segment { "Removed" } else { "Added" },
                    total_txs_processed.1,
                    total_txs_processed_diff as f64 / instant.elapsed().as_secs_f64(),
                    total_blocks_processed.1,
                    total_blocks_processed_diff as f64 / instant.elapsed().as_secs_f64(),
                    if is_end { 100.0 } else { percent_completed.1 },
                );
                percent_completed.0 = percent_completed.1;
                total_blocks_processed.0 = total_blocks_processed.1;
                total_txs_processed.0 = total_txs_processed.1;
                instant = std::time::Instant::now();
            }
        }
        Ok(())
    }
}

impl TxIndexApi for TxIndex {
    // Resync methods.
    fn resync(&mut self) -> TxIndexResult<()> {
        trace!("[{0}] Started Resyncing", IDENT);

        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        // Gather the necessary potential block hashes to sync from and to.
        let txindex_history_root = self.stores.history_root_store.get()?;
        let consensus_history_root = session.get_history_root();
        let txindex_sink = self.stores.sink_store.get()?;
        let consensus_sink = session.get_sink();

        let resync_from =
            txindex_history_root.filter(|txindex_history_root| *txindex_history_root == consensus_history_root).unwrap_or({
                info!("[{0}] Resetting the DB: txindex and consensus history roots are not synced!", IDENT);
                self.stores.delete_all()?; // we reset the txindex
                self.stores.history_root_store.set(consensus_history_root)?; // We can set the history_root anew after clearing db
                consensus_history_root
            });
        let resync_to = consensus_sink;

        let unsync_from = txindex_sink;
        let unsync_to = if let Some(txindex_sink) = txindex_sink {
            if txindex_sink != consensus_sink && !session.is_chain_block(txindex_sink)? {
                Some(session.find_highest_common_chain_block(consensus_sink, txindex_sink)?)
            } else {
                None
            }
        } else {
            None
        };

        if unsync_from.is_some() && unsync_to.is_some() {
            self.sync_segement(unsync_from.unwrap(), unsync_to.unwrap(), true, &session)?;
        }
        self.sync_segement(resync_from, resync_to, false, &session)?;
        Ok(())
    }

    // Sync state methods
    fn is_synced(&self) -> TxIndexResult<bool> {
        trace!("[{0}] checking sync status...", IDENT);

        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        if let Some(txindex_sink) = self.stores.sink_store.get()? {
            if txindex_sink == session.get_sink() {
                if let Some(txindex_source) = self.stores.history_root_store.get()? {
                    if txindex_source == session.get_history_root() {
                        return Ok(true);
                    }
                }
            }
        };

        Ok(false)
    }

    fn get_block_acceptance_offset(&self, hash: Hash) -> TxIndexResult<Option<BlockAcceptanceOffset>> {
        trace!("[{0}] Getting merged block acceptance offsets for block: {1}", IDENT, hash);

        Ok(self.stores.block_acceptance_offsets_store.get(hash)?)
    }

    fn get_tx_offset(&self, tx_id: TransactionId) -> TxIndexResult<Option<TxOffset>> {
        trace!("[{0}] Getting tx offsets for transaction_id: {1} ", IDENT, tx_id);

        Ok(self.stores.accepted_tx_offsets_store.get(tx_id)?)
    }

    // Update methods
    fn update_via_virtual_chain_changed(&mut self, vspcc_notification: ConsensusVirtualChainChangedNotification) -> TxIndexResult<()> {
        trace!(
            "[{0}] Updating: Added: {1} chain blocks; Removed: {2} chain blocks - via virtual chain changed notification",
            IDENT,
            vspcc_notification.added_chain_block_hashes.len(),
            vspcc_notification.removed_chain_blocks_acceptance_data.len()
        );

        self.update_with_reindexed_changes(TxIndexReindexer::from(vspcc_notification))
    }

    fn update_via_chain_acceptance_data_pruned(
        &mut self,
        chain_acceptance_data_pruned: ConsensusChainAcceptanceDataPrunedNotification,
    ) -> TxIndexResult<()> {
        trace!(
            "[{0}] Updating: Added: {1} chain blocks - via virtual chain changed notification",
            IDENT,
            chain_acceptance_data_pruned.mergeset_block_acceptance_data_pruned.len()
        );

        self.update_with_reindexed_changes(TxIndexReindexer::from(chain_acceptance_data_pruned))
    }

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_accepted_tx_offsets(&self) -> TxIndexResult<usize> {
        trace!("[{0}] Counting: All accepted tx offsets", IDENT);
        Ok(self.stores.accepted_tx_offsets_store.count()?)
    }

    // This potentially causes a large chunk of processing, so it should only be used only for tests.
    fn count_block_acceptance_offsets(&self) -> TxIndexResult<usize> {
        trace!("[{0}] Counting: All block acceptance offsets", IDENT);
        Ok(self.stores.block_acceptance_offsets_store.count()?)
    }

    fn get_sink(&self) -> TxIndexResult<Option<Hash>> {
        trace!("[{0}] Getting: Sink", IDENT);
        Ok(self.stores.sink_store.get()?)
    }

    fn get_history_root(&self) -> TxIndexResult<Option<Hash>> {
        trace!("[{0}] Getting: History root", IDENT);
        Ok(self.stores.history_root_store.get()?)
    }
}

impl Debug for TxIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(IDENT).finish()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use kaspa_consensus::{
        consensus::test_consensus::{TestConsensus, TestConsensusFactory},
        model::stores::virtual_state::VirtualState,
        params::MAINNET_PARAMS,
    };
    use kaspa_consensus_core::{
        acceptance_data::{MergesetBlockAcceptanceData, TxEntry},
        api::ConsensusApi,
        config::Config as ConsensusConfig,
        tx::TransactionId,
        ChainPath,
    };
    use kaspa_consensus_notify::notification::{ChainAcceptanceDataPrunedNotification, VirtualChainChangedNotification};
    use kaspa_consensusmanager::ConsensusManager;

    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::Hash;

    use kaspa_index_core::models::txindex::AcceptanceDataIndexType;
    use parking_lot::RwLock;

    use rocksdb::WriteBatch;

    use crate::{api::TxIndexApi, config::Config as TxIndexConfig, TxIndex};

    fn assert_equal_along_virtual_chain(virtual_chain: &ChainPath, test_consensus: Arc<TestConsensus>, txindex: Arc<RwLock<TxIndex>>) {
        assert!(txindex.write().is_synced().unwrap());
        assert_eq!(txindex.write().get_sink().unwrap().unwrap(), test_consensus.get_sink());
        assert_eq!(txindex.write().get_history_root().unwrap().unwrap(), test_consensus.get_history_root());

        // check intial state
        for (accepting_block_hash, acceptance_data) in
            virtual_chain.added.iter().map(|hash| (*hash, test_consensus.get_block_acceptance_data(*hash).unwrap()))
        {
            for (i, mergeset_block_acceptance_data) in acceptance_data.iter().cloned().enumerate() {
                let block_acceptance_offset =
                    txindex.write().get_block_acceptance_offset(mergeset_block_acceptance_data.block_hash).unwrap().unwrap();
                assert_eq!(block_acceptance_offset.accepting_block, accepting_block_hash);
                assert_eq!(block_acceptance_offset.acceptance_data_index, i as AcceptanceDataIndexType);
                for tx_entry in mergeset_block_acceptance_data.accepted_transactions {
                    let tx_offset = txindex.write().get_tx_offset(tx_entry.transaction_id).unwrap().unwrap();
                    assert_eq!(tx_offset.including_block, mergeset_block_acceptance_data.block_hash);
                    assert_eq!(tx_offset.transaction_index, tx_entry.index_within_block);
                }
            }
        }

        for (accepting_block, acceptance_data) in
            virtual_chain.removed.iter().map(|hash| (*hash, test_consensus.get_block_acceptance_data(*hash).unwrap()))
        {
            for mergeset_block_acceptance_data in acceptance_data.iter().cloned() {
                let block_acceptance = txindex.write().get_block_acceptance_offset(mergeset_block_acceptance_data.block_hash).unwrap();
                if let Some(block_acceptance) = block_acceptance {
                    assert_ne!(block_acceptance.accepting_block, accepting_block);
                } else {
                    continue;
                }
            }
        }
    }

    #[test]
    fn test_txindex_updates() {
        kaspa_core::log::try_init_logger("TRACE");

        // Note: this test closely mirrors the test `test_txindex_reindexer_from_virtual_chain_changed_notification`
        // If both fail, check for problems within the reindexer.

        // Set-up:
        let (_txindex_db_lt, txindex_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (_tc_db_lt, tc_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));

        let tc_config = ConsensusConfig::new(MAINNET_PARAMS);
        let txindex_config = Arc::new(TxIndexConfig::from(&Arc::new(tc_config.clone())));

        let tc = Arc::new(TestConsensus::with_db(tc_db.clone(), &tc_config));
        let tcm = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(tc.clone()))));
        let txindex = TxIndex::new(tcm, txindex_db, txindex_config).unwrap();

        // Define the block hashes:

        // Blocks removed (i.e. unaccepted):
        let block_a = Hash::from_u64_word(1);
        let block_b = Hash::from_u64_word(2);

        // Blocks ReAdded (i.e. reaccepted):
        let block_aa @ block_hh = Hash::from_u64_word(3);

        // Blocks Added (i.e. newly reaccepted):
        let block_h = Hash::from_u64_word(4);
        let block_i = Hash::from_u64_word(5);

        // Define the tx ids;

        // Txs removed (i.e. unaccepted)):
        let tx_a_1 = TransactionId::from_u64_word(6); // accepted in block a, not reaccepted
        let tx_aa_2 = TransactionId::from_u64_word(7); // accepted in block aa, not reaccepted
        let tx_b_3 = TransactionId::from_u64_word(8); // accepted in block bb, not reaccepted

        // Txs ReAdded (i.e. reaccepted)):
        let tx_a_2 @ tx_h_1 = TransactionId::from_u64_word(9); // accepted in block a, reaccepted in block h
        let tx_a_3 @ tx_i_4 = TransactionId::from_u64_word(10); // accepted in block a, reaccepted in block i
        let tx_a_4 @ tx_hh_3 = TransactionId::from_u64_word(11); // accepted in block a, reaccepted in block hh
        let tx_aa_1 @ tx_h_2 = TransactionId::from_u64_word(12); // accepted in block aa, reaccepted in block_h
        let tx_aa_3 @ tx_i_1 = TransactionId::from_u64_word(13); // accepted in block aa, reaccepted in block_i
        let tx_aa_4 @ tx_hh_4 = TransactionId::from_u64_word(14); // accepted in block aa, reaccepted in block_hh
        let tx_b_1 @ tx_h_3 = TransactionId::from_u64_word(15); // accepted in block b, reaccepted in block_h
        let tx_b_2 @ tx_i_2 = TransactionId::from_u64_word(16); // accepted in block b, reaccepted in block_i
        let tx_b_4 @ tx_hh_1 = TransactionId::from_u64_word(17); // accepted in block b, reaccepted in block_hh

        // Txs added (i.e. newly accepted)):
        let tx_h_4 = TransactionId::from_u64_word(18); // not originally accepted, accepted in block h.
        let tx_hh_2 = TransactionId::from_u64_word(19); // not originally accepted, accepted in block hh.
        let tx_i_3 = TransactionId::from_u64_word(20); // not originally accepted, accepted in block i.

        let acceptance_data_a = Arc::new(vec![
            MergesetBlockAcceptanceData {
                block_hash: block_a,
                accepted_transactions: vec![
                    TxEntry { transaction_id: tx_a_1, index_within_block: 0 },
                    TxEntry { transaction_id: tx_a_2, index_within_block: 1 },
                    TxEntry { transaction_id: tx_a_3, index_within_block: 2 },
                    TxEntry { transaction_id: tx_a_4, index_within_block: 3 },
                ],
            },
            MergesetBlockAcceptanceData {
                block_hash: block_aa,
                accepted_transactions: vec![
                    TxEntry { transaction_id: tx_aa_1, index_within_block: 0 },
                    TxEntry { transaction_id: tx_aa_2, index_within_block: 1 },
                    TxEntry { transaction_id: tx_aa_3, index_within_block: 2 },
                    TxEntry { transaction_id: tx_aa_4, index_within_block: 3 },
                ],
            },
        ]);

        let acceptance_data_b = Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: block_b,
            accepted_transactions: vec![
                TxEntry { transaction_id: tx_b_1, index_within_block: 0 },
                TxEntry { transaction_id: tx_b_2, index_within_block: 1 },
                TxEntry { transaction_id: tx_b_3, index_within_block: 2 },
                TxEntry { transaction_id: tx_b_4, index_within_block: 3 },
            ],
        }]);

        let virtual_chain = ChainPath { added: vec![block_a, block_b], removed: Vec::new() };

        let mut batch = WriteBatch::default();
        tc.acceptance_data_store.insert_batch(&mut batch, block_a, acceptance_data_a.clone()).unwrap();
        tc.acceptance_data_store.insert_batch(&mut batch, block_b, acceptance_data_b.clone()).unwrap();
        let mut state = VirtualState::default();
        state.ghostdag_data.selected_parent = block_b;
        tc.virtual_stores.write().state.set_batch(&mut batch, Arc::new(state)).unwrap();
        tc_db.write(batch).unwrap();

        let init_virtual_chain_changed_notification = VirtualChainChangedNotification {
            added_chain_block_hashes: virtual_chain.added.clone().into(),
            removed_chain_block_hashes: virtual_chain.removed.clone().into(),
            added_chain_blocks_acceptance_data: Arc::new(vec![acceptance_data_a.clone(), acceptance_data_b.clone()]),
            removed_chain_blocks_acceptance_data: Arc::new(Vec::new()),
        };

        txindex.write().update_via_virtual_chain_changed(init_virtual_chain_changed_notification).unwrap();

        assert_equal_along_virtual_chain(&virtual_chain, tc.clone(), txindex.clone());
        assert_eq!(txindex.write().count_block_acceptance_offsets().unwrap(), 3);
        assert_eq!(txindex.write().count_accepted_tx_offsets().unwrap(), 12);

        let acceptance_data_h = Arc::new(vec![
            MergesetBlockAcceptanceData {
                block_hash: block_h,
                accepted_transactions: vec![
                    TxEntry { transaction_id: tx_h_1, index_within_block: 0 },
                    TxEntry { transaction_id: tx_h_2, index_within_block: 1 },
                    TxEntry { transaction_id: tx_h_3, index_within_block: 2 },
                    TxEntry { transaction_id: tx_h_4, index_within_block: 3 },
                ],
            },
            MergesetBlockAcceptanceData {
                block_hash: block_hh,
                accepted_transactions: vec![
                    TxEntry { transaction_id: tx_hh_1, index_within_block: 0 },
                    TxEntry { transaction_id: tx_hh_2, index_within_block: 1 },
                    TxEntry { transaction_id: tx_hh_3, index_within_block: 2 },
                    TxEntry { transaction_id: tx_hh_4, index_within_block: 3 },
                ],
            },
        ]);

        let acceptance_data_i = Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: block_i,
            accepted_transactions: vec![
                TxEntry { transaction_id: tx_i_1, index_within_block: 0 },
                TxEntry { transaction_id: tx_i_2, index_within_block: 1 },
                TxEntry { transaction_id: tx_i_3, index_within_block: 2 },
                TxEntry { transaction_id: tx_i_4, index_within_block: 3 },
            ],
        }]);

        let virtual_chain = ChainPath { added: vec![block_h, block_i], removed: vec![block_a, block_b] };

        // Define the notification:
        let test_vspcc_change_notification = VirtualChainChangedNotification {
            added_chain_block_hashes: virtual_chain.added.clone().into(),
            added_chain_blocks_acceptance_data: Arc::new(vec![acceptance_data_h.clone(), acceptance_data_i.clone()]),
            removed_chain_block_hashes: virtual_chain.removed.clone().into(),
            removed_chain_blocks_acceptance_data: Arc::new(vec![acceptance_data_a.clone(), acceptance_data_b.clone()]),
        };

        let mut batch = WriteBatch::default();
        tc.acceptance_data_store.insert_batch(&mut batch, block_h, acceptance_data_h.clone()).unwrap();
        tc.acceptance_data_store.insert_batch(&mut batch, block_i, acceptance_data_i.clone()).unwrap();
        let mut state = VirtualState::default();
        state.ghostdag_data.selected_parent = block_i;
        tc.virtual_stores.write().state.set_batch(&mut batch, Arc::new(state)).unwrap();
        tc_db.write(batch).unwrap();

        txindex.write().update_via_virtual_chain_changed(test_vspcc_change_notification).unwrap();

        assert_equal_along_virtual_chain(&virtual_chain, tc.clone(), txindex.clone());
        assert_eq!(txindex.write().count_block_acceptance_offsets().unwrap(), 3);
        assert_eq!(txindex.write().count_accepted_tx_offsets().unwrap(), 12);

        let prune_notification = ChainAcceptanceDataPrunedNotification {
            chain_hash_pruned: block_h,
            mergeset_block_acceptance_data_pruned: acceptance_data_h.clone(),
            history_root: block_i,
        };

        let virtual_chain = ChainPath { added: vec![block_i], removed: vec![] };

        let mut batch = WriteBatch::default();
        tc.acceptance_data_store.delete_batch(&mut batch, block_h).unwrap();
        tc.pruning_point_store.write().set_history_root(&mut batch, block_i).unwrap();
        tc_db.write(batch).unwrap();

        txindex.write().update_via_chain_acceptance_data_pruned(prune_notification).unwrap();

        assert_equal_along_virtual_chain(&virtual_chain, tc, txindex.clone());
        assert_eq!(txindex.write().count_block_acceptance_offsets().unwrap(), 1);
        assert_eq!(txindex.write().count_accepted_tx_offsets().unwrap(), 4);
    }
}
