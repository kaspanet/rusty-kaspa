use async_channel::Sender;
use kaspa_consensus_core::{
    api::ConsensusApi, block::MutableBlock, blockstatus::BlockStatus, header::Header, merkle::calc_hash_merkle_root,
    subnets::SUBNETWORK_ID_COINBASE, tx::Transaction,
};
use kaspa_consensus_notify::{notification::Notification, root::ConsensusNotificationRoot};
use kaspa_core::{core::Core, service::Service};
use kaspa_database::utils::{create_temp_db, DbLifetime};
use kaspa_hashes::Hash;
use parking_lot::RwLock;

use std::future::Future;
use std::{sync::Arc, thread::JoinHandle};

use crate::{
    config::Config,
    constants::TX_VERSION,
    errors::BlockProcessResult,
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            ghostdag::DbGhostdagStore, headers::HeaderStoreReader, pruning::PruningStoreReader, reachability::DbReachabilityStore,
            virtual_state::VirtualStores, DB,
        },
    },
    params::Params,
    pipeline::{body_processor::BlockBodyProcessor, virtual_processor::VirtualStateProcessor, ProcessingCounters},
    test_helpers::header_from_precomputed_hash,
};

use super::services::{DbDagTraversalManager, DbPastMedianTimeManager};
use super::{Consensus, DbGhostdagManager};

pub struct TestConsensus {
    consensus: Arc<Consensus>,
    params: Params,
    db_lifetime: DbLifetime,
}

impl TestConsensus {
    /// Creates a test consensus instance based on `config` with the provided `db` and `notification_sender`
    pub fn with_db(db: Arc<DB>, config: &Config, notification_sender: Sender<Notification>) -> Self {
        let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_sender));
        let counters = Arc::new(ProcessingCounters::default());
        Self {
            consensus: Arc::new(Consensus::new(db, Arc::new(config.clone()), Default::default(), notification_root, counters)),
            params: config.params.clone(),
            db_lifetime: Default::default(),
        }
    }

    /// Creates a test consensus instance based on `config` with a temp DB and the provided `notification_sender`
    pub fn with_notifier(config: &Config, notification_sender: Sender<Notification>) -> Self {
        let (db_lifetime, db) = create_temp_db();
        let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_sender));
        let counters = Arc::new(ProcessingCounters::default());
        Self {
            consensus: Arc::new(Consensus::new(db, Arc::new(config.clone()), Default::default(), notification_root, counters)),
            params: config.params.clone(),
            db_lifetime,
        }
    }

    /// Creates a test consensus instance based on `config` with a temp DB and no notifier
    pub fn new(config: &Config) -> Self {
        let (db_lifetime, db) = create_temp_db();
        let (dummy_notification_sender, _) = async_channel::unbounded();
        let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
        let counters = Arc::new(ProcessingCounters::default());
        Self {
            consensus: Arc::new(Consensus::new(db, Arc::new(config.clone()), Default::default(), notification_root, counters)),
            params: config.params.clone(),
            db_lifetime,
        }
    }

    /// Clone the inner consensus Arc. For general usage of the underlying consensus simply deref
    pub fn consensus_clone(&self) -> Arc<Consensus> {
        self.consensus.clone()
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    pub fn build_header_with_parents(&self, hash: Hash, parents: Vec<Hash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents);
        let ghostdag_data = self.consensus.services.ghostdag_primary_manager.ghostdag(header.direct_parents());
        header.pruning_point = self
            .consensus
            .services
            .pruning_manager
            .expected_header_pruning_point(ghostdag_data.to_compact(), self.consensus.pruning_point_store.read().get().unwrap());
        let window =
            self.consensus.services.dag_traversal_manager.block_window(&ghostdag_data, self.params.difficulty_window_size).unwrap();
        let (daa_score, _) = self
            .consensus
            .services
            .difficulty_manager
            .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &ghostdag_data);
        header.bits = self.consensus.services.difficulty_manager.calculate_difficulty_bits(&window);
        header.daa_score = daa_score;
        header.timestamp = self.consensus.services.past_median_time_manager.calc_past_median_time(&ghostdag_data).unwrap().0 + 1;
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
            .chain(self.consensus.services.coinbase_manager.calc_block_subsidy(header.daa_score).to_le_bytes().iter().copied()) // Subsidy
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

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        self.consensus.run_processors()
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.consensus.shutdown(wait_handles)
    }

    pub fn dag_traversal_manager(&self) -> &DbDagTraversalManager {
        &self.consensus.services.dag_traversal_manager
    }

    pub fn ghostdag_store(&self) -> &Arc<DbGhostdagStore> {
        &self.consensus.ghostdag_primary_store
    }

    pub fn reachability_store(&self) -> &Arc<RwLock<DbReachabilityStore>> {
        &self.consensus.reachability_store
    }

    pub fn reachability_service(&self) -> &MTReachabilityService<DbReachabilityStore> {
        &self.consensus.services.reachability_service
    }

    pub fn headers_store(&self) -> Arc<impl HeaderStoreReader> {
        self.consensus.headers_store.clone()
    }

    pub fn virtual_stores(&self) -> Arc<RwLock<VirtualStores>> {
        self.consensus.virtual_stores.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        self.consensus.processing_counters()
    }

    pub fn block_body_processor(&self) -> &Arc<BlockBodyProcessor> {
        &self.consensus.body_processor
    }

    pub fn virtual_processor(&self) -> &Arc<VirtualStateProcessor> {
        &self.consensus.virtual_processor
    }

    pub fn past_median_time_manager(&self) -> &DbPastMedianTimeManager {
        &self.consensus.services.past_median_time_manager
    }

    pub fn ghostdag_manager(&self) -> &DbGhostdagManager {
        &self.consensus.services.ghostdag_primary_manager
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
