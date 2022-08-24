use crate::{
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService, statuses::MTStatusesService},
        stores::{
            block_window_cache::BlockWindowCacheStore, daa::DbDaaStore, ghostdag::DbGhostdagStore,
            headers::DbHeadersStore, pruning::DbPruningStore, reachability::DbReachabilityStore,
            relations::DbRelationsStore, statuses::DbStatusesStore, DB,
        },
    },
    params::Params,
    pipeline::{
        header_processor::{BlockTask, HeaderProcessor},
        ProcessingCounters,
    },
    processes::{
        dagtraversalmanager::DagTraversalManager, difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager,
        pastmediantime::PastMedianTimeManager, reachability::inquirer as reachability,
    },
};
use consensus_core::block::Block;
use crossbeam_channel::{bounded, Receiver, Sender};
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::{
    ops::DerefMut,
    sync::Arc,
    thread::{self, JoinHandle},
};

pub struct Consensus {
    // DB
    db: Arc<DB>,

    // Channels
    block_sender: Sender<BlockTask>,

    // Processors
    header_processor: Arc<HeaderProcessor>,

    // Stores
    statuses_store: Arc<RwLock<DbStatusesStore>>,
    relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,

    // Append-only stores
    ghostdag_store: Arc<DbGhostdagStore>,

    // Services and managers
    statuses_service: Arc<MTStatusesService<DbStatusesStore>>,
    relations_service: Arc<MTRelationsService<DbRelationsStore>>,
    reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) ghostdag_manager: GhostdagManager<
        DbGhostdagStore,
        MTRelationsService<DbRelationsStore>,
        MTReachabilityService<DbReachabilityStore>,
    >,
    pub(super) past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,

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
        let block_window_cache_store = Arc::new(BlockWindowCacheStore::new(2000));

        let statuses_service = Arc::new(MTStatusesService::new(statuses_store.clone()));
        let relations_service = Arc::new(MTRelationsService::new(relations_store.clone()));
        let reachability_service = MTReachabilityService::new(reachability_store.clone());
        let dag_traversal_manager = DagTraversalManager::new(
            params.genesis_hash,
            ghostdag_store.clone(),
            block_window_cache_store.clone(),
            params.difficulty_window_size,
        );
        let past_median_time_manager = PastMedianTimeManager::new(
            headers_store.clone(),
            dag_traversal_manager.clone(),
            params.timestamp_deviation_tolerance as usize,
            params.genesis_timestamp,
        );

        let (sender, receiver): (Sender<BlockTask>, Receiver<BlockTask>) = bounded(2000);
        let counters = Arc::new(ProcessingCounters::default());

        let header_processor = Arc::new(HeaderProcessor::new(
            receiver,
            params,
            db.clone(),
            relations_store.clone(),
            reachability_store.clone(),
            ghostdag_store.clone(),
            headers_store.clone(),
            daa_store,
            statuses_store.clone(),
            pruning_store,
            block_window_cache_store,
            reachability_service.clone(),
            relations_service.clone(),
            past_median_time_manager.clone(),
            counters.clone(),
        ));

        Self {
            db,
            block_sender: sender,
            header_processor,
            statuses_store,
            relations_store,
            reachability_store,
            ghostdag_store: ghostdag_store.clone(),

            statuses_service,
            relations_service: relations_service.clone(),
            reachability_service: reachability_service.clone(),
            difficulty_manager: DifficultyManager::new(
                headers_store,
                params.genesis_bits,
                params.difficulty_window_size,
            ),
            dag_traversal_manager,
            ghostdag_manager: GhostdagManager::new(
                params.genesis_hash,
                params.ghostdag_k,
                ghostdag_store,
                relations_service,
                reachability_service,
            ),
            past_median_time_manager,

            counters,
        }
    }

    pub fn init(&self) -> JoinHandle<()> {
        // Ensure that reachability store is initialized
        reachability::init(self.reachability_store.write().deref_mut()).unwrap();

        // Ensure that genesis was processed
        self.header_processor.process_genesis_if_needed();

        // Spawn the asynchronous header processor.
        let header_processor = self.header_processor.clone();
        thread::spawn(move || header_processor.worker())

        // TODO: add block body processor and virtual state processor workers and return a vec of join handles.
    }

    pub fn validate_and_insert_block(&self, block: Arc<Block>) {
        self.block_sender
            .send(BlockTask::Process(block))
            .unwrap();
    }

    pub fn signal_exit(&self) {
        self.block_sender.send(BlockTask::Exit).unwrap();
    }

    /// Drops consensus, and specifically drops sender channels so that
    /// internal workers fold up and can be joined.
    pub fn drop(self) -> (Arc<RwLock<DbReachabilityStore>>, Arc<DbGhostdagStore>) {
        self.signal_exit();
        (self.reachability_store, self.ghostdag_store)
    }
}

impl Service for Consensus {
    fn ident(self: Arc<Consensus>) -> String {
        "consensus".to_owned()
    }

    fn start(self: Arc<Consensus>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
        vec![self.init()]
    }

    fn stop(self: Arc<Consensus>) {
        self.signal_exit()
    }
}
