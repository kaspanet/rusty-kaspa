pub mod ctl;
pub mod factory;
pub mod test_consensus;

use crate::{
    config::Config,
    constants::store_names,
    errors::{BlockProcessResult, RuleError},
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
            statuses::MTStatusesService,
        },
        stores::{
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::BlockWindowCacheStore,
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            headers_selected_tip::{DbHeadersSelectedTipStore, HeadersSelectedTipStoreReader},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStoreReader},
            pruning::{DbPruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            selected_chain::DbSelectedChainStore,
            statuses::{DbStatusesStore, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::DbUtxoDiffsStore,
            utxo_multisets::DbUtxoMultisetsStore,
            utxo_set::{DbUtxoSetStore, UtxoSetStore, UtxoSetStoreReader},
            virtual_state::{DbVirtualStateStore, VirtualStateStoreReader},
            DB,
        },
    },
    pipeline::{
        body_processor::BlockBodyProcessor,
        deps_manager::{BlockProcessingMessage, BlockResultSender, BlockTask},
        header_processor::HeaderProcessor,
        pruning_processor::processor::{PruningProcessingMessage, PruningProcessor},
        virtual_processor::{errors::PruningImportResult, VirtualStateProcessor},
        ProcessingCounters,
    },
    processes::{
        block_depth::BlockDepthManager, coinbase::CoinbaseManager, difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager,
        mass::MassCalculator, parents_builder::ParentsManager, past_median_time::PastMedianTimeManager, pruning::PruningManager,
        pruning_proof::PruningProofManager, reachability::inquirer as reachability, sync::SyncManager,
        transaction_validator::TransactionValidator, traversal_manager::DagTraversalManager,
    },
};
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    api::ConsensusApi,
    block::{Block, BlockInfo, BlockRelations, BlockTemplate},
    blockhash::BlockHashExtensions,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    errors::pruning::PruningImportError,
    errors::{
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        tx::TxResult,
    },
    header::Header,
    muhash::MuHashExtensions,
    pruning::{PruningPointProof, PruningPointsList},
    sync_info::SyncInfo,
    trusted::TrustedBlock,
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet, ChainPath,
};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_utils::option::OptionExtensions;

use crossbeam_channel::{unbounded as unbounded_crossbeam, Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use futures_util::future::BoxFuture;
use itertools::Itertools;
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use parking_lot::RwLock;
use std::{
    cmp::max,
    future::Future,
    iter::once,
    sync::{atomic::Ordering, Arc},
};
use std::{
    ops::DerefMut,
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

pub type DbGhostdagManager =
    GhostdagManager<DbGhostdagStore, MTRelationsService<DbRelationsStore>, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>;

/// Used in order to group virtual related stores under a single lock
pub struct VirtualStores {
    pub state: DbVirtualStateStore,
    pub utxo_set: DbUtxoSetStore,
}

impl VirtualStores {
    pub fn new(state: DbVirtualStateStore, utxo_set: DbUtxoSetStore) -> Self {
        Self { state, utxo_set }
    }
}

pub struct Consensus {
    // DB
    db: Arc<DB>,

    // Channels
    block_sender: CrossbeamSender<BlockProcessingMessage>,

    // Processors
    pub header_processor: Arc<HeaderProcessor>,
    pub(super) body_processor: Arc<BlockBodyProcessor>,
    pub virtual_processor: Arc<VirtualStateProcessor>,
    pub pruning_processor: Arc<PruningProcessor>,

    // Stores
    statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub headers_store: Arc<DbHeadersStore>,
    pub block_transactions_store: Arc<DbBlockTransactionsStore>,
    pruning_point_utxo_set_store: Arc<RwLock<DbUtxoSetStore>>,
    pub(super) virtual_stores: Arc<RwLock<VirtualStores>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    // TODO: remove all pub from stores and processors when StoreManager is implemented

    // Append-only stores
    pub ghostdag_store: Arc<DbGhostdagStore>,

    // Services and managers
    statuses_service: MTStatusesService<DbStatusesStore>,
    relations_service: MTRelationsService<DbRelationsStore>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) dag_traversal_manager:
        DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) past_median_time_manager: PastMedianTimeManager<
        DbHeadersStore,
        DbGhostdagStore,
        BlockWindowCacheStore,
        DbReachabilityStore,
        MTRelationsService<DbRelationsStore>,
    >,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    pub(super) pruning_proof_manager: PruningProofManager,
    sync_manager: SyncManager<
        DbReachabilityStore,
        DbGhostdagStore,
        DbSelectedChainStore,
        DbHeadersSelectedTipStore,
        DbPruningStore,
        DbStatusesStore,
    >,
    depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,

    // Notification management
    notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    pub counters: Arc<ProcessingCounters>,

    // Config
    config: Arc<Config>,
}

