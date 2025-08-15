pub mod cache_policy_builder;
pub mod ctl;
pub mod factory;
pub mod services;
pub mod storage;
pub mod test_consensus;

#[cfg(feature = "devnet-prealloc")]
mod utxo_set_override;

use crate::{
    config::Config,
    errors::{BlockProcessResult, RuleError},
    model::{
        services::reachability::ReachabilityService,
        stores::{
            acceptance_data::AcceptanceDataStoreReader,
            block_transactions::BlockTransactionsStoreReader,
            ghostdag::{GhostdagData, GhostdagStoreReader},
            headers::{CompactHeaderData, HeaderStoreReader},
            headers_selected_tip::HeadersSelectedTipStoreReader,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::PruningStoreReader,
            pruning_samples::{PruningSamplesStore, PruningSamplesStoreReader},
            reachability::ReachabilityStoreReader,
            relations::RelationsStoreReader,
            statuses::StatusesStoreReader,
            tips::TipsStoreReader,
            utxo_set::{UtxoSetStore, UtxoSetStoreReader},
            DB,
        },
    },
    pipeline::{
        body_processor::BlockBodyProcessor,
        deps_manager::{BlockProcessingMessage, BlockResultSender, BlockTask, VirtualStateProcessingMessage},
        header_processor::HeaderProcessor,
        pruning_processor::processor::{PruningProcessingMessage, PruningProcessor},
        virtual_processor::{errors::PruningImportResult, VirtualStateProcessor},
        ProcessingCounters,
    },
    processes::{
        ghostdag::ordering::SortableBlock,
        window::{WindowManager, WindowType},
    },
};
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    api::{
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
        stats::BlockCount,
        BlockValidationFutures, ConsensusApi, ConsensusStats,
    },
    block::{Block, BlockTemplate, TemplateBuildMode, TemplateTransactionSelector, VirtualStateApproxId},
    blockhash::BlockHashExtensions,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    daa_score_timestamp::DaaScoreTimestamp,
    errors::{
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        difficulty::DifficultyError,
        pruning::PruningImportError,
        tx::TxResult,
    },
    header::Header,
    mass::{ContextualMasses, NonContextualMasses},
    merkle::calc_hash_merkle_root,
    mining_rules::MiningRules,
    muhash::MuHashExtensions,
    network::NetworkType,
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList, PruningProofMetadata},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{MutableTransaction, SignableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    utxo::utxo_inquirer::UtxoInquirerError,
    BlockHashSet, BlueWorkType, ChainPath, HashMapCustomHasher,
};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;

use crossbeam_channel::{
    bounded as bounded_crossbeam, unbounded as unbounded_crossbeam, Receiver as CrossbeamReceiver, Sender as CrossbeamSender,
};
use itertools::Itertools;
use kaspa_consensusmanager::{SessionLock, SessionReadGuard};

