use std::{fmt::Debug, sync::{atomic::AtomicBool, Arc}};

use kaspa_consensus_core::tx::TransactionId;
use kaspa_consensus_notify::notification::{
    PruningPointBlueScoreChangedNotification as ConsensusPruningPointBlueScoreChangedNotification,
    VirtualChainChangedNotification as ConsensusVirtualChainChangedNotification,
};
use kaspa_consensusmanager::{ConsensusManager, ConsensusSessionBlocking};
use kaspa_core::{debug, error, info, trace};
use kaspa_database::prelude::DB;
use kaspa_hashes::Hash;
use kaspa_index_core::{
    models::txindex::{BlockAcceptanceOffset, TxIndexPruningState, TxOffset},
    reindexers::txindex::TxIndexReindexer,
};
use parking_lot::RwLock;
use rocksdb::WriteBatch;

use crate::{
    config::Config as TxIndexConfig,
    core::{api::TxIndexApi, errors::TxIndexResult},
    stores::{
        TxIndexAcceptedTxEntriesIterator, TxIndexAcceptedTxEntriesReader, TxIndexAcceptedTxEntriesStore, TxIndexAcceptedTxOffsetsReader, TxIndexAcceptedTxOffsetsStore, TxIndexBlockAcceptanceOffsetsReader, TxIndexBlockAcceptanceOffsetsStore, TxIndexPruningStateReader, TxIndexPruningStateStore, TxIndexSinkDataReader, TxIndexSinkReader, TxIndexSinkStore, TxIndexSourceReader, TxIndexSourceStore, TxIndexStores, TxIndexTxEntriesIterator, TxIndexTxEntriesReader, TxIndexTxEntriesStore
    },
    IDENT,
};

