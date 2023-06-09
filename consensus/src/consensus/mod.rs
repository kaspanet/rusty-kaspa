pub mod ctl;
pub mod factory;
pub mod services;
pub mod storage;
pub mod test_consensus;

use crate::{
    config::Config,
    errors::{BlockProcessResult, RuleError},
    model::{
        services::reachability::ReachabilityService,
        stores::{
            acceptance_data::AcceptanceDataStoreReader,
            block_transactions::BlockTransactionsStoreReader,
            ghostdag::{GhostdagData, GhostdagStoreReader},
            headers::HeaderStoreReader,
            headers_selected_tip::HeadersSelectedTipStoreReader,
            past_pruning_points::PastPruningPointsStoreReader,
            pruning::PruningStoreReader,
            relations::RelationsStoreReader,
            statuses::StatusesStoreReader,
            tips::TipsStoreReader,
            utxo_set::{UtxoSetStore, UtxoSetStoreReader},
            virtual_state::VirtualStateStoreReader,
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
    processes::window::{WindowManager, WindowType},
};
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    api::{BlockValidationFuture, ConsensusApi},
    block::{Block, BlockTemplate},
    block_count::BlockCount,
    blockhash::BlockHashExtensions,
    blockstatus::BlockStatus,
    coinbase::MinerData,
    errors::{
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        tx::TxResult,
    },
    errors::{difficulty::DifficultyError, pruning::PruningImportError},
    header::Header,
    muhash::MuHashExtensions,
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{MutableTransaction, Transaction, TransactionOutpoint, UtxoEntry},
    BlockHashSet, ChainPath,
};
use kaspa_consensus_notify::root::ConsensusNotificationRoot;

use crossbeam_channel::{
    bounded as bounded_crossbeam, unbounded as unbounded_crossbeam, Receiver as CrossbeamReceiver, Sender as CrossbeamSender,
};
use itertools::Itertools;
use kaspa_consensusmanager::{SessionLock, SessionReadGuard};
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;

use std::thread::{self, JoinHandle};
use std::{
    future::Future,
    iter::once,
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};
use tokio::sync::oneshot;

use self::{services::ConsensusServices, storage::ConsensusStorage};

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
    ) -> Self {
        let params = &config.params;
        let perf_params = &config.perf;

        //
        // Storage layer
        //

        let storage = ConsensusStorage::new(db.clone(), config.clone());

        //
        // Services and managers
        //

        let services = ConsensusServices::new(db.clone(), storage.clone(), config.clone());

        //
        // Processor channels
        //

        let (sender, receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (body_sender, body_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
        let (virtual_sender, virtual_receiver): (CrossbeamSender<BlockProcessingMessage>, CrossbeamReceiver<BlockProcessingMessage>) =
            unbounded_crossbeam();
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
            db.clone(),
            storage.statuses_store.clone(),
            storage.ghostdag_primary_store.clone(),
            storage.headers_store.clone(),
            storage.block_transactions_store.clone(),
            storage.body_tips_store.clone(),
            services.reachability_service.clone(),
            services.coinbase_manager.clone(),
            services.mass_calculator.clone(),
            services.transaction_validator.clone(),
            services.window_manager.clone(),
            params.max_block_mass,
            params.genesis.clone(),
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
        ));

        let pruning_processor =
            Arc::new(PruningProcessor::new(pruning_receiver, db.clone(), &storage, &services, pruning_lock.clone(), config.clone()));

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
            storage,
            services,
            pruning_lock,
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

    /// Acquires a consensus session, blocking data-pruning from occurring until released
    pub fn acquire_session(&self) -> SessionReadGuard {
        self.pruning_lock.blocking_read()
    }

    fn validate_and_insert_block_impl(&self, task: BlockTask) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        let (tx, rx): (BlockResultSender, _) = oneshot::channel();
        self.block_sender.send(BlockProcessingMessage::Process(task, tx)).unwrap();
        self.counters.blocks_submitted.fetch_add(1, Ordering::Relaxed);
        async { rx.await.unwrap() }
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

    fn estimate_network_hashes_per_second_impl(&self, ghostdag_data: &GhostdagData, window_size: usize) -> ConsensusResult<u64> {
        let window = match self.services.window_manager.block_window(ghostdag_data, WindowType::VaryingWindow(window_size)) {
            Ok(w) => w,
            Err(RuleError::InsufficientDaaWindowSize(s)) => return Err(DifficultyError::InsufficientWindowData(s).into()),
            Err(e) => panic!("unexpected error: {e}"),
        };
        Ok(self.services.window_manager.estimate_network_hashes_per_second(window)?)
    }
}