impl Consensus {
    pub fn new(
        db: Arc<DB>,
        config: Arc<Config>,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        let params = &config.params;
        let perf_params = &config.perf;
        //
        // Stores
        //

        let pruning_size_for_caches = params.pruning_depth;
        let pruning_plus_finality_size_for_caches = params.pruning_depth + params.finality_depth;

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), pruning_plus_finality_size_for_caches)));
        let relations_stores = Arc::new(RwLock::new(
            (0..=params.max_block_level)
                .map(|level| {
                    let cache_size =
                        max(pruning_plus_finality_size_for_caches.checked_shr(level as u32).unwrap_or(0), 2 * params.pruning_proof_m);
                    DbRelationsStore::new(db.clone(), level, cache_size)
                })
                .collect_vec(),
        ));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), pruning_plus_finality_size_for_caches)));
        let ghostdag_stores = (0..=params.max_block_level)
            .map(|level| {
                let cache_size =
                    max(pruning_plus_finality_size_for_caches.checked_shr(level as u32).unwrap_or(0), 2 * params.pruning_proof_m);
                Arc::new(DbGhostdagStore::new(db.clone(), level, cache_size))
            })
            .collect_vec();
        let ghostdag_store = ghostdag_stores[0].clone();
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), pruning_size_for_caches));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), perf_params.header_data_cache_size));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), perf_params.header_data_cache_size));
        let selected_chain_store = Arc::new(RwLock::new(DbSelectedChainStore::new(db.clone(), perf_params.header_data_cache_size)));
        // Pruning
        let pruning_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let past_pruning_points_store = Arc::new(DbPastPruningPointsStore::new(db.clone(), 4));
        let pruning_point_utxo_set_store =
            Arc::new(RwLock::new(DbUtxoSetStore::new(db.clone(), perf_params.utxo_set_cache_size, store_names::PRUNING_UTXO_SET)));

        // Block data

        let block_transactions_store = Arc::new(DbBlockTransactionsStore::new(db.clone(), perf_params.block_data_cache_size));
        let utxo_diffs_store = Arc::new(DbUtxoDiffsStore::new(db.clone(), perf_params.block_data_cache_size));
        let utxo_multisets_store = Arc::new(DbUtxoMultisetsStore::new(db.clone(), perf_params.block_data_cache_size));
        let acceptance_data_store = Arc::new(DbAcceptanceDataStore::new(db.clone(), perf_params.block_data_cache_size));
        // Tips
        let headers_selected_tip_store = Arc::new(RwLock::new(DbHeadersSelectedTipStore::new(db.clone())));
        let body_tips_store = Arc::new(RwLock::new(DbTipsStore::new(db.clone())));
        // Block windows
        let block_window_cache_for_difficulty = Arc::new(BlockWindowCacheStore::new(perf_params.block_window_cache_size));
        let block_window_cache_for_past_median_time = Arc::new(BlockWindowCacheStore::new(perf_params.block_window_cache_size));
        // Virtual stores
        let virtual_stores = Arc::new(RwLock::new(VirtualStores::new(
            DbVirtualStateStore::new(db.clone()),
            DbUtxoSetStore::new(db.clone(), perf_params.utxo_set_cache_size, store_names::VIRTUAL_UTXO_SET),
        )));

        //
        // Services and managers
        //

        let statuses_service = MTStatusesService::new(statuses_store.clone());
        let relations_services =
            (0..=params.max_block_level).map(|level| MTRelationsService::new(relations_stores.clone(), level)).collect_vec();
        let relations_service = relations_services[0].clone();
        let reachability_service = MTReachabilityService::new(reachability_store.clone());
        let dag_traversal_manager = DagTraversalManager::new(
            params.genesis.hash,
            ghostdag_store.clone(),
            relations_service.clone(),
            block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time.clone(),
            params.difficulty_window_size,
            (2 * params.timestamp_deviation_tolerance - 1) as usize, // TODO: incorporate target_time_per_block to this calculation
            reachability_service.clone(),
        );
        let past_median_time_manager = PastMedianTimeManager::new(
            headers_store.clone(),
            dag_traversal_manager.clone(),
            params.timestamp_deviation_tolerance as usize,
            params.genesis.timestamp,
        );
        let difficulty_manager = DifficultyManager::new(
            headers_store.clone(),
            params.genesis.bits,
            params.difficulty_window_size,
            params.target_time_per_block,
        );
        let depth_manager = BlockDepthManager::new(
            params.merge_depth,
            params.finality_depth,
            params.genesis.hash,
            depth_store.clone(),
            reachability_service.clone(),
            ghostdag_store.clone(),
        );
        let ghostdag_managers = ghostdag_stores
            .iter()
            .cloned()
            .enumerate()
            .map(|(level, ghostdag_store)| {
                GhostdagManager::new(
                    params.genesis.hash,
                    params.ghostdag_k,
                    ghostdag_store,
                    relations_services[level].clone(),
                    headers_store.clone(),
                    reachability_service.clone(),
                )
            })
            .collect_vec();
        let ghostdag_manager = ghostdag_managers[0].clone();

        let coinbase_manager = CoinbaseManager::new(
            params.coinbase_payload_script_public_key_max_len,
            params.max_coinbase_payload_len,
            params.deflationary_phase_daa_score,
            params.pre_deflationary_phase_base_subsidy,
        );

        let mass_calculator =
            MassCalculator::new(params.mass_per_tx_byte, params.mass_per_script_pub_key_byte, params.mass_per_sig_op);

        let transaction_validator = TransactionValidator::new(
            params.max_tx_inputs,
            params.max_tx_outputs,
            params.max_signature_script_len,
            params.max_script_public_key_len,
            params.ghostdag_k,
            params.coinbase_payload_script_public_key_max_len,
            params.coinbase_maturity,
        );

        let pruning_manager = PruningManager::new(
            params.pruning_depth,
            params.finality_depth,
            params.genesis.hash,
            reachability_service.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            past_pruning_points_store.clone(),
        );

        let parents_manager = ParentsManager::new(
            params.max_block_level,
            params.genesis.hash,
            headers_store.clone(),
            reachability_service.clone(),
            relations_service.clone(),
        );

        let (sender, receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (body_sender, body_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (virtual_sender, virtual_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (pruning_sender, pruning_receiver): (
            CrossbeamSender<PruningProcessingMessage>,
            CrossbeamReceiver<PruningProcessingMessage>,
        ) = unbounded_crossbeam();

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
            relations_stores.clone(),
            reachability_store.clone(),
            ghostdag_stores.clone(),
            headers_store.clone(),
            daa_excluded_store.clone(),
            statuses_store.clone(),
            pruning_store.clone(),
            depth_store.clone(),
            headers_selected_tip_store.clone(),
            selected_chain_store.clone(),
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            reachability_service.clone(),
            past_median_time_manager.clone(),
            dag_traversal_manager.clone(),
            difficulty_manager.clone(),
            depth_manager.clone(),
            pruning_manager.clone(),
            parents_manager.clone(),
            ghostdag_managers.clone(),
            counters.clone(),
        ));

        let body_processor = Arc::new(BlockBodyProcessor::new(
            body_receiver,
            virtual_sender,
            block_processors_pool,
            db.clone(),
            statuses_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            block_transactions_store.clone(),
            body_tips_store.clone(),
            reachability_service.clone(),
            coinbase_manager.clone(),
            mass_calculator,
            transaction_validator.clone(),
            past_median_time_manager.clone(),
            params.max_block_mass,
            params.genesis.clone(),
            notification_root.clone(),
            counters.clone(),
        ));

        let virtual_processor = Arc::new(VirtualStateProcessor::new(
            virtual_receiver,
            pruning_sender,
            virtual_pool,
            params,
            db.clone(),
            statuses_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_excluded_store,
            block_transactions_store.clone(),
            pruning_store.clone(),
            past_pruning_points_store.clone(),
            body_tips_store.clone(),
            utxo_diffs_store.clone(),
            utxo_multisets_store,
            acceptance_data_store,
            virtual_stores.clone(),
            pruning_point_utxo_set_store.clone(),
            ghostdag_manager.clone(),
            reachability_service.clone(),
            relations_service.clone(),
            dag_traversal_manager.clone(),
            difficulty_manager.clone(),
            coinbase_manager.clone(),
            transaction_validator,
            past_median_time_manager.clone(),
            pruning_manager.clone(),
            parents_manager.clone(),
            depth_manager.clone(),
            notification_root.clone(),
            counters.clone(),
        ));

        let pruning_processor = Arc::new(PruningProcessor::new(
            pruning_receiver,
            db.clone(),
            pruning_store.clone(),
            past_pruning_points_store.clone(),
            pruning_point_utxo_set_store.clone(),
            utxo_diffs_store,
            headers_store.clone(),
            pruning_manager.clone(),
            reachability_service.clone(),
        ));

        let pruning_proof_manager = PruningProofManager::new(
            db.clone(),
            headers_store.clone(),
            reachability_store.clone(),
            parents_manager,
            reachability_service.clone(),
            ghostdag_stores,
            relations_stores.clone(),
            pruning_store.clone(),
            past_pruning_points_store.clone(),
            virtual_stores.clone(),
            body_tips_store.clone(),
            headers_selected_tip_store.clone(),
            depth_store,
            selected_chain_store.clone(),
            ghostdag_managers,
            dag_traversal_manager.clone(),
            params.max_block_level,
            params.genesis.hash,
            params.pruning_proof_m,
            params.difficulty_window_size,
            params.ghostdag_k,
        );

        let sync_manager = SyncManager::new(
            params.mergeset_size_limit as usize,
            reachability_service.clone(),
            ghostdag_store.clone(),
            selected_chain_store,
            headers_selected_tip_store.clone(),
            pruning_store.clone(),
            statuses_store.clone(),
        );

        // Ensure that reachability store is initialized
        reachability::init(reachability_store.write().deref_mut()).unwrap();

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

        Self {
            db,
            block_sender: sender,
            header_processor,
            body_processor,
            virtual_processor,
            pruning_processor,
            statuses_store,
            relations_stores,
            reachability_store,
            ghostdag_store,
            pruning_store,
            headers_selected_tip_store,
            body_tips_store,
            headers_store,
            block_transactions_store,
            pruning_point_utxo_set_store,
            virtual_stores,
            past_pruning_points_store,

            statuses_service,
            relations_service,
            reachability_service,
            difficulty_manager,
            dag_traversal_manager,
            ghostdag_manager,
            past_median_time_manager,
            coinbase_manager,
            pruning_manager,
            pruning_proof_manager,
            sync_manager,
            depth_manager,
            notification_root,
            counters,
            config,
        }
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

    fn validate_and_insert_block_impl(
        &self,
        block: Block,
        update_virtual: bool,
    ) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        let (tx, rx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender
            .send(BlockProcessingMessage::Process(BlockTask { block, trusted_ghostdag_data: None, update_virtual }, tx))
            .unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::Relaxed);
        async { rx.await.unwrap() }
    }

    fn validate_and_insert_trusted_block_impl(&self, tb: TrustedBlock) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        let (tx, rx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender
            .send(BlockProcessingMessage::Process(
                BlockTask { block: tb.block, trusted_ghostdag_data: Some(Arc::new(tb.ghostdag.into())), update_virtual: false },
                tx,
            ))
            .unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::Relaxed);
        async { rx.await.unwrap() }
    }

    pub fn resolve_virtual(&self) {
        self.virtual_processor.resolve_virtual()
    }

    pub fn body_tips(&self) -> Arc<BlockHashSet> {
        self.body_tips_store.read().get().unwrap()
    }

    pub fn block_status(&self, hash: Hash) -> BlockStatus {
        self.statuses_store.read().get(hash).unwrap()
    }

    pub fn notification_root(&self) -> Arc<ConsensusNotificationRoot> {
        self.notification_root.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.counters
    }

    pub fn signal_exit(&self) {
        self.block_sender.send(BlockProcessingMessage::Exit).unwrap();
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.signal_exit();
        // Wait for async consensus processors to exit
        for handle in wait_handles {
            handle.join().unwrap();
        }
    }

    fn validate_block_exists(&self, hash: Hash) -> Result<(), ConsensusError> {
        if match self.statuses_store.read().get(hash).unwrap_option() {
            Some(status) => status.is_valid(),
            None => false,
        } {
            Ok(())
        } else {
            Err(ConsensusError::BlockNotFound(hash))
        }
    }
}