pub struct TxIndex {
    stores: TxIndexStores,
    consensus_manager: Arc<ConsensusManager>,
    config: Arc<TxIndexConfig>, // move into config, once txindex is configurable.
    is_pruning: AtomicBool,
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
                    let mut batch = WriteBatch::default();
                    txindex.stores.delete_all(&mut batch)?; // we try and delete all, in order to remove any partial data that may have been written.
                    txindex.stores.write_batch(batch)?;
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
            "[{0}] Updating: Reindex Removed: {1} Blocks with {2} Txs, with source => {3:?}",
            IDENT,
            reindexer.block_acceptance_offsets_changes.removed.len(),
            reindexer.tx_offset_changes.removed.len(),
            reindexer.source,
        );

        let mut batch = WriteBatch::default();

        self.stores.accepted_tx_offsets_store.write_diff_batch(&mut batch, reindexer.tx_offset_changes)?;
        self.stores.block_acceptance_offsets_store.write_diff_batch(&mut batch, reindexer.block_acceptance_offsets_changes)?;

        if let Some(source) = reindexer.source {
            self.stores.source_store.set(&mut batch, source)?;
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
        let mut is_end = false;
        let mut is_start = true;

        while !is_end {
            let mut to_process_hashes =
                session.get_virtual_chain_from_block(sync_from, Some(sync_to), self.config.perf.resync_chunksize)?.added;

            if is_start {
                // Prepend `sync_from` as `get_virtual_chain_from_block` does not include the from block.
                // this is important only in the first iteration.
                assert!(to_process_hashes.first() != Some(&sync_from)); // sanity check
                to_process_hashes.insert(0, sync_from);
                is_start = false;
            }

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

            is_end = sync_from == sync_to;

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


    fn prune_below_blue_score(&self, accepting_blue_score: u64) -> TxIndexResult<()> {
        info!(
            "[{0}] Pruning: Removing all txs with accepting blue score below: {1}",
            IDENT, accepting_blue_score
        );
        let pruning_state = self.stores.pruning_state_store().get()?.expect("expected a pruning state to be set before pruning");
        // Note: this is approximate, as new txs may be added to the txindex during the it's pruning process, these are not accounted for. 
        let approx_to_scan_count = self.stores.accepted_tx_entries_store.num_of_entries()? - pruning_state.last_prune_count();
        let mut is_start = true;
        let mut is_end = false;
        let mut total_tx_entries_processed = (0u64, 0u64); // .0 holds the value of the former display
        let mut total_txs_pruned= (0u64, 0u64); // .0 holds the value of the former display
        let mut percent_completed = (0f64, 0f64); // .0 holds the value of the former display
        let percent_display_granularity = 1.0; // in percent
        let mut instant = std::time::Instant::now();
        let mut is_end = false;
        let mut is_start = true;
        
        while !is_end{
            let filtered_count = 0u64;
            let last_transaction_scanned: TransactionId;
            let entry_iter = self.stores.tx_entries_store().seek_iterator(pruning_state.last_pruned_transaction(), self.config.perf.pruning_chunksize_units, pruning_state.last_pruned_transaction().is_none());
            let last_transaction_scanned = tx_entries_batch.last();
            total_txs_pruned = tx_entries_batch.len() as u64;
            total_tx_entries_processed.1 += (tx_entries_batch.len() as u64 + filtered_count);
            is_end = last_transaction_scanned.is_none();
            let mut batch = WriteBatch::default();
            self.stores.tx_entries_store().remove_many(&mut batch, tx_entries_batch.into_iter())?;
            self.stores.pruning_state_store().update(&mut batch, move |pruning_state| {
                pruning_state.last_prune_count = total_tx_entries_processed.1;
                pruning_state.last_pruned_transaction = last_transaction_scanned;
                pruning_state
            })?;
            if percent_completed.0 + percent_display_granularity <= percent_completed.1 || is_end {
                let total_txs_processed_diff = total_txs_processed.1 - total_txs_processed.0;
                info!(
                    "[{0}] Pruning Txs: Pruned: {1} ({2:.0}/s); Processed: {3} ({4:.0}/s);, Remaining: {5}; {6:.0}%",
                    IDENT,
                    total_txs_pruned,
                    total_txs_processed.1,
                    approx_to_scan_count - total_tx_entries_processed.1,
                    total_txs_processed_diff as f64 / instant.elapsed().as_secs_f64(),
                    if is_end { 100.0 } else { percent_completed.1 },
                );
                percent_completed.0 = percent_completed.1;
                total_txs_pruned.0 = total_txs_pruned.1;
                total_txs_processed.0 = total_txs_processed.1;
                instant = std::time::Instant::now();
            }
            if is_end {
                break;
            } else {
                is_start = false;
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
        let txindex_source = self.stores.source_store.get()?;
        let consensus_source = session.get_source();
        let txindex_sink = self.stores.sink_store()?.get().hash;
        let consensus_sink = session.get_sink();

        let reset_db = {
            debug!("[{0}] Reset db Check with: Consensus source {1:?}, txindex_source {2:?}", IDENT, consensus_source, txindex_source);
            if let Some(txindex_source) = txindex_source {
                txindex_source != consensus_source
            } else {
                true
            }
        };

        if reset_db {
            debug!("[{0}] Reset db Check failed - resetting the db", IDENT);
            let mut batch = WriteBatch::default();
            self.stores.delete_all(&mut batch)?;
            self.stores.source_store.set(&mut batch, consensus_source)?;
            self.stores.write_batch(batch)?;
        }

        let resync_points = if reset_db || txindex_sink.is_none() {
            debug!("[{0}] Resyncing from Consensus source to Consensus sink", IDENT);
            (consensus_source, consensus_sink)
        } else {
            // We may unwrap txindex sink as we check is None in if condition.
            if session.is_chain_block(txindex_sink.unwrap())? {
                debug!("[{0}] Resyncing from Txindex sink to Consensus sink", IDENT);
                (txindex_sink.unwrap(), consensus_sink)
            } else {
                debug!("[{0}] Resyncing from some common sink ancestor between Consensus and Txindex sink", IDENT);
                (session.find_highest_common_chain_block(txindex_sink.unwrap(), consensus_sink)?, consensus_sink)
            }
        };

        let unsync_points = if txindex_sink.is_some() && !session.is_chain_block(txindex_sink.unwrap())? {
            // We may unwrap txindex sink as we check is Some in if condition.
            debug!("[{0}] Unsycing from reorged txindex sink", IDENT);
            Some((txindex_sink.unwrap(), session.find_highest_common_chain_block(txindex_sink.unwrap(), consensus_sink)?))
        } else {
            None
        };

        if let Some(unsync_points) = unsync_points {
            debug!("{0} Unsyncing Reorged sink: {1} => {2}", IDENT, unsync_points.0, unsync_points.1);
            self.sync_segement(unsync_points.0, unsync_points.1, true, &session)?;
        }

        debug!("{0} Resyncing along virtual selected parent chain: {1} => {2}", IDENT, resync_points.0, resync_points.1);
        self.sync_segement(resync_points.0, resync_points.1, false, &session)?;
        Ok(())
    }

    // Sync state methods
    fn is_synced(&self) -> TxIndexResult<bool> {
        trace!("[{0}] checking sync status...", IDENT);

        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        if let Some(txindex_sink) = self.stores.sink_store().get()? {
            if txindex_sink == session.get_sink() {
                if let Some(txindex_source) = self.stores.source_store.get()? {
                    if txindex_source == session.get_source() {
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

    fn update_via_pruning_point_blue_score_changed(
            &mut self,
            ppbsc_notification: ConsensusPruningPointBlueScoreChangedNotification,
        ) -> TxIndexResult<()> {
            trace!(
                "[{0}] Updating: Pruning point blue score changed to: {1} - via pruning point blue score changed notification",
                IDENT,
                ppbsc_notification.blue_score
            );

            self.prune_below_accepting_blue_score(ppbsc_notification.blue_score)
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
        Ok(self.stores.sink_store().get()?)
    }

    fn get_source(&self) -> TxIndexResult<Option<Hash>> {
        trace!("[{0}] Getting: History root", IDENT);
        Ok(self.stores.source_store.get()?)
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
        acceptance_data::{MergesetBlockAcceptanceData, AcceptedTxEntry},
        api::ConsensusApi,
        config::Config as ConsensusConfig,
        tx::TransactionId,
        ChainPath,
    };
    use kaspa_consensus_notify::notification::{PruningPointAdvancementNotification, VirtualChainChangedNotification};
    use kaspa_consensusmanager::ConsensusManager;

    use kaspa_database::{create_temp_db, prelude::ConnBuilder};
    use kaspa_hashes::{Hash, ZERO_HASH};

    use kaspa_index_core::models::txindex::AcceptanceDataIndexType;
    use parking_lot::RwLock;

    use rocksdb::WriteBatch;

    use crate::{api::TxIndexApi, config::Config as TxIndexConfig, TxIndex};

    fn assert_equal_along_virtual_chain(virtual_chain: &ChainPath, test_consensus: Arc<TestConsensus>, txindex: Arc<RwLock<TxIndex>>) {
        assert!(txindex.write().is_synced().unwrap());
        assert_eq!(txindex.write().get_sink().unwrap().unwrap(), test_consensus.get_sink());
        assert_eq!(txindex.write().get_source().unwrap().unwrap(), test_consensus.get_source());

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
                    AcceptedTxEntry { transaction_id: tx_a_1, index_within_block: 0 },
                    AcceptedTxEntry { transaction_id: tx_a_2, index_within_block: 1 },
                    AcceptedTxEntry { transaction_id: tx_a_3, index_within_block: 2 },
                    AcceptedTxEntry { transaction_id: tx_a_4, index_within_block: 3 },
                ],
            },
            MergesetBlockAcceptanceData {
                block_hash: block_aa,
                accepted_transactions: vec![
                    AcceptedTxEntry { transaction_id: tx_aa_1, index_within_block: 0 },
                    AcceptedTxEntry { transaction_id: tx_aa_2, index_within_block: 1 },
                    AcceptedTxEntry { transaction_id: tx_aa_3, index_within_block: 2 },
                    AcceptedTxEntry { transaction_id: tx_aa_4, index_within_block: 3 },
                ],
            },
        ]);

        let acceptance_data_b = Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: block_b,
            accepted_transactions: vec![
                AcceptedTxEntry { transaction_id: tx_b_1, index_within_block: 0 },
                AcceptedTxEntry { transaction_id: tx_b_2, index_within_block: 1 },
                AcceptedTxEntry { transaction_id: tx_b_3, index_within_block: 2 },
                AcceptedTxEntry { transaction_id: tx_b_4, index_within_block: 3 },
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
                    AcceptedTxEntry { transaction_id: tx_h_1, index_within_block: 0 },
                    AcceptedTxEntry { transaction_id: tx_h_2, index_within_block: 1 },
                    AcceptedTxEntry { transaction_id: tx_h_3, index_within_block: 2 },
                    AcceptedTxEntry { transaction_id: tx_h_4, index_within_block: 3 },
                ],
            },
            MergesetBlockAcceptanceData {
                block_hash: block_hh,
                accepted_transactions: vec![
                    AcceptedTxEntry { transaction_id: tx_hh_1, index_within_block: 0 },
                    AcceptedTxEntry { transaction_id: tx_hh_2, index_within_block: 1 },
                    AcceptedTxEntry { transaction_id: tx_hh_3, index_within_block: 2 },
                    AcceptedTxEntry { transaction_id: tx_hh_4, index_within_block: 3 },
                ],
            },
        ]);

        let acceptance_data_i = Arc::new(vec![MergesetBlockAcceptanceData {
            block_hash: block_i,
            accepted_transactions: vec![
                AcceptedTxEntry { transaction_id: tx_i_1, index_within_block: 0 },
                AcceptedTxEntry { transaction_id: tx_i_2, index_within_block: 1 },
                AcceptedTxEntry { transaction_id: tx_i_3, index_within_block: 2 },
                AcceptedTxEntry { transaction_id: tx_i_4, index_within_block: 3 },
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

        let prune_notification = PruningPointAdvancementNotificationNotification {
            chain_hash_pruned: block_h,
            mergeset_block_acceptance_data_pruned: acceptance_data_h.clone(),
            source: block_i,
        };

        let virtual_chain = ChainPath { added: vec![block_i], removed: vec![] };

        let mut batch = WriteBatch::default();
        tc.acceptance_data_store.delete_batch(&mut batch, block_h).unwrap();
        tc.pruning_point_store.write().set_batch(&mut batch, block_i, ZERO_HASH, 0).unwrap();
        tc.pruning_point_store.write().set_history_root(&mut batch, block_i).unwrap();
        tc_db.write(batch).unwrap();

        txindex.write().update_via_chain_acceptance_data_pruned(prune_notification).unwrap();

        assert_equal_along_virtual_chain(&virtual_chain, tc, txindex.clone());
        assert_eq!(txindex.write().count_block_acceptance_offsets().unwrap(), 1);
        assert_eq!(txindex.write().count_accepted_tx_offsets().unwrap(), 4);
    }
}
