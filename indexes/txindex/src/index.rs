use std::{
    fmt::Display,
    sync::{Arc, Weak},
    time::Duration,
};

use crate::{
    api::TxIndexApi,
    errors::TxIndexResult,
    model::{
        bluescore_refs::{BlueScoreAcceptingRefData, DaaScoreIncludingRefData},
        transactions::{TxAcceptanceData, TxInclusionData},
    },
    reindexer::{
        block_reindexer,
        mergeset_reindexer::{self},
    },
    stores::{
        pruning_sync::{PruningData, ToPruneStore},
        store_manager::Store,
    },
    IDENT,
};
use itertools::Itertools;
use kaspa_consensus_core::{
    acceptance_data::{AcceptanceData, MergesetIndexType},
    block::Block,
    tx::TransactionId,
    BlockHashSet, Hash,
};
use kaspa_consensus_notify::notification::{
    BlockAddedNotification, RetentionRootChangedNotification, VirtualChainChangedNotification,
};
use kaspa_consensusmanager::{ConsensusManager, ConsensusResetHandler};
use kaspa_core::{debug, info};
use kaspa_database::prelude::DB;
use parking_lot::RwLock;
use tokio::sync::Mutex as AsyncMutex;

const RESYNC_ACCEPTANCE_DATA_CHUNK_SIZE: u64 = 2048;
const RESYNC_INCLUSION_DATA_CHUNK_SIZE: u64 = 2048;
const PRUNING_CHUNK_SIZE: u64 = 2048;
pub const PRUNING_WAIT_INTERVAL: Duration = Duration::from_millis(15);

pub struct TxIndex {
    consensus_manager: Arc<ConsensusManager>,
    pruning_lock: Arc<AsyncMutex<()>>,
    store: Store,
}

impl TxIndex {
    pub fn new(consensus_manager: Arc<ConsensusManager>, db: Arc<DB>) -> TxIndexResult<Arc<RwLock<Self>>> {
        debug!("[{}]Creating new TxIndex", IDENT);
        let mut txindex =
            Self { consensus_manager: consensus_manager.clone(), pruning_lock: Arc::new(AsyncMutex::new(())), store: Store::new(db) };
        if !txindex.is_synced()? {
            info!("[{}] TxIndex is not synced, starting resync", IDENT);
            txindex.resync_all_from_scratch()?;
        } else {
            info!("[{}] TxIndex is synced", IDENT);
        }
        let txindex = Arc::new(RwLock::new(txindex));
        consensus_manager.register_consensus_reset_handler(Arc::new(TxIndexConsensusResetHandler::new(Arc::downgrade(&txindex))));
        Ok(txindex)
    }
}

impl TxIndexApi for TxIndex {
    fn get_accepted_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxAcceptanceData>> {
        debug!("[{}] Getting accepted transaction data for txid: {}", IDENT, txid);
        Ok(self.store.get_accepted_transaction_data(txid)?)
    }

    fn get_included_transaction_data(&self, txid: TransactionId) -> TxIndexResult<Vec<TxInclusionData>> {
        debug!("[{}] Getting included transaction data for txid: {}", IDENT, txid);
        Ok(self.store.get_included_transaction_data(txid)?)
    }

    fn update_via_block_added(&mut self, block_added_notification: BlockAddedNotification) -> TxIndexResult<()> {
        debug!("[{}] Updating via block added notification: {:?}", IDENT, block_added_notification.block.hash());

        if block_added_notification.block.is_header_only() || block_added_notification.block.transactions.is_empty() {
            debug!("[{}] Skipping header-only block: {}", self, block_added_notification.block.hash());
            return Ok(());
        };
        let reindexed_block_added_state = block_reindexer::reindex_block_added_notification(&block_added_notification);
        Ok(self.store.update_via_reindexed_block_added_state(reindexed_block_added_state)?)
    }

    fn update_via_virtual_chain_changed(
        &mut self,
        virtual_chain_changed_notification: VirtualChainChangedNotification,
    ) -> TxIndexResult<()> {
        if virtual_chain_changed_notification.added_chain_block_hashes.is_empty()
            || virtual_chain_changed_notification.added_chain_blocks_acceptance_data.is_empty()
        {
            debug!("[{}] Skipping virtual chain changed notification with no added or removed blocks", self);
            return Ok(());
        }
        debug!(
            "[{}] Updating via virtual chain changed notification with sink: {:?}",
            self,
            virtual_chain_changed_notification.added_chain_block_hashes.last().unwrap()
        );
        let reindexerd_virtual_changed_state =
            mergeset_reindexer::reindex_virtual_changed_notification(&virtual_chain_changed_notification);
        Ok(self.store.update_via_reindexed_virtual_chain_changed_state(reindexerd_virtual_changed_state)?)
    }

