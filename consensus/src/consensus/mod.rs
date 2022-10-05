pub mod test_consensus;

use crate::{
    errors::BlockProcessResult,
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService, statuses::MTStatusesService},
        stores::{
            block_transactions::DbBlockTransactionsStore,
            block_window_cache::BlockWindowCacheStore,
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::DbGhostdagStore,
            headers::DbHeadersStore,
            pruning::DbPruningStore,
            reachability::DbReachabilityStore,
            relations::DbRelationsStore,
            statuses::{BlockStatus, DbStatusesStore},
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
        block_at_depth::BlockDepthManager, coinbase::CoinbaseManager, dagtraversalmanager::DagTraversalManager,
        difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager, mass::MassCalculator,
        pastmediantime::PastMedianTimeManager, reachability::inquirer as reachability, transaction_validator::TransactionValidator,
    },
};
use consensus_core::block::Block;
use crossbeam_channel::{unbounded, Receiver, Sender};
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::future::Future;
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
    virtual_processor: Arc<VirtualStateProcessor>,

    // Stores
    statuses_store: Arc<RwLock<DbStatusesStore>>,
    relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    pruning_store: Arc<RwLock<DbPruningStore>>,

    // Append-only stores
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,

    // Services and managers
    statuses_service: Arc<MTStatusesService<DbStatusesStore>>,
    relations_service: Arc<MTRelationsService<DbRelationsStore>>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) coinbase_manager: CoinbaseManager,

    // Counters
    pub counters: Arc<ProcessingCounters>,
}

impl Consensus {
    pub fn new(db: Arc<DB>, params: &Params) -> Self {
        let statuses_store = Arc::new(RwLock::new(DbStatusesStore::new(db.clone(), 100000)));
        let relations_store = Arc::new(RwLock::new(DbRelationsStore::new(db.clone(), 100000)));
        let reachability_store = Arc::new(RwLock::new(DbReachabilityStore::new(db.clone(), 100000)));
        let pruning_store = Arc::new(RwLock::new(DbPruningStore::new(db.clone())));
        let ghostdag_store = Arc::new(DbGhostdagStore::new(db.clone(), 100000));
        let daa_store = Arc::new(DbDaaStore::new(db.clone(), 100000));
        let headers_store = Arc::new(DbHeadersStore::new(db.clone(), 100000));
        let depth_store = Arc::new(DbDepthStore::new(db.clone(), 100000));
        let block_transactions_store = Arc::new(DbBlockTransactionsStore::new(db.clone(), 100000));
        let block_window_cache_for_difficulty = Arc::new(BlockWindowCacheStore::new(2000));
        let block_window_cache_for_past_median_time = Arc::new(BlockWindowCacheStore::new(2000));

        let statuses_service = Arc::new(MTStatusesService::new(statuses_store.clone()));
        let relations_service = Arc::new(MTRelationsService::new(relations_store.clone()));
        let reachability_service = MTReachabilityService::new(reachability_store.clone());
        let dag_traversal_manager = DagTraversalManager::new(
            params.genesis_hash,
            ghostdag_store.clone(),
            block_window_cache_for_difficulty.clone(),
            block_window_cache_for_past_median_time.clone(),
            params.difficulty_window_size,
            (2 * params.timestamp_deviation_tolerance - 1) as usize,
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
            headers_store.clone(),
        );

        let (sender, receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();
        let (body_sender, body_receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();
        let (virtual_sender, virtual_receiver): (Sender<BlockTask>, Receiver<BlockTask>) = unbounded();

        let counters = Arc::new(ProcessingCounters::default());

        let header_processor = Arc::new(HeaderProcessor::new(
            receiver,
            body_sender,
            params,
            db.clone(),
            relations_store.clone(),
            reachability_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_store,
            statuses_store.clone(),
            pruning_store.clone(),
            depth_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            reachability_service.clone(),
            relations_service.clone(),
            past_median_time_manager.clone(),
            dag_traversal_manager.clone(),
            difficulty_manager.clone(),
            depth_manager,
            counters.clone(),
        ));

        let body_processor = Arc::new(BlockBodyProcessor::new(
            body_receiver,
            virtual_sender,
            db.clone(),
            statuses_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            block_transactions_store.clone(),
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
            params,
            db.clone(),
            statuses_store.clone(),
            ghostdag_store.clone(),
            headers_store,
            block_transactions_store,
            ghostdag_manager.clone(),
            reachability_service.clone(),
            transaction_validator,
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

            statuses_service,
            relations_service,
            reachability_service,
            difficulty_manager,
            dag_traversal_manager,
            ghostdag_manager,
            past_median_time_manager,
            coinbase_manager,

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
            thread::spawn(move || header_processor.worker()),
            thread::spawn(move || body_processor.worker()),
            thread::spawn(move || virtual_processor.worker()),
        ]
    }

    pub fn validate_and_insert_block(&self, block: Arc<Block>) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        let (tx, rx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender.send(BlockTask::Process(block, vec![tx])).unwrap();
        async { rx.await.unwrap() }
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

impl Service for Consensus {
    fn ident(self: Arc<Consensus>) -> &'static str {
        "consensus"
    }

    fn start(self: Arc<Consensus>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<Consensus>) {
        self.signal_exit()
    }
}
