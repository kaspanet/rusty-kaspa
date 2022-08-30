use std::{env, fs, sync::Arc, thread::JoinHandle};

use consensus_core::{block::Block, header::Header};
use hashes::Hash;
use parking_lot::RwLock;

use crate::{
    errors::BlockProcessResult,
    model::stores::{
        block_window_cache::BlockWindowCacheStore, ghostdag::DbGhostdagStore, reachability::DbReachabilityStore, DB,
    },
    params::Params,
    pipeline::header_processor::HeaderProcessingContext,
    processes::dagtraversalmanager::DagTraversalManager,
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
        Self {
            consensus: Consensus::new(db, params),
            params: params.clone(),
            temp_db_lifetime: TempDbLifetime::new(None),
        }
    }

    pub fn create_from_temp_db(params: &Params) -> Self {
        let (temp_db_lifetime, db) = create_temp_db();
        Self { consensus: Consensus::new(db, params), params: params.clone(), temp_db_lifetime }
    }

    pub fn build_header_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents);
        let mut ctx: HeaderProcessingContext = HeaderProcessingContext::new(hash, &header);
        self.consensus
            .ghostdag_manager
            .add_block(&mut ctx, hash);

        let ghostdag_data = ctx.ghostdag_data.unwrap();
        let window = self
            .consensus
            .dag_traversal_manager
            .block_window(ghostdag_data.clone(), self.params.difficulty_window_size);

        let mut window_hashes = window.into_iter().map(|item| item.0.hash);

        let (daa_score, _) = self
            .consensus
            .difficulty_manager
            .calc_daa_score_and_added_blocks(&mut window_hashes, &ghostdag_data);

        header.daa_score = daa_score;

        header.timestamp = self
            .consensus
            .past_median_time_manager
            .calc_past_median_time(ghostdag_data)
            .0
            + 1;
        header
    }

    pub fn build_block_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Block {
        Block::from_header(self.build_header_with_parents(hash, parents))
    }

    pub async fn validate_and_insert_block(&self, block: Arc<Block>) -> BlockProcessResult<()> {
        self.consensus
            .validate_and_insert_block(block)
            .await
    }

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        self.consensus.init()
    }

    pub fn signal_exit(&self) {
        self.consensus.signal_exit()
    }

    pub fn shutdown(self, wait_handles: Vec<JoinHandle<()>>) {
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
}

pub struct TempDbLifetime {
    tempdir: Option<tempfile::TempDir>,
}

impl TempDbLifetime {
    pub fn new(tempdir: Option<tempfile::TempDir>) -> Self {
        Self { tempdir }
    }
}

impl Drop for TempDbLifetime {
    fn drop(&mut self) {
        if let Some(dir) = self.tempdir.take() {
            let options = rocksdb::Options::default();
            DB::destroy(&options, dir.path().to_owned().to_str().unwrap()).unwrap();
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

    (TempDbLifetime::new(Some(db_tempdir)), Arc::new(DB::open_default(db_path.to_str().unwrap()).unwrap()))
}