    fn update_via_retention_root_changed(
        &mut self,
        retention_root_changed_notification: RetentionRootChangedNotification,
    ) -> TxIndexResult<()> {
        debug!(
            "[{}] Updating via retention root changed to root: {} with blue score: {}",
            self, retention_root_changed_notification.retention_root, retention_root_changed_notification.retention_root_blue_score
        );
        Ok(self.store.update_to_new_retention_root(
            retention_root_changed_notification.retention_root,
            retention_root_changed_notification.retention_root_blue_score,
            retention_root_changed_notification.retention_root_daa_score,
        )?)
    }

    /// Ranges are inclusive
    fn get_transaction_inclusion_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
        limit: Option<usize>,
        limit_to_blue_score_boundry: bool,
    ) -> TxIndexResult<Vec<DaaScoreIncludingRefData>> {
        debug!("[{}] Getting transaction inclusion data by blue score range: {} to {}", self, from, to);
        Ok(self.store.get_transaction_inclusion_data_by_blue_score_range(from..=to, limit, limit_to_blue_score_boundry)?)
    }
    /// Ranges are inclusive
    fn get_transaction_acceptance_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
        limit: Option<usize>,
        limit_to_blue_score_boundry: bool,
    ) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>> {
        debug!("[{}] Getting transaction acceptance data by blue score range: {} to {}", self, from, to);
        Ok(self.store.get_transaction_acceptance_data_by_blue_score_range(from..=to, limit, limit_to_blue_score_boundry)?)
    }

    fn get_sink_with_blue_score(&self) -> TxIndexResult<(Hash, u64)> {
        debug!("[{}] Getting sink with blue score", IDENT);
        Ok(self.store.get_sink_with_blue_score()?.unwrap())
    }

    fn is_acceptance_data_synced(&self) -> TxIndexResult<bool> {
        debug!("[{}] Checking if acceptance data is synced", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_sink = session.get_sink();
        let txindex_sink_with_blue_score = self.store.get_sink_with_blue_score()?;

        Ok(txindex_sink_with_blue_score.is_some_and(|(tx_sink, _)| tx_sink == consensus_sink))
    }

    fn is_inclusion_data_synced(&self) -> TxIndexResult<bool> {
        debug!("[{}] Checking if inclusion data is synced", IDENT);
        // check if our inclusion data tips match the consensus tips
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_tips = Arc::new(session.get_tips().into_iter().collect::<BlockHashSet>());
        let txindex_tips = self.store.get_tips()?;

        Ok(txindex_tips.is_some_and(|txindex_tips| txindex_tips == consensus_tips))
    }

    fn is_retention_synced(&self) -> TxIndexResult<bool> {
        debug!("[{}] Checking if retention is synced", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_retention_root = session.get_retention_period_root();
        let consensus_retention_root_header = session.get_header(consensus_retention_root)?;
        let consensus_retention_root_blue_score = consensus_retention_root_header.blue_score;
        let consensus_retention_root_daa_score = consensus_retention_root_header.daa_score;
        let txindex_retention_root = self.store.get_retention_root()?;
        let txindex_retention_root_blue_score = self.store.get_retention_root_blue_score()?;
        let txindex_retention_root_daa_score = self.store.get_retention_root_daa_score()?;
        let txindex_next_to_prune_blue_score = self.store.get_next_to_prune_blue_score()?;
        let txindex_next_to_prune_daa_score = self.store.get_next_to_prune_daa_score()?;

        Ok(txindex_retention_root.is_some_and(|trr| {
            trr == consensus_retention_root
                && txindex_retention_root_blue_score.is_some_and(|trrb| trrb == consensus_retention_root_blue_score)
                && txindex_retention_root_daa_score.is_some_and(|trrd| trrd == consensus_retention_root_daa_score)
                && txindex_next_to_prune_blue_score.is_some_and(|tnp| tnp == consensus_retention_root_blue_score)
                && txindex_next_to_prune_daa_score.is_some_and(|tnp| tnp == consensus_retention_root_daa_score)
        }))
    }

    fn is_synced(&self) -> TxIndexResult<bool> {
        debug!(
            "[{}] Checking if TxIndex is synced: acceptance data synced: {}, inclusion data synced: {}, retention synced: {}",
            IDENT,
            self.is_acceptance_data_synced()?,
            self.is_inclusion_data_synced()?,
            self.is_retention_synced()?,
        );
        Ok(self.is_acceptance_data_synced()? && self.is_inclusion_data_synced()? && self.is_retention_synced()?)
    }

    fn resync_all_from_scratch(&mut self) -> TxIndexResult<()> {
        if !self.is_acceptance_data_synced()? {
            self.resync_acceptance_data_from_scratch()?;
        };
        if !self.is_inclusion_data_synced()? {
            self.resync_inclusion_data_from_scratch()?;
        };
        if !self.is_retention_synced()? {
            self.resync_retention_data_from_scratch()?;
        };
        Ok(())
    }

    fn resync_acceptance_data_from_scratch(&mut self) -> TxIndexResult<()> {
        info!("[{}] Resyncing acceptance data from scratch", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());
        let acceptance_iterator = session.get_acceptance_data_iterator();
        let mut start_ts = std::time::Instant::now();
        let mut chunks_processed = 0;
        for chunk in &acceptance_iterator.into_iter().chunks(RESYNC_ACCEPTANCE_DATA_CHUNK_SIZE as usize) {
            debug!("[{}] Resyncing acceptance data chunk: {}", IDENT, chunks_processed + 1);
            // split chunk into hashes and acceptance data
            let (hashes, acceptance_data): (Vec<Hash>, Vec<Arc<AcceptanceData>>) = chunk.unzip();

            // Prefetch blue scores so we don't capture or move `session` into iterator closures
            let blue_scores: Vec<u64> = hashes.iter().map(|h| session.get_header(*h).unwrap().blue_score).collect();

            let reindexed_virtual_changed_state = acceptance_data
                .iter()
                .zip(hashes.iter())
                .zip(blue_scores.iter())
                .enumerate()
                .flat_map(move |(merged_index, ((acceptance_data_arc, hash), blue_score))| {
                    acceptance_data_arc.iter().map(move |mbad| {
                        mergeset_reindexer::reindex_mergeset_acceptance_data(
                            hash,
                            *blue_score,
                            merged_index as MergesetIndexType,
                            mbad,
                        )
                    })
                })
                .collect::<Vec<_>>();

            self.store.update_with_reindexed_mergeset_states(reindexed_virtual_changed_state)?;

            chunks_processed += 1;
            if start_ts.elapsed() >= Duration::from_secs(5) {
                info!(
                    "[{}] Resynced acceptance processed: {}, {:.2} items/sec",
                    IDENT,
                    chunks_processed * RESYNC_ACCEPTANCE_DATA_CHUNK_SIZE,
                    (chunks_processed * RESYNC_ACCEPTANCE_DATA_CHUNK_SIZE) as f64 / start_ts.elapsed().as_secs_f64(),
                );
                start_ts = std::time::Instant::now();
            }
        }
        let consensus_sink = session.get_sink();
        let consensus_sink_blue_score = session.get_header(consensus_sink)?.blue_score;
        self.store.set_sink(consensus_sink, consensus_sink_blue_score)?;
        info!(
            "[{}] Resynced acceptance data completed: {} chunks processed in {:.2} seconds",
            IDENT,
            chunks_processed,
            start_ts.elapsed().as_secs_f64()
        );
        Ok(())
    }

    fn resync_inclusion_data_from_scratch(&mut self) -> TxIndexResult<()> {
        info!("[{}] Resyncing inclusion data from scratch", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());
        let block_iterator = session.get_block_transaction_iterator();
        // chunk into RESYNC_INCLUSION_DATA_CHUNK_SIZE
        let mut chunks_processed = 0;
        let mut start_ts = std::time::Instant::now();
        for chunk in &block_iterator.into_iter().chunks(RESYNC_INCLUSION_DATA_CHUNK_SIZE as usize) {
            debug!("[{}] Resyncing inclusion data chunk: {}", IDENT, chunks_processed + 1);
            let blocks: Vec<Block> =
                chunk.map(|(hash, transactions)| Block::from_arcs(session.get_header(hash).unwrap(), transactions)).collect();
            let reindexed_block_body_states =
                block_reindexer::reindex_blocks(blocks.iter()).map(|state| state.body).collect::<Vec<_>>();
            self.store.update_with_reindexed_block_body_states(reindexed_block_body_states)?;
            chunks_processed += 1;
            if start_ts.elapsed() >= Duration::from_secs(5) {
                info!(
                    "[{}] Resynced inclusion processed: {}, {:.2} items/sec",
                    IDENT,
                    chunks_processed * RESYNC_INCLUSION_DATA_CHUNK_SIZE,
                    (chunks_processed * RESYNC_INCLUSION_DATA_CHUNK_SIZE) as f64 / start_ts.elapsed().as_secs_f64(),
                );
                start_ts = std::time::Instant::now();
            }
        }

        let consensus_tips = session.get_tips().into_iter().collect::<BlockHashSet>();
        self.store.init_tips(consensus_tips)?;

        info!(
            "[{}] Resynced inclusion data completed: {} chunks processed in {:.2} seconds",
            IDENT,
            chunks_processed,
            start_ts.elapsed().as_secs_f64()
        );
        Ok(())
    }

    fn get_pruning_lock(&self) -> Arc<AsyncMutex<()>> {
        self.pruning_lock.clone()
    }

    fn prune_batch(&mut self) -> TxIndexResult<bool> {
        debug!("[{}] Pruning TxIndex", IDENT);

        match self.store.get_next_to_prune_store()?.unwrap() {
            ToPruneStore::AcceptanceData => {
                let txindex_retention_root_blue_score = self.store.get_retention_root_blue_score()?.unwrap();
                let next_to_prune_blue_score = self.store.get_next_to_prune_blue_score()?.unwrap();
                self.store.prune_acceptance_data_from_blue_score(
                    next_to_prune_blue_score,
                    txindex_retention_root_blue_score,
                    Some(PRUNING_CHUNK_SIZE as usize),
                )?;
                if self.store.is_inclusion_pruning_done()? {
                    self.store.set_next_to_prune_store(ToPruneStore::AcceptanceData)?;
                } else {
                    self.store.set_next_to_prune_store(ToPruneStore::InclusionData)?;
                }
            }
            ToPruneStore::InclusionData => {
                let txindex_retention_root_daa_score = self.store.get_retention_root_daa_score()?.unwrap();
                let next_to_prune_daa_score = self.store.get_next_to_prune_daa_score()?.unwrap();
                self.store.prune_inclusion_data_from_daa_score(
                    next_to_prune_daa_score,
                    txindex_retention_root_daa_score,
                    Some(PRUNING_CHUNK_SIZE as usize),
                )?;
                if self.store.is_acceptance_pruning_done()? {
                    self.store.set_next_to_prune_store(ToPruneStore::InclusionData)?;
                } else {
                    self.store.set_next_to_prune_store(ToPruneStore::AcceptanceData)?;
                }
            }
        }
        Ok(self.store.is_acceptance_pruning_done()? && self.store.is_inclusion_pruning_done()?)
    }

    fn resync_retention_data_from_scratch(&mut self) -> TxIndexResult<()> {
        info!("[{}] Pruning TxIndex", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_retention_root = session.get_retention_period_root();
        let consensus_retention_root_header = session.get_header(consensus_retention_root).unwrap();
        let consensus_retention_root_blue_score = consensus_retention_root_header.blue_score;
        let consensus_retention_root_daa_score = consensus_retention_root_header.daa_score;
        let txindex_retention_root = self.store.get_retention_root()?;

        if txindex_retention_root.is_none() || txindex_retention_root.is_some_and(|trr| trr != consensus_retention_root) {
            self.store.set_new_pruning_data(PruningData::new(
                consensus_retention_root,
                consensus_retention_root_blue_score,
                consensus_retention_root_daa_score,
                0u64,
                0u64,
                ToPruneStore::AcceptanceData,
            ))?;
        }

        let mut start_ts = std::time::Instant::now();
        let mut chunks_processed = 0;
        // Prune in batches until done
        while !self.prune_batch()? {
            chunks_processed += 1;
            if start_ts.elapsed() >= Duration::from_secs(5) {
                info!(
                    "[{}] Pruned: {}, {:.2} items/sec",
                    IDENT,
                    chunks_processed * PRUNING_CHUNK_SIZE,
                    (chunks_processed * PRUNING_CHUNK_SIZE) as f64 / start_ts.elapsed().as_secs_f64(),
                );
                start_ts = std::time::Instant::now();
            }
            continue;
        }

        info!("[{}] Pruning completed", IDENT,);
        Ok(())
    }
}

impl std::fmt::Debug for TxIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(IDENT).finish()
    }
}

impl Display for TxIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", IDENT)
    }
}

struct TxIndexConsensusResetHandler {
    txindex: Weak<RwLock<TxIndex>>,
}

impl TxIndexConsensusResetHandler {
    fn new(txindex: Weak<RwLock<TxIndex>>) -> Self {
        Self { txindex }
    }
}

impl ConsensusResetHandler for TxIndexConsensusResetHandler {
    fn handle_consensus_reset(&self) {
        if let Some(txindex) = self.txindex.upgrade() {
            let mut txindex_write = txindex.write();

            if !txindex_write.is_synced().unwrap() {
                info!("[{}] TxIndex is not synced, starting resync", IDENT);
                txindex_write.resync_all_from_scratch().unwrap();
            } else {
                info!("[{}] TxIndex is synced", IDENT);
            };
        }
    }
}
