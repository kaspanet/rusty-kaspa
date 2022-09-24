use std::{
    env, fs,
    sync::{Arc, Weak},
    thread::JoinHandle,
};

use consensus_core::{block::Block, header::Header, merkle::calc_hash_merkle_root, subnets::SUBNETWORK_ID_COINBASE, tx::Transaction};
use hashes::Hash;
use kaspa_core::{core::Core, service::Service};
use parking_lot::RwLock;
use std::future::Future;

use crate::{
    constants::TX_VERSION,
    errors::BlockProcessResult,
    model::stores::{
        block_window_cache::BlockWindowCacheStore, ghostdag::DbGhostdagStore, headers::DbHeadersStore,
        reachability::DbReachabilityStore, statuses::BlockStatus, DB,
    },
    params::Params,
    pipeline::{body_processor::BlockBodyProcessor, ProcessingCounters},
    processes::{dagtraversalmanager::DagTraversalManager, pastmediantime::PastMedianTimeManager},
    test_helpers::header_from_precomputed_hash,
};

use super::Consensus;

pub struct TestConsensus {
    consensus: Consensus,
    params: Params,
    temp_db_lifetime: TempDbLifetime,
}

impl TestConsensus {
    pub fn new(db: Arc<DB>, params: &Params) -> Self {
        Self { consensus: Consensus::new(db, params), params: params.clone(), temp_db_lifetime: Default::default() }
    }

    pub fn create_from_temp_db(params: &Params) -> Self {
        let (temp_db_lifetime, db) = create_temp_db();
        Self { consensus: Consensus::new(db, params), params: params.clone(), temp_db_lifetime }
    }

    pub fn build_header_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents);
        let ghostdag_data = self.consensus.ghostdag_manager.ghostdag(header.direct_parents());

        let window = self.consensus.dag_traversal_manager.block_window(ghostdag_data.clone(), self.params.difficulty_window_size);
        let (daa_score, _) = self
            .consensus
            .difficulty_manager
            .calc_daa_score_and_added_blocks(&mut window.iter().map(|item| item.0.hash), &ghostdag_data);

        header.bits = self.consensus.difficulty_manager.calculate_difficulty_bits(&window);
        header.daa_score = daa_score;

        header.timestamp = self.consensus.past_median_time_manager.calc_past_median_time(ghostdag_data.clone()).0 + 1;
        header.blue_score = ghostdag_data.blue_score;
        header.blue_work = ghostdag_data.blue_work;

        header
    }

    pub fn add_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.validate_and_insert_block(Arc::new(self.build_block_with_parents(hash, parents)))
    }

    pub fn build_block_with_parents_and_transactions(&self, hash: Hash, parents: Vec<Hash>, txs: Vec<Transaction>) -> Block {
        let mut header = self.build_header_with_parents(hash, parents);
        let mut cb_payload: Vec<u8> = vec![];
        cb_payload.append(&mut header.blue_score.to_le_bytes().to_vec());
        cb_payload.append(&mut (self.consensus.coinbase_manager.calc_block_subsidy(header.daa_score)).to_le_bytes().to_vec()); // Subsidy
        cb_payload.append(&mut (0_u16).to_le_bytes().to_vec()); // Script public key version
        cb_payload.append(&mut (0_u8).to_le_bytes().to_vec()); // Script public key length
        cb_payload.append(&mut vec![]); // Script public key

        let cb = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_COINBASE, 0, cb_payload, 0);
        let final_txs = vec![vec![cb], txs].concat();
        header.hash_merkle_root = calc_hash_merkle_root(final_txs.iter());
        Block { header, transactions: Arc::new(final_txs) }
    }

    pub fn build_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Block {
        Block::from_header(self.build_header_with_parents(hash, parents))
    }

    pub fn validate_and_insert_block(&self, block: Arc<Block>) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.consensus.validate_and_insert_block(block)
    }

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        self.consensus.init()
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.consensus.shutdown(wait_handles)
    }

    pub fn dag_traversal_manager(&self) -> &DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore> {
        &self.consensus.dag_traversal_manager
    }

    pub fn ghostdag_store(&self) -> &Arc<DbGhostdagStore> {
        &self.consensus.ghostdag_store
    }

    pub fn reachability_store(&self) -> &Arc<RwLock<DbReachabilityStore>> {
        &self.consensus.reachability_store
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        &self.consensus.counters
    }

    pub fn block_body_processor(&self) -> &Arc<BlockBodyProcessor> {
        &self.consensus.body_processor
    }

    pub fn past_median_time_manager(&self) -> &PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore> {
        &self.consensus.past_median_time_manager
    }
}

impl Service for TestConsensus {
    fn ident(self: Arc<TestConsensus>) -> &'static str {
        "test-consensus"
    }

    fn start(self: Arc<TestConsensus>, core: Arc<Core>) -> Vec<JoinHandle<()>> {
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
pub fn create_temp_db() -> (TempDbLifetime, Arc<DB>) {
    let global_tempdir = env::temp_dir();
    let kaspa_tempdir = global_tempdir.join("kaspa-rust");
    fs::create_dir_all(kaspa_tempdir.as_path()).unwrap();

    let db_tempdir = tempfile::tempdir_in(kaspa_tempdir.as_path()).unwrap();
    let db_path = db_tempdir.path().to_owned();

    let db = Arc::new(DB::open_default(db_path.to_str().unwrap()).unwrap());
    (TempDbLifetime::new(db_tempdir, Arc::downgrade(&db)), db)
}