use kaspa_database::prelude::{StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_txscript::caches::TxScriptCacheCounters;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, VecDeque},
    future::Future,
    iter::once,
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};
use std::{
    sync::atomic::AtomicBool,
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

use self::{services::ConsensusServices, storage::ConsensusStorage};

use crate::model::stores::selected_chain::SelectedChainStoreReader;

use std::cmp;

pub struct Consensus {
    // DB
    db: Arc<DB>,

    // Channels
    block_sender: CrossbeamSender<BlockProcessingMessage>,

    // Processors
    pub(super) header_processor: Arc<HeaderProcessor>,
    pub(super) body_processor: Arc<BlockBodyProcessor>,
    pub(super) virtual_processor: Arc<VirtualStateProcessor>,
    pub(super) pruning_processor: Arc<PruningProcessor>,

    // Storage
    pub(super) storage: Arc<ConsensusStorage>,

    // Services and managers
    pub(super) services: Arc<ConsensusServices>,

    // Pruning lock
    pruning_lock: SessionLock,

    // Notification management
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Config
    config: Arc<Config>,

    // Other
    creation_timestamp: u64,

    // Signals
    is_consensus_exiting: Arc<AtomicBool>,
}

impl Deref for Consensus {
    type Target = ConsensusStorage;

    fn deref(&self) -> &Self::Target {
        &self.storage
    }
}

impl Consensus {
    pub fn new(
        db: Arc<DB>,
        config: Arc<Config>,
        pruning_lock: SessionLock,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
        tx_script_cache_counters: Arc<TxScriptCacheCounters>,
        creation_timestamp: u64,
        mining_rules: Arc<MiningRules>,
    ) -> Self {
        let params = &config.params;
        let perf_params = &config.perf;
        let is_consensus_exiting: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));

        //
        // Storage layer
        //

        let storage = ConsensusStorage::new(db.clone(), config.clone());

        //
        // Services and managers
        //

        let services = ConsensusServices::new(
            db.clone(),
            storage.clone(),
            config.clone(),
            tx_script_cache_counters,
            is_consensus_exiting.clone(),
        );

        //
        // Processor channels
        //

        let (sender, receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (body_sender, body_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (virtual_sender, virtual_receiver): (
            CrossbeamSender<VirtualStateProcessingMessage>,
            CrossbeamReceiver<VirtualStateProcessingMessage>,
        ) = unbounded_crossbeam();
        let (pruning_sender, pruning_receiver): (
            CrossbeamSender<PruningProcessingMessage>,
            CrossbeamReceiver<PruningProcessingMessage>,
        ) = bounded_crossbeam(2);

        //
        // Thread-pools
        //

        // Pool for header and body processors
        let block_processors_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.block_processors_num_threads)
                .thread_name(|i| format!("block-pool-{i}"))
                .build()
                .unwrap(),
        );
        // We need a dedicated thread-pool for the virtual processor to avoid possible deadlocks probably caused by the
        // combined usage of `par_iter` (in virtual processor) and `rayon::spawn` (in header/body processors).
        // See for instance https://github.com/rayon-rs/rayon/issues/690
        let virtual_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.virtual_processor_num_threads)
                .thread_name(|i| format!("virtual-pool-{i}"))
                .build()
                .unwrap(),
        );

        //
        // Pipeline processors
        //

        let header_processor = Arc::new(HeaderProcessor::new(
            receiver,
            body_sender,
            block_processors_pool.clone(),
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            counters.clone(),
        ));

        let body_processor = Arc::new(BlockBodyProcessor::new(
            body_receiver,
            virtual_sender,
            block_processors_pool,
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            notification_root.clone(),
            counters.clone(),
        ));

        let virtual_processor = Arc::new(VirtualStateProcessor::new(
            virtual_receiver,
            pruning_sender,
            pruning_receiver.clone(),
            virtual_pool,
            params,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            notification_root.clone(),
            counters.clone(),
            mining_rules,
        ));

        let pruning_processor = Arc::new(PruningProcessor::new(
            pruning_receiver,
            db.clone(),
            &storage,
            &services,
            pruning_lock.clone(),
            config.clone(),
            is_consensus_exiting.clone(),
        ));

        // Ensure the relations stores are initialized
        header_processor.init();
        // Ensure that some pruning point is registered
        virtual_processor.init();

        // Ensure that genesis was processed
        if config.process_genesis {
            header_processor.process_genesis();
            body_processor.process_genesis();
            virtual_processor.process_genesis();
        }

        let this = Self {
            db,
            block_sender: sender,
            header_processor,
            body_processor,
            virtual_processor,
            pruning_processor,
            storage,
            services,
            pruning_lock,
            notification_root,
            counters,
            config,
            creation_timestamp,
            is_consensus_exiting,
        };

        // Run database upgrades if any
        this.run_database_upgrades();

        this
    }

    /// A procedure for calling database upgrades which are self-contained (i.e., do not require knowing the DB version)
    fn run_database_upgrades(&self) {
        // Upgrade to initialize the new retention root field correctly
        self.retention_root_database_upgrade();

        // TODO (post HF): remove this upgrade
        // Database upgrade to include pruning samples
        self.pruning_samples_database_upgrade();
    }

    fn retention_root_database_upgrade(&self) {
        let mut pruning_point_store = self.pruning_point_store.write();
        if pruning_point_store.retention_period_root().unwrap_option().is_none() {
            let mut batch = rocksdb::WriteBatch::default();
            if self.config.is_archival {
                // The retention checkpoint is what was previously known as history root
                let retention_checkpoint = pruning_point_store.retention_checkpoint().unwrap();
                pruning_point_store.set_retention_period_root(&mut batch, retention_checkpoint).unwrap();
            } else {
                // For non-archival nodes the retention root was the pruning point
                let pruning_point = pruning_point_store.get().unwrap().pruning_point;
                pruning_point_store.set_retention_period_root(&mut batch, pruning_point).unwrap();
            }
            self.db.write(batch).unwrap();
        }
    }

    fn pruning_samples_database_upgrade(&self) {
        //
        // For the first time this version runs, make sure we populate pruning samples
        // from pov for all qualified chain blocks in the pruning point future
        //

        let sink = self.get_sink();
        if self.storage.pruning_samples_store.pruning_sample_from_pov(sink).unwrap_option().is_some() {
            // Sink is populated so we assume the database is upgraded
            return;
        }

        // Populate past pruning points (including current one)
        for (p1, p2) in (0..=self.pruning_point_store.read().get().unwrap().index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .tuple_windows()
        {
            // Set p[i] to point at p[i-1]
            self.pruning_samples_store.insert(p2, p1).unwrap_or_exists();
        }

        let pruning_point = self.pruning_point();
        let reachability = self.reachability_store.read();

        // We walk up via reachability tree children so that we only iterate blocks B s.t. pruning point ∈ chain(B)
        let mut queue = VecDeque::<Hash>::from_iter(reachability.get_children(pruning_point).unwrap().iter().copied());
        let mut processed = 0;
        kaspa_core::info!("Upgrading database to include and populate the pruning samples store");
        while let Some(current) = queue.pop_front() {
            if !self.get_block_status(current).is_some_and(|s| s == BlockStatus::StatusUTXOValid) {
                // Skip branches of the tree which are not chain qualified.
                // This is sufficient since we will only assume this field exists
                // for such chain qualified blocks
                continue;
            }
            queue.extend(reachability.get_children(current).unwrap().iter());

            processed += 1;

            // Populate the data
            let ghostdag_data = self.ghostdag_store.get_compact_data(current).unwrap();
            let pruning_sample_from_pov =
                self.services.pruning_point_manager.expected_header_pruning_point_v2(ghostdag_data).pruning_sample;
            self.pruning_samples_store.insert(current, pruning_sample_from_pov).unwrap_or_exists();
        }

        kaspa_core::info!("Done upgrading database (populated {} entries)", processed);
    }

    pub fn run_processors(&self) -> Vec<JoinHandle<()>> {
        // Spawn the asynchronous processors.
        let header_processor = self.header_processor.clone();
        let body_processor = self.body_processor.clone();
        let virtual_processor = self.virtual_processor.clone();
        let pruning_processor = self.pruning_processor.clone();

        vec![
            thread::Builder::new().name("header-processor".to_string()).spawn(move || header_processor.worker()).unwrap(),
            thread::Builder::new().name("body-processor".to_string()).spawn(move || body_processor.worker()).unwrap(),
            thread::Builder::new().name("virtual-processor".to_string()).spawn(move || virtual_processor.worker()).unwrap(),
            thread::Builder::new().name("pruning-processor".to_string()).spawn(move || pruning_processor.worker()).unwrap(),
        ]
    }

    /// Acquires a consensus session, blocking data-pruning from occurring until released
    pub fn acquire_session(&self) -> SessionReadGuard<'_> {
        self.pruning_lock.blocking_read()
    }

    fn validate_and_insert_block_impl(
        &self,
        task: BlockTask,
    ) -> (impl Future<Output = BlockProcessResult<BlockStatus>>, impl Future<Output = BlockProcessResult<BlockStatus>>) {
        let (btx, brx): (BlockResultSender, _) = oneshot::channel();
        let (vtx, vrx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender.send(BlockProcessingMessage::Process(task, btx, vtx)).unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::Relaxed);
        (async { brx.await.unwrap() }, async { vrx.await.unwrap() })
    }

    pub fn body_tips(&self) -> BlockHashSet {
        self.body_tips_store.read().get().unwrap().read().clone()
    }

    pub fn block_status(&self, hash: Hash) -> BlockStatus {
        self.statuses_store.read().get(hash).unwrap()
    }

    pub fn session_lock(&self) -> SessionLock {
        self.pruning_lock.clone()
    }

    pub fn notification_root(&self) -> Arc<ConsensusNotificationRoot> {
        self.notification_root.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.counters
    }

    pub fn signal_exit(&self) {
        self.is_consensus_exiting.store(true, Ordering::Relaxed);
        self.block_sender.send(BlockProcessingMessage::Exit).unwrap();
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.signal_exit();
        // Wait for async consensus processors to exit
        for handle in wait_handles {
            handle.join().unwrap();
        }
    }

    /// Validates that a valid block *header* exists for `hash`
    fn validate_block_exists(&self, hash: Hash) -> Result<(), ConsensusError> {
        if match self.statuses_store.read().get(hash).unwrap_option() {
            Some(status) => status.is_valid(),
            None => false,
        } {
            Ok(())
        } else {
            Err(ConsensusError::HeaderNotFound(hash))
        }
    }

    fn estimate_network_hashes_per_second_impl(&self, ghostdag_data: &GhostdagData, window_size: usize) -> ConsensusResult<u64> {
        let window = match self.services.window_manager.block_window(ghostdag_data, WindowType::VaryingWindow(window_size)) {
            Ok(w) => w,
            Err(RuleError::InsufficientDaaWindowSize(s)) => return Err(DifficultyError::InsufficientWindowData(s).into()),
            Err(e) => panic!("unexpected error: {e}"),
        };
        Ok(self.services.window_manager.estimate_network_hashes_per_second(window)?)
    }

    fn pruning_point_compact_headers(&self) -> Vec<(Hash, CompactHeaderData)> {
        // PRUNE SAFETY: index is monotonic and past pruning point headers are expected permanently
        let current_pp_info = self.pruning_point_store.read().get().unwrap();
        (0..current_pp_info.index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(current_pp_info.pruning_point))
            .map(|hash| (hash, self.headers_store.get_compact_header_data(hash).unwrap()))
            .collect_vec()
    }
}