impl ConsensusApi for Consensus {
    fn build_block_template(&self, miner_data: MinerData, txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        self.virtual_processor.build_block_template(miner_data, txs)
    }

    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFuture {
        let result = self.validate_and_insert_block_impl(BlockTask::Ordinary { block });
        Box::pin(result)
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFuture {
        let result = self.validate_and_insert_block_impl(BlockTask::Trusted { block: tb.block });
        Box::pin(result)
    }

    fn validate_mempool_transaction_and_populate(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        self.virtual_processor.validate_mempool_transaction_and_populate(transaction)?;
        Ok(())
    }

    fn calculate_transaction_mass(&self, transaction: &Transaction) -> u64 {
        self.services.mass_calculator.calc_tx_mass(transaction)
    }

    fn get_virtual_daa_score(&self) -> u64 {
        self.virtual_stores.read().state.get().unwrap().daa_score
    }

    fn get_virtual_bits(&self) -> u32 {
        self.virtual_stores.read().state.get().unwrap().bits
    }

    fn get_virtual_past_median_time(&self) -> u64 {
        self.virtual_stores.read().state.get().unwrap().past_median_time
    }

    fn get_virtual_merge_depth_root(&self) -> Option<Hash> {
        // TODO: consider saving the merge depth root as part of virtual state
        // TODO: unwrap on pruning_point and virtual state reads when staging consensus is implemented
        let Some(pruning_point) = self.pruning_point_store.read().pruning_point().unwrap_option() else { return None; };
        let Some(virtual_state) = self.virtual_stores.read().state.get().unwrap_option() else { return None; };
        let virtual_ghostdag_data = &virtual_state.ghostdag_data;
        let root = self.services.depth_manager.calc_merge_depth_root(virtual_ghostdag_data, pruning_point);
        if root.is_origin() {
            None
        } else {
            Some(root)
        }
    }

    fn get_sink(&self) -> Hash {
        self.virtual_stores.read().state.get().unwrap().ghostdag_data.selected_parent
    }

    fn get_sink_timestamp(&self) -> u64 {
        self.headers_store.get_timestamp(self.get_sink()).unwrap()
    }

    fn get_source(&self) -> Hash {
        if self.config.is_archival {
            // we use the history root in archival cases.
            return self.pruning_point_store.read().history_root().unwrap();
        }
        self.pruning_point_store.read().pruning_point().unwrap()
    }

    /// Estimates number of blocks and headers stored in the node
    ///
    /// This is an estimation based on the daa score difference between the node's `source` and `sink`'s daa score,
    /// as such, it does not include non-daa blocks, and does not include headers stored as part of the pruning proof.  
    fn estimate_block_count(&self) -> BlockCount {
        let count = self.get_virtual_daa_score() - self.get_header(self.get_source()).unwrap().daa_score;
        BlockCount { header_count: count, block_count: count }
    }

    fn is_nearly_synced(&self) -> bool {
        // See comment within `config.is_nearly_synced`
        self.config.is_nearly_synced(self.get_sink_timestamp(), self.headers_store.get_daa_score(self.get_sink()).unwrap())
    }

    fn get_virtual_chain_from_block(&self, hash: Hash) -> ConsensusResult<ChainPath> {
        // Calculate chain changes between the given hash and the
        // sink. Note that we explicitly don't
        // do the calculation against the virtual itself so that we
        // won't later need to remove it from the result.
        self.validate_block_exists(hash)?;
        Ok(self.services.dag_traversal_manager.calculate_chain_path(hash, self.get_sink()))
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        self.virtual_stores.read().state.get().unwrap().parents.iter().copied().collect()
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
        self.body_tips().iter().copied().collect_vec()
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

    fn validate_pruning_proof(&self, proof: &PruningPointProof) -> Result<(), PruningImportError> {
        self.services.pruning_proof_manager.validate_pruning_point_proof(proof)
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) {
        self.services.pruning_proof_manager.apply_proof(proof, trusted_set)
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) {
        self.services.pruning_proof_manager.import_pruning_points(&pruning_points)
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        // TODO: Check if a db tx is needed. We probably need some kind of a flag that is set on this function to true, and then
        // is set to false on the end of import_pruning_point_utxo_set. On any failure on any of those functions (and also if the
        // node starts when the flag is true) the related data will be deleted and the flag will be set to false.
        let mut pruning_utxoset_write = self.pruning_utxoset_stores.write();
        pruning_utxoset_write.utxo_set.write_many(utxoset_chunk).unwrap();
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
        Ok(self.services.reachability_service.is_chain_ancestor_of(low, high))
    }

    // max_blocks has to be greater than the merge set size limit
    fn get_hashes_between(&self, low: Hash, high: Hash, max_blocks: usize) -> ConsensusResult<(Vec<Hash>, Hash)> {
        assert!(max_blocks as u64 > self.config.mergeset_size_limit);
        self.validate_block_exists(low)?;
        self.validate_block_exists(high)?;

        Ok(self.services.sync_manager.antipast_hashes_between(low, high, Some(max_blocks)))
    }

    fn get_header(&self, hash: Hash) -> ConsensusResult<Arc<Header>> {
        self.validate_block_exists(hash)?;
        Ok(self.headers_store.get_header(hash).unwrap())
    }

    fn get_headers_selected_tip(&self) -> Hash {
        self.headers_selected_tip_store.read().get().unwrap().hash
    }

    fn get_anticone_from_pov(&self, hash: Hash, context: Hash, max_traversal_allowed: Option<u64>) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(hash)?;
        Ok(self.services.dag_traversal_manager.anticone(hash, std::iter::once(context), max_traversal_allowed)?)
    }

    fn get_anticone(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(hash)?;
        Ok(self.services.dag_traversal_manager.anticone(
            hash,
            self.virtual_stores.read().state.get().unwrap().parents.iter().copied(),
            None,
        )?)
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        self.services.pruning_proof_manager.get_pruning_point_proof()
    }

    fn create_headers_selected_chain_block_locator(&self, low: Option<Hash>, high: Option<Hash>) -> ConsensusResult<Vec<Hash>> {
        if let Some(low) = low {
            self.validate_block_exists(low)?;
        }

        if let Some(high) = high {
            self.validate_block_exists(high)?;
        }

        Ok(self.services.sync_manager.create_headers_selected_chain_block_locator(low, high)?)
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        let current_pp_info = self.pruning_point_store.read().get().unwrap();
        (0..current_pp_info.index)
            .map(|index| self.past_pruning_points_store.get(index).unwrap())
            .chain(once(current_pp_info.pruning_point))
            .map(|hash| self.headers_store.get_header(hash).unwrap())
            .collect_vec()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
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
            header: self.headers_store.get_header(hash).unwrap(),
            transactions: self.block_transactions_store.get(hash).unwrap(),
        })
    }

    fn get_block_even_if_header_only(&self, hash: Hash) -> ConsensusResult<Block> {
        let Some(status) = self.statuses_store.read().get(hash).unwrap_option().filter(|&status| status.has_block_header()) else {
            return Err(ConsensusError::BlockNotFound(hash));
        };
        Ok(Block {
            header: self.headers_store.get_header(hash).unwrap(),
            transactions: if status.is_header_only() { Arc::new(vec![]) } else { self.block_transactions_store.get(hash).unwrap() },
        })
    }

    fn get_ghostdag_data(&self, hash: Hash) -> ConsensusResult<ExternalGhostdagData> {
        match self.get_block_status(hash) {
            None => return Err(ConsensusError::BlockNotFound(hash)),
            Some(BlockStatus::StatusInvalid) => return Err(ConsensusError::InvalidBlock(hash)),
            _ => {}
        };
        Ok((&*self.ghostdag_primary_store.get_data(hash).unwrap()).into())
    }

    fn get_block_children(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.services.relations_service.get_children(hash).unwrap_option()
    }

    fn get_block_parents(&self, hash: Hash) -> Option<Arc<Vec<Hash>>> {
        self.services.relations_service.get_parents(hash).unwrap_option()
    }

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        self.statuses_store.read().get(hash).unwrap_option()
    }

    fn get_block_acceptance_data(&self, hash: Hash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.validate_block_exists(hash)?;
        self.acceptance_data_store.get(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash))
    }

    fn get_blocks_acceptance_data(&self, hashes: &[Hash]) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        hashes
            .iter()
            .copied()
            .map(|hash| {
                self.validate_block_exists(hash)?;
                self.acceptance_data_store.get(hash).unwrap_option().ok_or(ConsensusError::MissingData(hash))
            })
            .collect::<ConsensusResult<Vec<_>>>()
    }

    fn is_chain_block(&self, hash: Hash) -> ConsensusResult<bool> {
        self.is_chain_ancestor_of(hash, self.get_sink())
    }

    fn get_missing_block_body_hashes(&self, high: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(high)?;
        Ok(self.services.sync_manager.get_missing_block_body_hashes(high)?)
    }

    fn pruning_point(&self) -> Option<Hash> {
        self.pruning_point_store.read().pruning_point().unwrap_option()
    }

    fn get_daa_window(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(hash)?;
        Ok(self
            .services
            .window_manager
            .block_window(&self.ghostdag_primary_store.get_data(hash).unwrap(), WindowType::SampledDifficultyWindow)
            .unwrap()
            .deref()
            .iter()
            .map(|block| block.0.hash)
            .collect())
    }

    fn get_trusted_block_associated_ghostdag_data_block_hashes(&self, hash: Hash) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(hash)?;

        // In order to guarantee the chain height is at least k, we check that the pruning point is not genesis.
        if self.pruning_point().unwrap() == self.config.genesis.hash {
            return Err(ConsensusError::UnexpectedPruningPoint);
        }

        let mut hashes = Vec::with_capacity(self.config.params.ghostdag_k as usize);
        let mut current = hash;
        // TODO: This will crash if we don't have the data for k blocks in the past of
        // current. The syncee should validate it got all of the associated data.
        for _ in 0..=self.config.params.ghostdag_k {
            hashes.push(current);
            current = self.ghostdag_primary_store.get_selected_parent(current).unwrap();
        }
        Ok(hashes)
    }

    fn create_block_locator_from_pruning_point(&self, high: Hash, limit: usize) -> ConsensusResult<Vec<Hash>> {
        self.validate_block_exists(high)?;

        let pp_read_guard = self.pruning_point_store.read();
        let pp = pp_read_guard.pruning_point().unwrap();
        Ok(self.services.sync_manager.create_block_locator_from_pruning_point(high, pp, Some(limit))?)
    }

    fn estimate_network_hashes_per_second(&self, start_hash: Option<Hash>, window_size: usize) -> ConsensusResult<u64> {
        match start_hash {
            Some(hash) => {
                self.validate_block_exists(hash)?;
                let ghostdag_data = self.ghostdag_primary_store.get_data(hash).unwrap();
                self.estimate_network_hashes_per_second_impl(&ghostdag_data, window_size)
            }
            None => {
                let virtual_state = self.virtual_stores.read().state.get().unwrap();
                self.estimate_network_hashes_per_second_impl(&virtual_state.ghostdag_data, window_size)
            }
        }
    }
}
