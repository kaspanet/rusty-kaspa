use std::{
    env, fs,
    path::PathBuf,
    sync::{Arc, Weak},
    thread::JoinHandle,
};

use async_channel::Sender;
use consensus_core::{
    block::{Block, MutableBlock},
    blockstatus::BlockStatus,
    events::ConsensusEvent,
    header::Header,
    merkle::calc_hash_merkle_root,
    subnets::SUBNETWORK_ID_COINBASE,
    tx::Transaction,
    BlockHashSet,
};
use hashes::Hash;
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::future::Future;

use crate::{
    config::Config,
    constants::TX_VERSION,
    errors::BlockProcessResult,
    model::{
        services::relations::MTRelationsService,
        stores::{
            block_window_cache::BlockWindowCacheStore,
            ghostdag::DbGhostdagStore,
            headers::{DbHeadersStore, HeaderStoreReader},
            pruning::PruningStoreReader,
            reachability::DbReachabilityStore,
            relations::DbRelationsStore,
            DB,
        },
    },
    params::Params,
    pipeline::{body_processor::BlockBodyProcessor, ProcessingCounters},
    processes::{past_median_time::PastMedianTimeManager, traversal_manager::DagTraversalManager},
    test_helpers::header_from_precomputed_hash,
};

use super::{Consensus, DbGhostdagManager, VirtualStores};

pub struct TestConsensus {
    pub consensus: Arc<Consensus>,
    pub params: Params,
    temp_db_lifetime: TempDbLifetime,
}

impl TestConsensus {
    pub fn new(db: Arc<DB>, config: &Config, consensus_sender: Sender<ConsensusEvent>) -> Self {
        Self {
            consensus: Arc::new(Consensus::new(db, config, consensus_sender)),
            params: config.params.clone(),
            temp_db_lifetime: Default::default(),
        }
    }

    pub fn consensus(&self) -> Arc<Consensus> {
        self.consensus.clone()
    }

    pub fn create_from_temp_db(config: &Config, consensus_sender: Sender<ConsensusEvent>) -> Self {
        let (temp_db_lifetime, db) = create_temp_db();
        Self { consensus: Arc::new(Consensus::new(db, config, consensus_sender)), params: config.params.clone(), temp_db_lifetime }
    }

    pub fn create_from_temp_db_and_dummy_sender(config: &Config) -> Self {
        let (temp_db_lifetime, db) = create_temp_db();
        let (dummy_consensus_sender, _) = async_channel::unbounded::<ConsensusEvent>();
        Self {
            consensus: Arc::new(Consensus::new(db, config, dummy_consensus_sender)),
            params: config.params.clone(),
            temp_db_lifetime,
        }
    }

    pub fn build_header_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents);
        let ghostdag_data = self.consensus.ghostdag_manager.ghostdag(header.direct_parents());
        header.pruning_point = self
            .consensus
            .pruning_manager
            .expected_header_pruning_point(ghostdag_data.to_compact(), self.consensus.pruning_store.read().get().unwrap());
        let window = self.consensus.dag_traversal_manager.block_window(&ghostdag_data, self.params.difficulty_window_size).unwrap();
        let (daa_score, _) = self
            .consensus
            .difficulty_manager
            .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &ghostdag_data);
        header.bits = self.consensus.difficulty_manager.calculate_difficulty_bits(&window);
        header.daa_score = daa_score;
        header.timestamp = self.consensus.past_median_time_manager.calc_past_median_time(&ghostdag_data).unwrap().0 + 1;
        header.blue_score = ghostdag_data.blue_score;
        header.blue_work = ghostdag_data.blue_work;

        header
    }

    pub fn add_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.validate_and_insert_block(self.build_block_with_parents(hash, parents).to_immutable())
    }

    pub fn build_block_with_parents_and_transactions(
        &self,
        hash: Hash,
        parents: Vec<Hash>,
        mut txs: Vec<Transaction>,
    ) -> MutableBlock {
        let mut header = self.build_header_with_parents(hash, parents);
        let cb_payload: Vec<u8> = header.blue_score.to_le_bytes().iter().copied() // Blue score
            .chain(self.consensus.coinbase_manager.calc_block_subsidy(header.daa_score).to_le_bytes().iter().copied()) // Subsidy
            .chain((0_u16).to_le_bytes().iter().copied()) // Script public key version
            .chain((0_u8).to_le_bytes().iter().copied()) // Script public key length
            .collect();

        let cb = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_COINBASE, 0, cb_payload);
        txs.insert(0, cb);
        header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        MutableBlock::new(header, txs)
    }

    pub fn build_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> MutableBlock {
        MutableBlock::from_header(self.build_header_with_parents(hash, parents))
    }

    pub fn validate_and_insert_block(&self, block: Block) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.consensus.as_ref().validate_and_insert_block(block, true)
    }

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        self.consensus.init()
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.consensus.shutdown(wait_handles)
    }

    pub fn dag_traversal_manager(
        &self,
    ) -> &DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>> {
        &self.consensus.dag_traversal_manager
    }

    pub fn ghostdag_store(&self) -> &Arc<DbGhostdagStore> {
        &self.consensus.ghostdag_store
    }

    pub fn reachability_store(&self) -> &Arc<RwLock<DbReachabilityStore>> {
        &self.consensus.reachability_store
    }

    pub fn headers_store(&self) -> Arc<impl HeaderStoreReader> {
        self.consensus.headers_store.clone()
    }

    pub fn virtual_stores(&self) -> Arc<RwLock<VirtualStores>> {
        self.consensus.virtual_stores.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.consensus.counters
    }

    pub fn block_body_processor(&self) -> &Arc<BlockBodyProcessor> {
        &self.consensus.body_processor
    }

    pub fn past_median_time_manager(
        &self,
    ) -> &PastMedianTimeManager<
        DbHeadersStore,
        DbGhostdagStore,
        BlockWindowCacheStore,
        DbReachabilityStore,
        MTRelationsService<DbRelationsStore>,
    > {
        &self.consensus.past_median_time_manager
    }

    pub fn body_tips(&self) -> Arc<BlockHashSet> {
        self.consensus.body_tips()
    }

    pub fn block_status(&self, hash: Hash) -> BlockStatus {
        self.consensus.block_status(hash)
    }

    pub fn ghostdag_manager(&self) -> &DbGhostdagManager {
        &self.consensus.ghostdag_manager
    }
}

