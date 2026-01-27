use std::{
    fmt::Display,
    sync::{atomic::AtomicBool, Arc, Weak},
    time::Duration,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    acceptance_data::{AcceptanceData, MergesetIndexType},
    block::Block,
    tx::TransactionId,
    BlockHashSet, Hash, HashMapCustomHasher,
};
use kaspa_consensus_notify::notification::{
    BlockAddedNotification, RetentionRootChangedNotification, VirtualChainChangedNotification,
};
use kaspa_consensusmanager::{ConsensusManager, ConsensusResetHandler};
use kaspa_core::{debug, info};
use kaspa_database::prelude::DB;
use parking_lot::RwLock;

use crate::{
    api::TxIndexApi,
    errors::TxIndexResult,
    model::{
        bluescore_refs::{BlueScoreAcceptingRefData, BlueScoreIncludingRefData},
        transactions::{TxAcceptanceData, TxInclusionData},
    },
    reindexer::{
        block_reindexer,
        mergeset_reindexer::{self},
    },
    stores::store_manager::Store,
    IDENT,
};

const RESYNC_ACCEPTANCE_DATA_CHUNK_SIZE: u64 = 2048;
const RESYNC_INCLUSION_DATA_CHUNK_SIZE: u64 = 2048;
const PRUNING_CHUNK_SIZE: u64 = 2048;
pub const PRUNING_WAIT_INTERVAL: Duration = Duration::from_millis(15);

pub struct TxIndex {
    consensus_manager: Arc<ConsensusManager>,
    is_pruning: Arc<AtomicBool>,
    store: Store,
}

impl TxIndex {
    pub fn new(consensus_manager: Arc<ConsensusManager>, db: Arc<DB>) -> TxIndexResult<Arc<RwLock<Self>>> {
        debug!("[{}]Creating new TxIndex", IDENT);
        let mut txindex =
            Self { consensus_manager: consensus_manager.clone(), is_pruning: Arc::new(AtomicBool::new(false)), store: Store::new(db) };
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
        if virtual_chain_changed_notification.added_chain_block_hashes.is_empty() || virtual_chain_changed_notification.added_chain_blocks_acceptance_data.is_empty() {
            debug!(
                "[{}] Skipping virtual chain changed notification with no added or removed blocks",
                self
            );
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
        Ok(self.store.set_retention_root(
            retention_root_changed_notification.retention_root,
            retention_root_changed_notification.retention_root_blue_score,
        )?)
    }

    /// Ranges are inclusive
    fn get_transaction_inclusion_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
        limit: Option<usize>,
    ) -> TxIndexResult<Vec<BlueScoreIncludingRefData>> {
        debug!("[{}] Getting transaction inclusion data by blue score range: {} to {}", self, from, to);
        Ok(self.store.get_transaction_inclusion_data_by_blue_score_range(from..=to, limit)?)
    }
    /// Ranges are inclusive
    fn get_transaction_acceptance_data_by_blue_score_range(
        &self,
        from: u64, // inclusive
        to: u64,   // inclusive
        limit: Option<usize>,
    ) -> TxIndexResult<Vec<BlueScoreAcceptingRefData>> {
        debug!("[{}] Getting transaction acceptance data by blue score range: {} to {}", self, from, to);
        Ok(self.store.get_transaction_acceptance_data_by_blue_score_range(from..=to, limit)?)
    }