impl ConsensusApi for Consensus {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template(miner_data, txs)
    }

    fn validate_and_insert_block(&self, block: Block, update_virtual: bool) -> BoxFuture<'static, BlockProcessResult<BlockStatus>> {
        let result = self.validate_and_insert_block_impl(block, update_virtual);
        Box::pin(async move { result.await })
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BoxFuture<'static, BlockProcessResult<BlockStatus>> {
        let result = self.validate_and_insert_trusted_block_impl(tb);
        Box::pin(async move { result.await })
    }

    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        self.virtual_processor.validate_mempool_transaction_and_populate(transaction)?;
        Ok(())
    }

    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64 {
        self.body_processor.mass_calculator.calc_tx_mass(transaction)
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.virtual_processor.virtual_stores.read().state.get().unwrap().daa_score
    }

    fn get_virtual_merge_depth_root(&self) -> Option<Hash> {
        // TODO: consider saving the merge depth root as part of virtual state
        // TODO: unwrap on pruning_point and virtual state reads when staging consensus is implemented
        let Some(pruning_point) = self.pruning_store.read().pruning_point().unwrap_option() else { return None; };
        let Some(virtual_state) = self.virtual_processor.virtual_stores.read().state.get().unwrap_option() else { return None; };
        let virtual_ghostdag_data = &virtual_state.ghostdag_data;
        let root = self.depth_manager.calc_merge_depth_root(virtual_ghostdag_data, pruning_point);
        if root.is_origin() {
            None
        } else {
            Some(root)
        }
    }

    fn get_sink(&self) -> Option<Hash> {
        // TODO: unwrap on virtual state read when staging consensus is implemented
        self.virtual_processor.virtual_stores.read().state.get().unwrap_option().map(|state| state.ghostdag_data.selected_parent)
    }

    fn get_sink_timestamp(&self) -> Option<u64> {
        // TODO: unwrap on virtual state read when staging consensus is implemented
        self.virtual_processor.virtual_stores.read().state.get().unwrap_option().map(|state| {
            let sink = state.ghostdag_data.selected_parent;
            self.headers_store.get_timestamp(sink).unwrap()
        })
    }

    fn get_sync_info(&self) -> SyncInfo {
        SyncInfo::default()
    }

    fn is_nearly_synced(&self) -> bool {
        // See comment within `config.is_nearly_synced`
        self.get_sink_timestamp().has_value_and(|&t| self.config.is_nearly_synced(t))
    }

    fn get_virtual_chain_from_block(&self, hash: Hash) -> Option<ChainPath> {
        // Calculate chain changes between the given hash and the
        // sink. Note that we explicitly don't
        // do the calculation against the virtual itself so that we
        // won't later need to remove it from the result.
        self.get_sink().map(|sink_hash| self.dag_traversal_manager.calculate_chain_path(hash, sink_hash))
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        // TODO: unwrap on virtual state read when staging consensus is implemented
        match self.virtual_processor.virtual_stores.read().state.get().unwrap_option() {
            Some(s) => s.parents.iter().copied().collect(),
            None => Default::default(),
        }
    }

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        let virtual_stores = self.virtual_processor.virtual_stores.read();
        let iter = virtual_stores.utxo_set.seek_iterator(from_outpoint, chunk_size, skip_first);
        iter.map(|item| item.unwrap()).collect()
    }

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: Hash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        if self.pruning_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }
        let pruning_point_utxoset_read = self.pruning_point_utxo_set_store.read();
        let iter = pruning_point_utxoset_read.seek_iterator(from_outpoint, chunk_size, skip_first);
        let utxos = iter.map(|item| item.unwrap()).collect();
        drop(pruning_point_utxoset_read);

        // We recheck the expected pruning point in case it was switched just before the utxo set read.
        // NOTE: we rely on order of operations by pruning processor. See extended comment therein.
        if self.pruning_store.read().pruning_point().unwrap() != expected_pruning_point {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }

        Ok(utxos)
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        self.coinbase_manager.modify_coinbase_payload(payload, miner_data)
    }

    fn validate_pruning_proof(&self, proof: &PruningPointProof) -> Result<(), PruningImportError> {
        self.pruning_proof_manager.validate_pruning_point_proof(proof)
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) {
        self.pruning_proof_manager.apply_proof(proof, trusted_set)
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) {
        self.pruning_proof_manager.import_pruning_points(&pruning_points)
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        // TODO: Check if a db tx is needed. We probably need some kind of a flag that is set on this function to true, and then
        // is set to false on the end of import_pruning_point_utxo_set. On any failure on any of those functions (and also if the
        // node starts when the flag is true) the related data will be deleted and the flag will be set to false.
        let mut pruning_point_utxo_set = self.pruning_point_utxo_set_store.write();
        pruning_point_utxo_set.write_many(utxoset_chunk).unwrap();
        for (outpoint, entry) in utxoset_chunk {
            current_multiset.add_utxo(outpoint, entry);
        }
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: Hash, imported_utxo_multiset: &mut MuHash) -> PruningImportResult<()> {
        self.virtual_processor.import_pruning_point_utxo_set(new_pruning_point, imported_utxo_multiset)
    }

    fn header_exists(&self, hash: Hash) -> bool {
        match self.statuses_store.read().get(hash).unwrap_option() {
            Some(status) => status.has_block_header(),
            None => false,
        }
    }

    fn is_chain_ancestor_of(&self, low: Hash, high: Hash) -> ConsensusResult<bool> {
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;
        Ok(self.reachability_service.is_chain_ancestor_of(low, high))
    }

    // max_blocks has to be greater than the merge set size limit
    fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)> {
        assert!(max_blocks as u64 > self.config.mergeset_size_limit);
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;

        Ok(self.sync_manager.antipast_hashes_between(low, high, Some(max_blocks)))
    }

    fn get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>> {
        self.validate_block_exists(hash)?;
        Ok(self.headers_store.get_header(hash).unwrap())
    }

    fn get_headers_selected_tip(&self) -> Hash {
        self.headers_selected_tip_store.read().get().unwrap().hash
    }

    fn get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(hash)?;
        Ok(self.dag_traversal_manager.anticone(hash, self.virtual_stores.read().state.get().unwrap().parents.iter().copied(), None)?)
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        self.pruning_proof_manager.get_pruning_point_proof()
    }

    fn create_headers_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>> {
        if let Some(low) = low {
            self.validate_block_exists(low)?;
        }

        if let Some(high) = high {
            self.validate_block_exists(high)?;
        }

        Ok(self.sync_manager.create_headers_selected_chain_block_locator(low, high)?)
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        let current_pp_info = self.pruning_store.read().get().unwrap();
        (0..current_pp_info.index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(current_pp_info.pruning_point))
            .map(|hash| self.headers_store.get_header(hash).unwrap())
            .collect_vec()
    }

    fn get_pruning_point_anticone_and_trusted_data(
        &self,
    ) -> Arc<(Vec<Hash>, Vec<kaspa_consensus_core::trusted::TrustedHeader>, Vec<kaspa_consensus_core::trusted::TrustedGhostdagData>)>
    {
        self.pruning_proof_manager.get_pruning_point_anticone_and_trusted_data()
    }

    fn get_block(&self, hash: Hash) -> ConsensusResult<Block> {
        if match self.statuses_store.read().get(hash).unwrap_option() {
            Some(status) => !status.has_block_body(),
            None => true,
        } {
            return Err(ConsensusError::BlockNotFound(hash));
        }

        Ok(Block {
            header: self.headers_store.get_header(hash).unwrap(),
            transactions: self.block_transactions_store.get(hash).unwrap(),
        })
    }

    fn get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        let Some(status) = self.statuses_store.read().get(hash).unwrap_option().filter(|&status| !status.has_block_header()) else {
            return Err(ConsensusError::BlockNotFound(hash));
        };
        Ok(Block {
            header: self.headers_store.get_header(hash).unwrap(),
            transactions: if status.is_header_only() { Arc::new(vec![]) } else { self.block_transactions_store.get(hash).unwrap() },
        })
    }

    fn get_block_info(&self, hash: Hash) -> ConsensusResult<BlockInfo> {
        let (exists, block_status) = match self.get_block_status(hash) {
            Some(status) => (true, status),
            None => (false, BlockStatus::StatusInvalid),
        };

        // If the status is invalid, then we don't have the necessary reachability data to check if it's in PruningPoint.Future.
        if block_status == BlockStatus::StatusInvalid {
            return Ok(BlockInfo::with_exists(exists));
        }

        // FIXME: check this call
        let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();

        Ok(BlockInfo::new(
            exists,
            block_status,
            ghostdag_data.blue_score,
            ghostdag_data.blue_work,
            ghostdag_data.selected_parent,
            (*ghostdag_data.mergeset_blues).clone(),
            (*ghostdag_data.mergeset_reds).clone(),
        ))
    }

    fn get_block_relations(&self, hash: Hash) -> Option<BlockRelations> {
        let store = &self.relations_stores.read()[0];
        let parents = store.get_parents(hash).unwrap_option();
        let children = store.get_children(hash).unwrap_option();
        if parents.is_some() || children.is_some() {
            Some(BlockRelations::new(parents.unwrap_or_default(), children.unwrap_or_default()))
        } else {
            None
        }
    }

    fn get_block_children(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.relations_stores.read()[0].get_children(hash).unwrap_option()
    }

    fn get_block_parents(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.relations_stores.read()[0].get_parents(hash).unwrap_option()
    }

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        self.statuses_store.read().get(hash).unwrap_option()
    }

    fn get_block_acceptance_data(&self, hash: Hash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.validate_block_exists(hash)?;
        Ok(self.virtual_processor.acceptance_data_store().get(hash).unwrap())
    }

    fn get_blocks_acceptance_data(&self, hashes: &[Hash]) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        let acceptance_data_store = self.virtual_processor.acceptance_data_store();
        hashes
            .iter()
            .copied()
            .map(|hash| {
                self.validate_block_exists(hash)?;
                Ok(acceptance_data_store.get(hash).unwrap())
            })
            .collect::<ConsensusResult<Vec<_>>>()
    }

    fn is_chain_block(&self, hash: Hash) -> ConsensusResult<bool> {
        // FIXME: check this call
        let ghostdag_data = self.ghostdag_store.get_data(hash).unwrap();
        self.is_chain_ancestor_of(hash, ghostdag_data.selected_parent)
    }

    fn get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(high)?;
        Ok(self.sync_manager.get_missing_block_body_hashes(high)?)
    }

    fn pruning_point(&self) -> Option<Hash> {
        self.pruning_store.read().pruning_point().unwrap_option()
    }
}