impl std::ops::Deref for TestConsensus {
    type Target = Arc<Consensus>;

    fn deref(&self) -> &Self::Target {
        &self.consensus
    }
}

impl Service for TestConsensus {
    fn ident(self: Arc<TestConsensus>) -> &'static str {
        "test-consensus"
    }

    fn start(self: Arc<TestConsensus>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<TestConsensus>) {
        self.consensus.signal_exit()
    }
}

#[derive(Default)]
pub struct TempDbLifetime {
    weak_db_ref: Weak<DB>,
    tempdir: Option<tempfile::TempDir>,
}

impl TempDbLifetime {
    pub fn new(tempdir: tempfile::TempDir, weak_db_ref: Weak<DB>) -> Self {
        Self { tempdir: Some(tempdir), weak_db_ref }
    }

    /// Tracks the DB reference and makes sure all strong refs are cleaned up
    /// but does not remove the DB from disk when dropped.
    pub fn without_destroy(weak_db_ref: Weak<DB>) -> Self {
        Self { tempdir: None, weak_db_ref }
    }
}

impl Drop for TempDbLifetime {
    fn drop(&mut self) {
        for _ in 0..16 {
            if self.weak_db_ref.strong_count() > 0 {
                // Sometimes another thread is shuting-down and cleaning resources
                std::thread::sleep(std::time::Duration::from_millis(1000));
            } else {
                break;
            }
        }
        assert_eq!(self.weak_db_ref.strong_count(), 0, "DB is expected to have no strong references when lifetime is dropped");
        if let Some(dir) = self.tempdir.take() {
            let options = rocksdb::Options::default();
            let path_buf = dir.path().to_owned();
            let path = path_buf.to_str().unwrap();
            DB::destroy(&options, path).expect("DB is expected to be deletable since there are no references to it");
        }
    }
}

/// Creates a DB within a temp directory under `<OS SPECIFIC TEMP DIR>/kaspa-rust`
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB to exist.
pub fn create_temp_db_with_parallelism(parallelism: usize) -> (TempDbLifetime, Arc<DB>) {
    let global_tempdir = env::temp_dir();
    let kaspa_tempdir = global_tempdir.join("kaspa-rust");
    fs::create_dir_all(kaspa_tempdir.as_path()).unwrap();
    let db_tempdir = tempfile::tempdir_in(kaspa_tempdir.as_path()).unwrap();
    let db_path = db_tempdir.path().to_owned();
    let db = database::prelude::open_db(db_path, true, parallelism);
    (TempDbLifetime::new(db_tempdir, Arc::downgrade(&db)), db)
}

/// Creates a DB within a temp directory under `<OS SPECIFIC TEMP DIR>/kaspa-rust`
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB to exist.
pub fn create_temp_db() -> (TempDbLifetime, Arc<DB>) {
    // Temp DB usually indicates test environments, so we default to a single thread
    create_temp_db_with_parallelism(1)
}

/// Creates a DB within the provided directory path.
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB instance to exist.
pub fn create_permanent_db(db_path: String, parallelism: usize) -> (TempDbLifetime, Arc<DB>) {
    let db_dir = PathBuf::from(db_path);
    if let Err(e) = fs::create_dir(db_dir.as_path()) {
        match e.kind() {
            std::io::ErrorKind::AlreadyExists => panic!("The directory {db_dir:?} already exists"),
            _ => panic!("{e}"),
        }
    }
    let db = database::prelude::open_db(db_dir, true, parallelism);
    (TempDbLifetime::without_destroy(Arc::downgrade(&db)), db)
}

/// Loads an existing DB from the provided directory path.
/// Callers must keep the `TempDbLifetime` guard for as long as they wish the DB instance to exist.
pub fn load_existing_db(db_path: String, parallelism: usize) -> (TempDbLifetime, Arc<DB>) {
    let db_dir = PathBuf::from(db_path);
    let db = database::prelude::open_db(db_dir, false, parallelism);
    (TempDbLifetime::without_destroy(Arc::downgrade(&db)), db)
}