    fn is_acceptance_data_synced(&self) -> TxIndexResult<bool> {
        debug!("[{}] Checking if acceptance data is synced", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_sink = session.get_sink();
        let txindex_sink = self.store.get_sink()?;

        Ok(txindex_sink.is_some_and(|tx_sink| tx_sink == consensus_sink))
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
        let consensus_retention_root_blue_score = session.get_header(consensus_retention_root)?.blue_score;
        let txindex_retention_root = self.store.get_retention_root()?;
        let txindex_retention_root_blue_score = self.store.get_retention_root_blue_score()?;
        let txindex_next_to_prune_blue_score = self.store.get_next_to_prune_blue_score()?;

        Ok(txindex_retention_root.is_some_and(|trr| {
            trr == consensus_retention_root
                && txindex_retention_root_blue_score.is_some_and(|trrb| trrb == consensus_retention_root_blue_score)
                && txindex_next_to_prune_blue_score.is_some_and(|tnp| tnp == consensus_retention_root_blue_score)
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

    fn is_pruning(&self) -> bool {
        self.is_pruning.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn toggle_pruning_active(&self, active: bool) {
        self.is_pruning.store(active, std::sync::atomic::Ordering::SeqCst);
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
            let reindexed_virtual_changed_state = acceptance_data
                .iter()
                .zip(hashes.iter())
                .enumerate()
                .flat_map(|(mergeset_index, (mad, hash))| {
                    mad.iter().map(move |mbad| {
                        mergeset_reindexer::reindex_mergeset_acceptance_data(hash, mergeset_index as MergesetIndexType, mbad)
                    })
                })
                .collect();
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
        info!("[{}] Resynced acceptance data completed: {} chunks processed in {:.2} seconds", IDENT, chunks_processed, start_ts.elapsed().as_secs_f64());
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
            // collect blocks
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

        info!("[{}] Resynced inclusion data completed: {} chunks processed in {:.2} seconds", IDENT, chunks_processed, start_ts.elapsed().as_secs_f64());
        Ok(())
    }

    fn prune_on_the_fly(&mut self) -> TxIndexResult<bool> {
        debug!("[{}] Pruning TxIndex on the fly", IDENT);
        let txindex_retention_root_blue_score = self.store.get_retention_root_blue_score()?.unwrap();
        let mut next_to_prune_blue_score = self.store.get_next_to_prune_blue_score()?.unwrap();
        let mut is_store_empty = false;

        (next_to_prune_blue_score, is_store_empty) = self.store.prune_from_blue_score(
            next_to_prune_blue_score,
            txindex_retention_root_blue_score,
            Some(PRUNING_CHUNK_SIZE as usize),
        )?;

        Ok(next_to_prune_blue_score == txindex_retention_root_blue_score || is_store_empty)
    }

    fn resync_retention_data_from_scratch(&mut self) -> TxIndexResult<()> {
        info!("[{}] Pruning TxIndex", IDENT);
        let consensus = self.consensus_manager.consensus();
        let session = futures::executor::block_on(consensus.session_blocking());

        let consensus_retention_root = session.get_retention_period_root();
        let consensus_retention_root_blue_score = session.get_header(consensus_retention_root).unwrap().blue_score;
        let txindex_retention_root = self.store.get_retention_root()?;

        if txindex_retention_root.is_none() || txindex_retention_root.is_some_and(|trr| trr != consensus_retention_root) {
            self.store.set_retention_root(consensus_retention_root, consensus_retention_root_blue_score).unwrap();
            self.store.set_next_to_prune_blue_score(0u64)?;
        }

        let mut next_to_prune_blue_score = self.store.get_next_to_prune_blue_score().unwrap().unwrap_or(u64::MIN);

        // Sanity check, this should always hold true, unless txindex tail end pruned beyond the retention root somehow.
        assert!(next_to_prune_blue_score <= consensus_retention_root_blue_score);

        let mut chunks_processed = 0;
        let mut start_ts = std::time::Instant::now();
        let mut is_store_empty = false;
        if self.is_pruning() {
            info!("[{}] TxIndex is already pruning, skipping resync retention data", IDENT);
            return Ok(());
        };
        self.toggle_pruning_active(true);
        while (next_to_prune_blue_score < consensus_retention_root_blue_score) || is_store_empty {
            debug!("[{}] Pruning TxIndex up to blue score: {}/{}, store: {}", IDENT, next_to_prune_blue_score, consensus_retention_root_blue_score, is_store_empty);
            (next_to_prune_blue_score, is_store_empty) = self.store.prune_from_blue_score(
                next_to_prune_blue_score,
                consensus_retention_root_blue_score,
                Some(PRUNING_CHUNK_SIZE as usize),
            )?;

            chunks_processed += 1;
            if start_ts.elapsed() >= Duration::from_secs(5) {
                info!(
                    "[{}] Pruning processed: {}, {:.2} items/sec",
                    IDENT,
                    chunks_processed * PRUNING_CHUNK_SIZE,
                    (chunks_processed * PRUNING_CHUNK_SIZE) as f64 / start_ts.elapsed().as_secs_f64(),
                );
                start_ts = std::time::Instant::now();
            }
        }
        info!("[{}] Pruning completed: {} chunks processed, in {:.2} seconds", IDENT, chunks_processed, start_ts.elapsed().as_secs_f64());
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
