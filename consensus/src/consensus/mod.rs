pub mod test_consensus;

use crate::{
    constants::{
        perf::{PerfParams, PERF_PARAMS},
        store_names,
    },
    errors::BlockProcessResult,
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService, statuses::MTStatusesService},
        stores::{
            acceptance_data::DbAcceptanceDataStore,
            block_transactions::DbBlockTransactionsStore,
            block_window_cache::BlockWindowCacheStore,
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::DbGhostdagStore,
            headers::DbHeadersStore,
            headers_selected_tip::DbHeadersSelectedTipStore,
            past_pruning_points::DbPastPruningPointsStore,
            pruning::DbPruningStore,
            reachability::DbReachabilityStore,
            relations::DbRelationsStore,
            statuses::{DbStatusesStore, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::DbUtxoDiffsStore,
            utxo_multisets::DbUtxoMultisetsStore,
            utxo_set::DbUtxoSetStore,
            virtual_state::DbVirtualStateStore,
            DB,
        },
    },
    params::Params,
    pipeline::{
        body_processor::BlockBodyProcessor,
        deps_manager::{BlockResultSender, BlockTask},
        header_processor::HeaderProcessor,
        virtual_processor::VirtualStateProcessor,
        ProcessingCounters,
    },
    processes::{
        block_depth::BlockDepthManager, coinbase::CoinbaseManager, difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager,
        mass::MassCalculator, parents_builder::ParentsManager, past_median_time::PastMedianTimeManager, pruning::PruningManager,
        reachability::inquirer as reachability, transaction_validator::TransactionValidator, traversal_manager::DagTraversalManager,
    },
};
use consensus_core::{
    api::ConsensusApi,
    block::{Block, BlockTemplate},
    blockstatus::BlockStatus,
    coinbase::MinerData,
    tx::Transaction,
    BlockHashSet,
};
use crossbeam_channel::{unbounded, Receiver, Sender};
use futures_util::future::BoxFuture;
use hashes::Hash;
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::{future::Future, sync::atomic::Ordering};
use std::{
    ops::DerefMut,
    sync::Arc,
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

pub type DbGhostdagManager =
    GhostdagManager<DbGhostdagStore, MTRelationsService<DbRelationsStore>, MTReachabilityService<DbReachabilityStore>, DbHeadersStore>;

pub struct Consensus {
    // DB
    db: Arc<DB>,

    // Channels
    block_sender: Sender<BlockTask>,

    // Processors
    header_processor: Arc<HeaderProcessor>,
    pub(super) body_processor: Arc<BlockBodyProcessor>,
    pub virtual_processor: Arc<VirtualStateProcessor>,

    // Stores
    statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,
    headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    body_tips_store: Arc<RwLock<DbTipsStore>>,
    pub headers_store: Arc<DbHeadersStore>,
    pub block_transactions_store: Arc<DbBlockTransactionsStore>,
    // TODO: remove all pub from stores and processors when StoreManager is implemented

    // Append-only stores
    pub ghostdag_store: Arc<DbGhostdagStore>,

    // Services and managers
    statuses_service: MTStatusesService<DbStatusesStore>,
    relations_service: MTRelationsService<DbRelationsStore>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,

    // Counters
    pub counters: Arc<ProcessingCounters>,
}

impl Consensus {
    pub fn new(db: Arc<DB>, params: &Params) -> Self {
        Self::with_perf_params(db, params, &PERF_PARAMS)
    }

    pub fn with_perf_params(db: Arc<DB>, params: &Params, perf_params: &PerfParams) -> Self {
        //
        // Stores
        //

        let pruning_size_for_caches = params.pruning_depth;
        let pruning_plus_finality_size_for_caches = params.pruning_depth + params.finality_depth;

        // Headers
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), pruning_plus_finality_size_for_caches)));
        let relations_store = Arc::new(RwLock::new(DbRelationsStore::new(db.clone(), pruning_plus_finality_size_for_caches)));
        let reachability_store =
            Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), pruning_plus_finality_size_for_caches * 2)));
        let ghostdag_store = Arc::new(DbGhostdagStore::new(db.clone(), pruning_plus_finality_size_for_caches));
        let daa_excluded_store = Arc::new(DbDaaStore::new(db.clone(), pruning_size_for_caches));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), perf_params.header_data_cache_size));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), perf_params.header_data_cache_size));
        // Pruning
        let pruning_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let past_pruning_points_store = Arc::new(DbPastPruningPointsStore::new(db.clone(), 4));
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
        // Virtual (TODO: decide about locking semantics of virtual utxo set)
        let virtual_utxo_store =
            Arc::new(DbUtxoSetStore::new(db.clone(), perf_params.utxo_set_cache_size, store_names::VIRTUAL_UTXO_SET));
        let virtual_state_store = Arc::new(RwLock::new(DbVirtualStateStore::new(db.clone())));

        //
        // Services and managers
        //

        let statuses_service = MTStatusesService::new(statuses_store.clone());
        let relations_service = MTRelationsService::new(relations_store.clone());
        let reachability_service = MTReachabilityService::new(reachability_store.clone());
        let dag_traversal_manager = DagTraversalManager::new(
            params.genesis_hash,
            ghostdag_store.clone(),
            block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time.clone(),
            params.difficulty_window_size,
            (2 * params.timestamp_deviation_tolerance - 1) as usize, // TODO: incorporate target_time_per_block to this calculation
        );
        let past_median_time_manager = PastMedianTimeManager::new(
            headers_store.clone(),
            dag_traversal_manager.clone(),
            params.timestamp_deviation_tolerance as usize,
            params.genesis_timestamp,
        );
        let difficulty_manager = DifficultyManager::new(
            headers_store.clone(),
            params.genesis_bits,
            params.difficulty_window_size,
            params.target_time_per_block,
        );
        let depth_manager = BlockDepthManager::new(
            params.merge_depth,
            params.finality_depth,
            params.genesis_hash,
            depth_store.clone(),
            reachability_service.clone(),
            ghostdag_store.clone(),
        );
        let ghostdag_manager = GhostdagManager::new(
            params.genesis_hash,
            params.ghostdag_k,
            ghostdag_store.clone(),
            relations_service.clone(),
            headers_store.clone(),
            reachability_service.clone(),
        );

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
            params.genesis_hash,
            reachability_service.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            past_pruning_points_store.clone(),
        );

        let parents_manager = ParentsManager::new(
            params.max_block_level,
            params.genesis_hash,
            headers_store.clone(),
            reachability_service.clone(),
            relations_store.clone(),
        );

        let (sender, receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();
        let (body_sender, body_receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();
        let (virtual_sender, virtual_receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();

        let counters = Arc::new(ProcessingCounters::default());

        //
        // Thread-pools
        //

        // Pool for header and body processors
        let block_processors_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.block_processors_num_threads)
                .thread_name(|i| format!("block-pool-{}", i))
                .build()
                .unwrap(),
        );
        // We need a dedicated thread-pool for the virtual processor to avoid possible deadlocks probably caused by the
        // combined usage of `par_iter` (in virtual processor) and `rayon::spawn` (in header/body processors).
        // See for instance https://github.com/rayon-rs/rayon/issues/690
        let virtual_pool = Arc::new(
            rayon::ThreadPoolBuilder::new()
                .num_threads(perf_params.virtual_processor_num_threads)
                .thread_name(|i| format!("virtual-pool-{}", i))
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
            relations_store.clone(),
            reachability_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_excluded_store.clone(),
            statuses_store.clone(),
            pruning_store.clone(),
            depth_store,
            headers_selected_tip_store.clone(),
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            reachability_service.clone(),
            relations_service.clone(),
            past_median_time_manager.clone(),
            dag_traversal_manager.clone(),
            difficulty_manager.clone(),
            depth_manager.clone(),
            pruning_manager.clone(),
            parents_manager.clone(),
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
            params.genesis_hash,
        ));

        let virtual_processor = Arc::new(VirtualStateProcessor::new(
            virtual_receiver,
            virtual_pool,
            params,
            db.clone(),
            statuses_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_excluded_store,
            block_transactions_store.clone(),
            pruning_store.clone(),
            past_pruning_points_store,
            body_tips_store.clone(),
            utxo_diffs_store,
            utxo_multisets_store,
            acceptance_data_store,
            virtual_utxo_store,
            virtual_state_store,
            ghostdag_manager.clone(),
            reachability_service.clone(),
            relations_service.clone(),
            dag_traversal_manager.clone(),
            difficulty_manager.clone(),
            coinbase_manager.clone(),
            transaction_validator,
            past_median_time_manager.clone(),
            pruning_manager.clone(),
            parents_manager,
            depth_manager,
        ));

        Self {
            db,
            block_sender: sender,
            header_processor,
            body_processor,
            virtual_processor,
            statuses_store,
            relations_store,
            reachability_store,
            ghostdag_store,
            pruning_store,
            headers_selected_tip_store,
            body_tips_store,
            headers_store,
            block_transactions_store,

            statuses_service,
            relations_service,
            reachability_service,
            difficulty_manager,
            dag_traversal_manager,
            ghostdag_manager,
            past_median_time_manager,
            coinbase_manager,
            pruning_manager,

            counters,
        }
    }

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        // Ensure that reachability store is initialized
        reachability::init(self.reachability_store.write().deref_mut()).unwrap();

        // Ensure that genesis was processed
        self.header_processor.process_genesis_if_needed();
        self.body_processor.process_genesis_if_needed();
        self.virtual_processor.process_genesis_if_needed();

        // Spawn the asynchronous processors.
        let header_processor = self.header_processor.clone();
        let body_processor = self.body_processor.clone();
        let virtual_processor = self.virtual_processor.clone();

        vec![
            thread::Builder::new().name("header-processor".to_string()).spawn(move || header_processor.worker()).unwrap(),
            thread::Builder::new().name("body-processor".to_string()).spawn(move || body_processor.worker()).unwrap(),
            thread::Builder::new().name("virtual-processor".to_string()).spawn(move || virtual_processor.worker()).unwrap(),
        ]
    }

    pub fn validate_and_insert_block(&self, block: Block) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        let (tx, rx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender.send(BlockTask::Process(block, vec![tx])).unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::SeqCst);
        async { rx.await.unwrap() }
    }

    pub fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> BlockTemplate {
        self.virtual_processor.build_block_template(miner_data, txs)
    }

    pub fn body_tips(&self) -> Arc<BlockHashSet> {
        self.body_tips_store.read().get().unwrap()
    }

    pub fn block_status(&self, hash: Hash) -> BlockStatus {
        self.statuses_store.read().get(hash).unwrap()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.counters
    }

    pub fn signal_exit(&self) {
        self.block_sender.send(BlockTask::Exit).unwrap();
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.signal_exit();
        // Wait for async consensus processors to exit
        for handle in wait_handles {
            handle.join().unwrap();
        }
    }
}

impl ConsensusApi for Consensus {
    fn build_block_template(self: Arc<Self>, miner_data: MinerData, txs: Vec<Transaction>) -> BlockTemplate {
        self.as_ref().build_block_template(miner_data, txs)
    }

    fn validate_and_insert_block(
        self: Arc<Self>,
        block: Block,
        _update_virtual: bool,
    ) -> BoxFuture<'static, Result<BlockStatus, String>> {
        let result = self.as_ref().validate_and_insert_block(block);
        Box::pin(async move { result.await.map_err(|err| err.to_string()) })
    }
}

impl Service for Consensus {
    fn ident(self: Arc<Consensus>) -> &'static str {
        "consensus"
    }

    fn start(self: Arc<Consensus>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<Consensus>) {
        self.signal_exit()
    }
}