impl ConsensusApi for Consensus {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template(miner_data, tx_selector, build_mode)
    }

    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        let (block_task, virtual_state_task) = self.validate_and_insert_block_impl(BlockTask::Ordinary { block });
        BlockValidationFutures { block_task: Box::pin(block_task), virtual_state_task: Box::pin(virtual_state_task) }
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        let (block_task, virtual_state_task) = self.validate_and_insert_block_impl(BlockTask::Trusted { block: tb.block });
        BlockValidationFutures { block_task: Box::pin(block_task), virtual_state_task: Box::pin(virtual_state_task) }
    }

    fn validate_mempool_transaction(&self, transaction: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        self.virtual_processor.validate_mempool_transaction(transaction, args)?;
        Ok(())
    }

    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        self.virtual_processor.validate_mempool_transactions_in_parallel(transactions, args)
    }

    fn populate_mempool_transaction(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        self.virtual_processor.populate_mempool_transaction(transaction)?;
        Ok(())
    }

    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        self.virtual_processor.populate_mempool_transactions_in_parallel(transactions)
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> NonContextualMasses {
        self.services.mass_calculator.calc_non_contextual_masses(transaction)
    }

    fn calculate_transaction_contextual_masses(&self, transaction: &MutableTransaction) -> Option<ContextualMasses> {
        self.services.mass_calculator.calc_contextual_masses(&transaction.as_verifiable())
    }

    fn get_stats(&self) -> ConsensusStats {
        // This method is designed to return stats asap and not depend on locks which
        // might take time to acquire
        ConsensusStats {
            block_counts: self.estimate_block_count(),
            // This call acquires the tips store read lock which is expected to be fast. If this
            // turns out to be not fast enough then we should maintain an atomic integer holding this value
            num_tips: self.get_tips_len() as u64,
            virtual_stats: self.lkg_virtual_state.load().as_ref().into(),
        }
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.lkg_virtual_state.load().daa_score
    }

    fn get_virtual_bits(&self) -> u32 {
        self.lkg_virtual_state.load().bits
    }

    fn get_virtual_past_median_time(&self) -> u64 {
        self.lkg_virtual_state.load().past_median_time
    }

    fn get_virtual_merge_depth_root(&self) -> Option<Hash> {
        // TODO: consider saving the merge depth root as part of virtual state
        let pruning_point = self.pruning_point_store.read().pruning_point().unwrap();
        let virtual_state = self.lkg_virtual_state.load();
        let virtual_ghostdag_data = &virtual_state.ghostdag_data;
        let root = self.services.depth_manager.calc_merge_depth_root(virtual_ghostdag_data, pruning_point);
        if root.is_origin() {
            None
        } else {
            Some(root)
        }
    }

    fn get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        // PRUNE SAFETY: merge depth root is never close to being pruned (in terms of block depth)
        self.get_virtual_merge_depth_root().map_or(BlueWorkType::ZERO, |root| self.ghostdag_store.get_blue_work(root).unwrap())
    }

    fn get_sink(&self) -> Hash {
        self.lkg_virtual_state.load().ghostdag_data.selected_parent
    }

    fn get_sink_timestamp(&self) -> u64 {
        self.headers_store.get_timestamp(self.get_sink()).unwrap()
    }

    fn get_sink_blue_score(&self) -> u64 {
        self.headers_store.get_blue_score(self.get_sink()).unwrap()
    }

    fn get_sink_daa_score_timestamp(&self) -> DaaScoreTimestamp {
        let sink = self.get_sink();
        let compact = self.headers_store.get_compact_header_data(sink).unwrap();
        DaaScoreTimestamp { daa_score: compact.daa_score, timestamp: compact.timestamp }
    }

    fn get_current_block_color(&self, hash: Hash) -> Option<bool> {
        let _guard = self.pruning_lock.blocking_read();

        // Verify the block exists and can be assumed to have relations and reachability data
        self.validate_block_exists(hash).ok()?;

        // Verify that the block is in future(retention root), where Ghostdag data is complete
        self.services.reachability_service.is_dag_ancestor_of(self.get_retention_period_root(), hash).then_some(())?;

        let sink = self.get_sink();

        // Optimization: verify that the block is in past(sink), otherwise the search will fail anyway
        // (means the block was not merged yet by a virtual chain block)
        self.services.reachability_service.is_dag_ancestor_of(hash, sink).then_some(())?;

        let mut heap: BinaryHeap<Reverse<SortableBlock>> = BinaryHeap::new();
        let mut visited = BlockHashSet::new();

        let initial_children = self.get_block_children(hash).unwrap();

        for child in initial_children {
            if visited.insert(child) {
                let blue_work = self.ghostdag_store.get_blue_work(child).unwrap();
                heap.push(Reverse(SortableBlock::new(child, blue_work)));
            }
        }

        while let Some(Reverse(SortableBlock { hash: decedent, .. })) = heap.pop() {
            if self.services.reachability_service.is_chain_ancestor_of(decedent, sink) {
                let decedent_data = self.get_ghostdag_data(decedent).unwrap();

                if decedent_data.mergeset_blues.contains(&hash) {
                    return Some(true);
                } else if decedent_data.mergeset_reds.contains(&hash) {
                    return Some(false);
                }

                // Note: because we are doing a topological BFS up (from `hash` towards virtual), the first chain block
                // found must also be our merging block, so hash will be either in blues or in reds, rendering this line
                // unreachable.
                kaspa_core::warn!("DAG topology inconsistency: {decedent} is expected to be a merging block of {hash}");
                // TODO: we should consider the option of returning Result<Option<bool>> from this method
                return None;
            }

            let children = self.get_block_children(decedent).unwrap();

            for child in children {
                if visited.insert(child) {
                    let blue_work = self.ghostdag_store.get_blue_work(child).unwrap();
                    heap.push(Reverse(SortableBlock::new(child, blue_work)));
                }
            }
        }

        None
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        self.lkg_virtual_state.load().to_virtual_state_approx_id()
    }

    fn get_retention_period_root(&self) -> Hash {
        self.pruning_point_store.read().retention_period_root().unwrap()
    }

    /// Estimates the number of blocks and headers stored in the node database.
    ///
    /// This is an estimation based on the DAA score difference between the node's `retention root` and `virtual`'s DAA score,
    /// as such, it does not include non-daa blocks, and does not include headers stored as part of the pruning proof.  
    fn estimate_block_count(&self) -> BlockCount {
        // PRUNE SAFETY: retention root is always a current or past pruning point which its header is kept permanently
        let retention_period_root_score = self.headers_store.get_daa_score(self.get_retention_period_root()).unwrap();
        let virtual_score = self.get_virtual_daa_score();
        let header_count = self
            .headers_store
            .get_daa_score(self.get_headers_selected_tip())
            .unwrap_option()
            .unwrap_or(virtual_score)
            .max(virtual_score)
            - retention_period_root_score;
        let block_count = virtual_score - retention_period_root_score;
        BlockCount { header_count, block_count }
    }

    fn get_virtual_chain_from_block(&self, low: Hash, chain_path_added_limit: Option<usize>) -> ConsensusResult<ChainPath> {
        // Calculate chain changes between the given `low` and the current sink hash (up to `limit` amount of block hashes).
        // Note:
        // 1) that we explicitly don't
        // do the calculation against the virtual itself so that we
        // won't later need to remove it from the result.
        // 2) supplying `None` as `chain_path_added_limit` will result in the full chain path, with optimized performance.
        let _guard = self.pruning_lock.blocking_read();

        // Verify that the block exists
        self.validate_block_exists(low)?;

        // Verify that retention root is on chain(block)
        self.services
            .reachability_service
            .is_chain_ancestor_of(self.get_retention_period_root(), low)
            .then_some(())
            .ok_or(ConsensusError::General("the queried hash does not have retention root on its chain"))?;

        Ok(self.services.dag_traversal_manager.calculate_chain_path(low, self.get_sink(), chain_path_added_limit))
    }

    /// Returns a Vec of header samples since genesis
    /// ordered by ascending daa_score, first entry is genesis
    fn get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        // We need consistency between the past pruning points, selected chain and header store reads
        let _guard = self.pruning_lock.blocking_read();

        // Sorted from genesis to latest pruning_point_headers
        let pp_headers = self.pruning_point_compact_headers();
        let step_divisor: usize = 3; // The number of extra samples we'll get from blocks after last pp header
        let prealloc_len = pp_headers.len() + step_divisor + 1;

        let mut sample_headers;

        // Part 1: Add samples from pruning point headers:
        if self.config.net.network_type == NetworkType::Mainnet {
            // For mainnet, we add extra data (16 pp headers) from before checkpoint genesis.
            // Source: https://github.com/kaspagang/kaspad-py-explorer/blob/main/src/tx_timestamp_estimation.ipynb
            // For context see also: https://github.com/kaspagang/kaspad-py-explorer/blob/main/src/genesis_proof.ipynb
            const POINTS: &[DaaScoreTimestamp] = &[
                DaaScoreTimestamp { daa_score: 0, timestamp: 1636298787842 },
                DaaScoreTimestamp { daa_score: 87133, timestamp: 1636386662010 },
                DaaScoreTimestamp { daa_score: 176797, timestamp: 1636473700804 },
                DaaScoreTimestamp { daa_score: 264837, timestamp: 1636560706885 },
                DaaScoreTimestamp { daa_score: 355974, timestamp: 1636650005662 },
                DaaScoreTimestamp { daa_score: 445152, timestamp: 1636737841327 },
                DaaScoreTimestamp { daa_score: 536709, timestamp: 1636828600930 },
                DaaScoreTimestamp { daa_score: 624635, timestamp: 1636912614350 },
                DaaScoreTimestamp { daa_score: 712234, timestamp: 1636999362832 },
                DaaScoreTimestamp { daa_score: 801831, timestamp: 1637088292662 },
                DaaScoreTimestamp { daa_score: 890716, timestamp: 1637174890675 },
                DaaScoreTimestamp { daa_score: 978396, timestamp: 1637260956454 },
                DaaScoreTimestamp { daa_score: 1068387, timestamp: 1637349078269 },
                DaaScoreTimestamp { daa_score: 1139626, timestamp: 1637418723538 },
                DaaScoreTimestamp { daa_score: 1218320, timestamp: 1637495941516 },
                DaaScoreTimestamp { daa_score: 1312860, timestamp: 1637609671037 },
            ];
            sample_headers = Vec::<DaaScoreTimestamp>::with_capacity(prealloc_len + POINTS.len());
            sample_headers.extend_from_slice(POINTS);
        } else {
            sample_headers = Vec::<DaaScoreTimestamp>::with_capacity(prealloc_len);
        }

        for header in pp_headers.iter() {
            sample_headers.push(DaaScoreTimestamp { daa_score: header.1.daa_score, timestamp: header.1.timestamp });
        }

        // Part 2: Add samples from recent chain blocks
        let sc_read = self.storage.selected_chain_store.read();
        let high_index = sc_read.get_tip().unwrap().0;
        // The last pruning point is always expected in the selected chain store. However if due to some reason
        // this is not the case, we prefer not crashing but rather avoid sampling (hence set low index to high index)
        let low_index = sc_read.get_by_hash(pp_headers.last().unwrap().0).unwrap_option().unwrap_or(high_index);
        let step_size = cmp::max((high_index - low_index) / (step_divisor as u64), 1);

        // We chain `high_index` to make sure we sample sink, and dedup to avoid sampling it twice
        for index in (low_index + step_size..=high_index).step_by(step_size as usize).chain(once(high_index)).dedup() {
            let compact = self
                .storage
                .headers_store
                .get_compact_header_data(sc_read.get_by_index(index).expect("store lock is acquired"))
                .unwrap();
            sample_headers.push(DaaScoreTimestamp { daa_score: compact.daa_score, timestamp: compact.timestamp });
        }

        sample_headers
    }

    fn get_populated_transaction(&self, txid: Hash, accepting_block_daa_score: u64) -> Result<SignableTransaction, UtxoInquirerError> {
        // We need consistency between the pruning_point_store, utxo_diffs_store, block_transactions_store, selected chain and headers store reads
        let _guard = self.pruning_lock.blocking_read();
        self.virtual_processor.get_populated_transaction(txid, accepting_block_daa_score, self.get_retention_period_root())
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        self.lkg_virtual_state.load().parents.iter().copied().collect()
    }

    fn get_virtual_parents_len(&self) -> usize {
        self.lkg_virtual_state.load().parents.len()
    }

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        let virtual_stores = self.virtual_stores.read();
        let iter = virtual_stores.utxo_set.seek_iterator(from_outpoint, chunk_size, skip_first);
        iter.map(|item| item.unwrap()).collect()
    }

    fn get_tips(&self) -> Vec<Hash> {
        self.body_tips_store.read().get().unwrap().read().iter().copied().collect_vec()
    }

    fn get_tips_len(&self) -> usize {
        self.body_tips_store.read().get().unwrap().read().len()
    }

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: Hash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        if self.pruning_point_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }
        let pruning_utxoset_read = self.pruning_utxoset_stores.read();
        let iter = pruning_utxoset_read.utxo_set.seek_iterator(from_outpoint, chunk_size, skip_first);
        let utxos = iter.map(|item| item.unwrap()).collect();
        drop(pruning_utxoset_read);

        // We recheck the expected pruning point in case it was switched just before the utxo set read.
        // NOTE: we rely on order of operations by pruning processor. See extended comment therein.
        if self.pruning_point_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }

        Ok(utxos)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        self.services.coinbase_manager.modify_coinbase_payload(payload, miner_data)
    }

    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction], pov_daa_score: u64) -> Hash {
        let storage_mass_activated = self.config.crescendo_activation.is_active(pov_daa_score);
        calc_hash_merkle_root(txs.iter(), storage_mass_activated)
    }

    fn validate_pruning_proof(
        &self,
        proof: &PruningPointProof,
        proof_metadata: &PruningProofMetadata,
    ) -> Result<(), PruningImportError> {
        self.services.pruning_proof_manager.validate_pruning_point_proof(proof, proof_metadata)
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        self.services.pruning_proof_manager.apply_proof(proof, trusted_set)
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) -> PruningImportResult<()> {
        self.services.pruning_proof_manager.import_pruning_points(&pruning_points)
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        let mut pruning_utxoset_write = self.pruning_utxoset_stores.write();
        pruning_utxoset_write.utxo_set.write_many(utxoset_chunk).unwrap();

        // Parallelize processing using the context of an existing thread pool.
        let inner_multiset = self.virtual_processor.install(|| {
            utxoset_chunk.par_iter().map(|(outpoint, entry)| MuHash::from_utxo(outpoint, entry)).reduce(MuHash::new, |mut a, b| {
                a.combine(&b);
                a
            })
        });

        current_multiset.combine(&inner_multiset);
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: Hash, imported_utxo_multiset: MuHash) -> PruningImportResult<()> {
        self.virtual_processor.import_pruning_point_utxo_set(new_pruning_point, imported_utxo_multiset)
    }

    fn validate_pruning_points(&self, syncer_virtual_selected_parent: Hash) -> ConsensusResult<()> {
        let hst = self.storage.headers_selected_tip_store.read().get().unwrap().hash;
        let pp_info = self.pruning_point_store.read().get().unwrap();
        if !self.services.pruning_point_manager.is_valid_pruning_point(pp_info.pruning_point, hst) {
            return Err(ConsensusError::General("pruning point does not coincide with the synced header selected tip"));
        }
        if !self.services.pruning_point_manager.is_valid_pruning_point(pp_info.pruning_point, syncer_virtual_selected_parent) {
            return Err(ConsensusError::General("pruning point does not coincide with the syncer's sink (virtual selected parent)"));
        }
        self.services
            .pruning_point_manager
            .are_pruning_points_in_valid_chain(pp_info, syncer_virtual_selected_parent)
            .map_err(|e| ConsensusError::GeneralOwned(format!("past pruning points do not form a valid chain: {}", e)))
    }

    fn is_chain_ancestor_of(&self, low: Hash, high: Hash) -> ConsensusResult<bool> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;
        Ok(self.services.reachability_service.is_chain_ancestor_of(low, high))
    }

    // max_blocks has to be greater than the merge set size limit
    fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)> {
        let _guard = self.pruning_lock.blocking_read();
        assert!(max_blocks as u64 > self.config.mergeset_size_limit().upper_bound());
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;

        Ok(self.services.sync_manager.antipast_hashes_between(low, high, Some(max_blocks)))
    }

    fn get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>> {
        self.headers_store.get_header(hash).unwrap_option().ok_or(ConsensusError::HeaderNotFound(hash))
    }

    fn get_headers_selected_tip(&self) -> Hash {
        self.headers_selected_tip_store.read().get().unwrap().hash
    }

    fn get_antipast_from_pov(&self, hash: Hash, context: Hash, max_traversal_allowed: Option<u64>) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;
        self.validate_block_exists(context)?;
        Ok(self.services.dag_traversal_manager.antipast(hash, std::iter::once(context), max_traversal_allowed)?)
    }

    fn get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;
        let virtual_state = self.lkg_virtual_state.load();
        Ok(self.services.dag_traversal_manager.anticone(hash, virtual_state.parents.iter().copied(), None)?)
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        // PRUNE SAFETY: proof is cached before the prune op begins and the
        // pruning point cannot move during the prune so the cache remains valid
        self.services.pruning_proof_manager.get_pruning_point_proof()
    }

    fn create_virtual_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        if let Some(low) = low {
            self.validate_block_exists(low)?;
        }

        if let Some(high) = high {
            self.validate_block_exists(high)?;
        }

        Ok(self.services.sync_manager.create_virtual_selected_chain_block_locator(low, high)?)
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        // PRUNE SAFETY: index is monotonic and past pruning point headers are expected permanently
        let current_pp_info = self.pruning_point_store.read().get().unwrap();
        (0..current_pp_info.index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(current_pp_info.pruning_point))
            .map(|hash| self.headers_store.get_header(hash).unwrap())
            .collect_vec()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        // PRUNE SAFETY: anticone and trusted data are cached before the prune op begins and the
        // pruning point cannot move during the prune so the cache remains valid
        self.services.pruning_proof_manager.get_pruning_point_anticone_and_trusted_data()
    }

    fn get_block(&self, hash: Hash) -> ConsensusResult<Block> {
        if match self.statuses_store.read().get(hash).unwrap_option() {
            Some(status) => !status.has_block_body(),
            None => true,
        } {
            return Err(ConsensusError::BlockNotFound(hash));
        }

        Ok(Block {
            header: self.headers_store.get_header(hash).unwrap_option().ok_or(ConsensusError::BlockNotFound(hash))?,
            transactions: self.block_transactions_store.get(hash).unwrap_option().ok_or(ConsensusError::BlockNotFound(hash))?,
        })
    }

    fn get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        let Some(status) = self.statuses_store.read().get(hash).unwrap_option().filter(|&status| status.has_block_header()) else {
            return Err(ConsensusError::HeaderNotFound(hash));
        };
        Ok(Block {
            header: self.headers_store.get_header(hash).unwrap_option().ok_or(ConsensusError::HeaderNotFound(hash))?,
            transactions: if status.is_header_only() {
                Default::default()
            } else {
                self.block_transactions_store.get(hash).unwrap_option().unwrap_or_default()
            },
        })
    }

    fn get_ghostdag_data(&self, hash: Hash) -> ConsensusResult<ExternalGhostdagData> {
        match self.get_block_status(hash) {
            None => return Err(ConsensusError::HeaderNotFound(hash)),
            Some(BlockStatus::StatusInvalid) => return Err(ConsensusError::InvalidBlock(hash)),
            _ => {}
        };
        let ghostdag = self.ghostdag_store.get_data(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash))?;
        Ok((&*ghostdag).into())
    }

    fn get_block_children(&self, hash: Hash) -> Option<Vec<Hash>> {
        self.services
            .relations_service
            .get_children(hash)
            .unwrap_option()
            .map(|children| children.read().iter().copied().collect_vec())
    }

    fn get_block_parents(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.services.relations_service.get_parents(hash).unwrap_option()
    }

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        self.statuses_store.read().get(hash).unwrap_option()
    }

    fn get_block_acceptance_data(&self, hash: Hash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.acceptance_data_store.get(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash))
    }

    fn get_blocks_acceptance_data(
        &self,
        hashes: &[Hash],
        merged_blocks_limit: Option<usize>,
    ) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        // Note: merged_blocks_limit will limit after the sum of merged blocks is breached along the supplied hash's acceptance data
        // and not limit the acceptance data within a queried hash. i.e. It has mergeset_size_limit granularity, this is to guarantee full acceptance data coverage.
        if merged_blocks_limit.is_none() {
            return hashes
                .iter()
                .copied()
                .map(|hash| self.acceptance_data_store.get(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash)))
                .collect::<ConsensusResult<Vec<_>>>();
        }
        let merged_blocks_limit = merged_blocks_limit.unwrap(); // we handle `is_none`, so may unwrap.
        let mut num_of_merged_blocks = 0usize;

        hashes
            .iter()
            .copied()
            .map_while(|hash| {
                let entry = self.acceptance_data_store.get(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash));
                num_of_merged_blocks += entry.as_ref().map_or(0, |entry| entry.len());
                if num_of_merged_blocks > merged_blocks_limit {
                    None
                } else {
                    Some(entry)
                }
            })
            .collect::<ConsensusResult<Vec<_>>>()
    }

    fn is_chain_block(&self, hash: Hash) -> ConsensusResult<bool> {
        self.is_chain_ancestor_of(hash, self.get_sink())
    }

    fn get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(high)?;
        Ok(self.services.sync_manager.get_missing_block_body_hashes(high)?)
    }

    fn pruning_point(&self) -> Hash {
        self.pruning_point_store.read().pruning_point().unwrap()
    }

    fn get_daa_window(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;
        Ok(self
            .services
            .window_manager
            .block_window(&self.ghostdag_store.get_data(hash).unwrap(), WindowType::DifficultyWindow)
            .unwrap()
            .deref()
            .iter()
            .map(|block| block.0.hash)
            .collect())
    }

    fn get_trusted_block_associated_ghostdag_data_block_hashes(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(hash)?;

        // In order to guarantee the chain height is at least k, we check that the pruning point is not genesis.
        let pruning_point = self.pruning_point();
        if pruning_point == self.config.genesis.hash {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }

        // [Crescendo]: get ghostdag k based on the pruning point's DAA score. The off-by-one of not going by selected parent
        // DAA score is not important here as we simply increase K one block earlier which is more conservative (saving/sending more data)
        let ghostdag_k = self.config.ghostdag_k().get(self.headers_store.get_daa_score(pruning_point).unwrap());

        // Note: the method `get_ghostdag_chain_k_depth` might return a partial chain if data is missing.
        // Ideally this node when synced would validate it got all of the associated data up to k blocks
        // back and then we would be able to assert we actually got `k + 1` blocks, however we choose to
        // simply ignore, since if the data was truly missing we wouldn't accept the staging consensus in
        // the first place
        Ok(self.services.pruning_proof_manager.get_ghostdag_chain_k_depth(hash, ghostdag_k))
    }

    fn create_block_locator_from_pruning_point(&self, high: Hash, limit: usize) -> ConsensusResult<Vec<Hash>> {
        let _guard = self.pruning_lock.blocking_read();
        self.validate_block_exists(high)?;
        // Keep the pruning point read guard throughout building the locator
        let pruning_point_read = self.pruning_point_store.read();
        let pruning_point = pruning_point_read.pruning_point().unwrap();
        Ok(self.services.sync_manager.create_block_locator_from_pruning_point(high, pruning_point, Some(limit))?)
    }

    fn estimate_network_hashes_per_second(&self, start_hash: Option<Hash>, window_size: usize) -> ConsensusResult<u64> {
        let _guard = self.pruning_lock.blocking_read();
        match start_hash {
            Some(hash) => {
                self.validate_block_exists(hash)?;
                let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();
                // The selected parent header is used within to check for sampling activation, so we verify its existence first
                if !self.headers_store.has(ghostdag_data.selected_parent).unwrap() {
                    return Err(ConsensusError::DifficultyError(DifficultyError::InsufficientWindowData(0)));
                }
                self.estimate_network_hashes_per_second_impl(&ghostdag_data, window_size)
            }
            None => {
                let virtual_state = self.lkg_virtual_state.load();
                self.estimate_network_hashes_per_second_impl(&virtual_state.ghostdag_data, window_size)
            }
        }
    }

    fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        self.virtual_processor.are_pruning_points_violating_finality(pp_list)
    }

    fn creation_timestamp(&self) -> u64 {
        self.creation_timestamp
    }

    fn finality_point(&self) -> Hash {
        self.virtual_processor.virtual_finality_point(&self.lkg_virtual_state.load().ghostdag_data, self.pruning_point())
    }
}
