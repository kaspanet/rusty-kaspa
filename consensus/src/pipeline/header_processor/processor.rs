use crate::{
    errors::{BlockProcessResult, RuleError},
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            block_window_cache::{BlockWindowCacheStore, BlockWindowHeap},
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::DbHeadersStore,
            headers_selected_tip::{DbHeadersSelectedTipStore, HeadersSelectedTipStoreReader},
            past_pruning_points::DbPastPruningPointsStore,
            pruning::{DbPruningStore, PruningPointInfo, PruningStoreReader},
            reachability::{DbReachabilityStore, ReachabilityStoreReader, StagingReachabilityStore},
            relations::{DbRelationsStore, RelationsStoreReader},
            selected_chain::{DbSelectedChainStore, SelectedChainStore},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            DB,
        },
    },
    params::Params,
    pipeline::deps_manager::{BlockProcessingMessage, BlockTask, BlockTaskDependencyManager, TaskId},
    processes::{
        block_depth::BlockDepthManager,
        difficulty::DifficultyManager,
        ghostdag::{ordering::SortableBlock, protocol::GhostdagManager},
        parents_builder::ParentsManager,
        past_median_time::PastMedianTimeManager,
        pruning::PruningManager,
        reachability::inquirer as reachability,
        traversal_manager::DagTraversalManager,
    },
};
use crossbeam_channel::{Receiver, Sender};
use itertools::Itertools;
use kaspa_consensus_core::{
    blockhash::{BlockHashExtensions, BlockHashes, ORIGIN},
    blockstatus::BlockStatus::{self, StatusHeaderOnly, StatusInvalid},
    config::genesis::GenesisBlock,
    header::Header,
    BlockHashSet, BlockLevel,
};
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use kaspa_utils::vec::VecExtensions;
use parking_lot::RwLock;
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::sync::{atomic::Ordering, Arc};

use super::super::ProcessingCounters;

pub struct HeaderProcessingContext {
    pub hash: Hash,
    pub header: Arc<Header>,
    pub pruning_info: PruningPointInfo,
    pub block_level: BlockLevel,
    pub non_pruned_parents: Vec<BlockHashes>,

    // Staging data
    pub ghostdag_data: Option<Vec<Arc<GhostdagData>>>,
    pub block_window_for_difficulty: Option<BlockWindowHeap>,
    pub block_window_for_past_median_time: Option<BlockWindowHeap>,
    pub mergeset_non_daa: BlockHashSet,
    pub merge_depth_root: Option<Hash>,
    pub finality_point: Option<Hash>,
}

impl HeaderProcessingContext {
    pub fn new(
        hash: Hash,
        header: Arc<Header>,
        block_level: BlockLevel,
        pruning_info: PruningPointInfo,
        non_pruned_parents: Vec<BlockHashes>,
    ) -> Self {
        Self {
            hash,
            header,
            block_level,
            pruning_info,
            non_pruned_parents,
            ghostdag_data: None,
            block_window_for_difficulty: None,
            mergeset_non_daa: Default::default(),
            block_window_for_past_median_time: None,
            merge_depth_root: None,
            finality_point: None,
        }
    }

    /// Returns the direct parents of this header after removal of pruned parents
    pub fn direct_non_pruned_parents(&mut self) -> BlockHashes {
        self.non_pruned_parents[0].clone()
    }

    /// Returns the pruning point at the time this header began processing
    pub fn pruning_point(&self) -> Hash {
        self.pruning_info.pruning_point
    }

    /// Returns the GHOSTDAG data of this header.
    /// NOTE: is expected to be called only after GHOSTDAG computation was pushed into the context
    pub fn ghostdag_data(&self) -> &Arc<GhostdagData> {
        &self.ghostdag_data.as_ref().unwrap()[0]
    }
}

pub struct HeaderProcessor {
    // Channels
    receiver: Receiver<BlockProcessingMessage>,
    body_sender: Sender<BlockProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) timestamp_deviation_tolerance: u64,
    pub(super) target_time_per_block: u64,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    pub(super) skip_proof_of_work: bool,
    pub(super) max_block_level: BlockLevel,

    // DB
    db: Arc<DB>,

    // Stores
    relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) pruning_store: Arc<RwLock<DbPruningStore>>,
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,
    pub(super) daa_store: Arc<DbDaaStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
    pub selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
    depth_store: Arc<DbDepthStore>,

    // Managers and services
    ghostdag_managers: Vec<
        GhostdagManager<
            DbGhostdagStore,
            MTRelationsService<DbRelationsStore>,
            MTReachabilityService<DbReachabilityStore>,
            DbHeadersStore,
        >,
    >,
    pub(super) dag_traversal_manager:
        DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) past_median_time_manager: PastMedianTimeManager<
        DbHeadersStore,
        DbGhostdagStore,
        BlockWindowCacheStore,
        DbReachabilityStore,
        MTRelationsService<DbRelationsStore>,
    >,
    pub(super) depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    pub(super) parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,

    // Dependency manager
    task_manager: BlockTaskDependencyManager,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl HeaderProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: Receiver<BlockProcessingMessage>,
        body_sender: Sender<BlockProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        relations_stores: Arc<RwLock<Vec<DbRelationsStore>>>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>,
        ghostdag_stores: Vec<Arc<DbGhostdagStore>>,
        headers_store: Arc<DbHeadersStore>,
        daa_store: Arc<DbDaaStore>,
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        depth_store: Arc<DbDepthStore>,
        headers_selected_tip_store: Arc<RwLock<DbHeadersSelectedTipStore>>,
        selected_chain_store: Arc<RwLock<DbSelectedChainStore>>,
        block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
        block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        past_median_time_manager: PastMedianTimeManager<
            DbHeadersStore,
            DbGhostdagStore,
            BlockWindowCacheStore,
            DbReachabilityStore,
            MTRelationsService<DbRelationsStore>,
        >,
        dag_traversal_manager: DagTraversalManager<
            DbGhostdagStore,
            BlockWindowCacheStore,
            DbReachabilityStore,
            MTRelationsService<DbRelationsStore>,
        >,
        difficulty_manager: DifficultyManager<DbHeadersStore>,
        depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
        parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
        ghostdag_managers: Vec<
            GhostdagManager<
                DbGhostdagStore,
                MTRelationsService<DbRelationsStore>,
                MTReachabilityService<DbReachabilityStore>,
                DbHeadersStore,
            >,
        >,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self {
            receiver,
            body_sender,
            thread_pool,
            genesis: params.genesis.clone(),
            difficulty_window_size: params.difficulty_window_size,
            db,
            relations_stores,
            reachability_store,
            ghostdag_stores,
            statuses_store,
            pruning_store,
            daa_store,
            headers_store,
            depth_store,
            headers_selected_tip_store,
            selected_chain_store,
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            ghostdag_managers,
            dag_traversal_manager,
            difficulty_manager,
            reachability_service,
            past_median_time_manager,
            depth_manager,
            pruning_manager,
            parents_manager,
            task_manager: BlockTaskDependencyManager::new(),
            counters,
            timestamp_deviation_tolerance: params.timestamp_deviation_tolerance,
            target_time_per_block: params.target_time_per_block,
            max_block_parents: params.max_block_parents,
            mergeset_size_limit: params.mergeset_size_limit,
            skip_proof_of_work: params.skip_proof_of_work,
            max_block_level: params.max_block_level,
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        while let Ok(msg) = self.receiver.recv() {
            match msg {
                BlockProcessingMessage::Exit => break,
                BlockProcessingMessage::Process(task, result_transmitter) => {
                    if let Some(task_id) = self.task_manager.register(task, result_transmitter) {
                        let processor = self.clone();
                        self.thread_pool.spawn(move || {
                            processor.queue_block(task_id);
                        });
                    }
                }
            };
        }

        // Wait until all workers are idle before exiting
        self.task_manager.wait_for_idle();

        // Pass the exit signal on to the following processor
        self.body_sender.send(BlockProcessingMessage::Exit).unwrap();
    }

    fn queue_block(self: &Arc<HeaderProcessor>, task_id: TaskId) {
        if let Some(task) = self.task_manager.try_begin(task_id) {
            let res = self.process_header(&task);

            let dependent_tasks = self.task_manager.end(task, |task, result_transmitter| {
                if res.is_err() || task.block().is_header_only() {
                    // We don't care if receivers were dropped
                    let _ = result_transmitter.send(res.clone());
                } else {
                    self.body_sender.send(BlockProcessingMessage::Process(task, result_transmitter)).unwrap();
                }
            });

            for dep in dependent_tasks {
                let processor = self.clone();
                self.thread_pool.spawn(move || processor.queue_block(dep));
            }
        }
    }

    fn process_header(&self, task: &BlockTask) -> BlockProcessResult<BlockStatus> {
        let header = &task.block().header;
        let status_option = self.statuses_store.read().get(header.hash).unwrap_option();

        match status_option {
            Some(StatusInvalid) => return Err(RuleError::KnownInvalid),
            Some(status) => return Ok(status),
            None => {}
        }

        // Validate the header depending on task type
        let ctx = match task {
            BlockTask::Ordinary { .. } => self.validate_header(header)?,
            BlockTask::Trusted { .. } => self.validate_trusted_header(header)?,
        };

        // Commit the header to stores
        self.commit_header(ctx, header);

        // Report counters
        self.counters.header_counts.fetch_add(1, Ordering::Relaxed);
        self.counters.dep_counts.fetch_add(header.direct_parents().len() as u64, Ordering::Relaxed);

        Ok(StatusHeaderOnly)
    }

    /// Runs full ordinary header validation
    fn validate_header(&self, header: &Arc<Header>) -> BlockProcessResult<HeaderProcessingContext> {
        let block_level = self.validate_header_in_isolation(header)?;
        let mut ctx = self.build_processing_context(header, block_level);
        self.validate_parent_relations(&mut ctx, header)?;
        self.ghostdag(&mut ctx);
        self.pre_pow_validation(&mut ctx, header)?;
        if let Err(e) = self.post_pow_validation(&mut ctx, header) {
            self.statuses_store.write().set(ctx.hash, StatusInvalid).unwrap();
            return Err(e);
        }
        Ok(ctx)
    }

    // Runs partial header validation for trusted blocks (currently validates only header-in-isolation and computes GHOSTDAG).
    fn validate_trusted_header(&self, header: &Arc<Header>) -> BlockProcessResult<HeaderProcessingContext> {
        // TODO: For now we skip most validations for trusted blocks, but in the future we should
        // employ some validations to avoid spam etc.
        let block_level = self.validate_header_in_isolation(header)?;
        let mut ctx = self.build_processing_context(header, block_level);
        self.ghostdag(&mut ctx);
        ctx.merge_depth_root = Some(ORIGIN);
        ctx.finality_point = Some(ORIGIN);
        Ok(ctx)
    }

    fn build_processing_context(&self, header: &Arc<Header>, block_level: u8) -> HeaderProcessingContext {
        HeaderProcessingContext::new(
            header.hash,
            header.clone(),
            block_level,
            self.pruning_store.read().get().unwrap(),
            self.collect_non_pruned_parents(header, block_level),
        )
    }

    /// Collects the non-pruned parents for all block levels
    fn collect_non_pruned_parents(&self, header: &Header, block_level: BlockLevel) -> Vec<Arc<Vec<Hash>>> {
        let relations_read = self.relations_stores.read();
        (0..=block_level)
            .map(|level| {
                Arc::new(
                    self.parents_manager
                        .parents_at_level(header, level)
                        .iter()
                        .copied()
                        .filter(|parent| relations_read[level as usize].has(*parent).unwrap())
                        .collect_vec()
                        .push_if_empty(ORIGIN),
                )
            })
            .collect_vec()
    }

    /// Runs the GHOSTDAG algorithm for all block levels and writes the data into the context (if hasn't run already)
    fn ghostdag(&self, ctx: &mut HeaderProcessingContext) {
        let ghostdag_data = (0..=ctx.block_level)
            .map(|level| {
                self.ghostdag_stores[level as usize].get_data(ctx.hash).unwrap_option().unwrap_or_else(|| {
                    Arc::new(self.ghostdag_managers[level as usize].ghostdag(&ctx.non_pruned_parents[level as usize]))
                })
            })
            .collect_vec();
        ctx.ghostdag_data = Some(ghostdag_data);
    }

    fn commit_header(&self, ctx: HeaderProcessingContext, header: &Header) {
        let ghostdag_data = ctx.ghostdag_data.as_ref().unwrap();
        let pp = ctx.pruning_point();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // Write to append only stores: this requires no lock and hence done first
        for (level, datum) in ghostdag_data.iter().enumerate() {
            if self.ghostdag_stores[level].has(ctx.hash).unwrap() {
                // The data might have been already written when applying the pruning proof.
                continue;
            }
            self.ghostdag_stores[level].insert_batch(&mut batch, ctx.hash, datum).unwrap();
        }
        if let Some(window) = ctx.block_window_for_difficulty {
            self.block_window_cache_for_difficulty.insert(ctx.hash, Arc::new(window));
        }

        if let Some(window) = ctx.block_window_for_past_median_time {
            self.block_window_cache_for_past_median_time.insert(ctx.hash, Arc::new(window));
        }

        self.daa_store.insert_batch(&mut batch, ctx.hash, Arc::new(ctx.mergeset_non_daa)).unwrap();
        if !self.headers_store.has(ctx.hash).unwrap() {
            // The data might have been already written when applying the pruning proof.
            self.headers_store.insert_batch(&mut batch, ctx.hash, ctx.header, ctx.block_level).unwrap();
        }
        if let Some(merge_depth_root) = ctx.merge_depth_root {
            self.depth_store.insert_batch(&mut batch, ctx.hash, merge_depth_root, ctx.finality_point.unwrap()).unwrap();
        }

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());

        if !staging.has(ctx.hash).unwrap() {
            // Add block to staging reachability
            let reachability_parent = if ctx.non_pruned_parents[0].len() == 1 && ctx.non_pruned_parents[0][0].is_origin() {
                ORIGIN
            } else {
                ghostdag_data[0].selected_parent
            };
            let reachability_read = &self.reachability_store.read();
            let mut reachability_mergeset =
                ghostdag_data[0].unordered_mergeset_without_selected_parent().filter(|hash| reachability_read.has(*hash).unwrap());
            reachability::add_block(&mut staging, ctx.hash, reachability_parent, &mut reachability_mergeset).unwrap();
        }

        // Non-append only stores need to use write locks.
        // Note we need to keep the lock write guards until the batch is written.
        let mut hst_write = self.headers_selected_tip_store.write();
        let mut sc_write = self.selected_chain_store.write();
        let prev_hst = hst_write.get().unwrap();
        if SortableBlock::new(ctx.hash, header.blue_work) > prev_hst
            && reachability::is_chain_ancestor_of(&staging, pp, ctx.hash).unwrap()
        // We can't calculate chain path for blocks that do not have the pruning point in their chain, so we just skip them.
        {
            // Hint reachability about the new tip.
            reachability::hint_virtual_selected_parent(&mut staging, ctx.hash).unwrap();
            hst_write.set_batch(&mut batch, SortableBlock::new(ctx.hash, header.blue_work)).unwrap();
            if ctx.hash != pp {
                let mut chain_path = self.dag_traversal_manager.calculate_chain_path(prev_hst.hash, ghostdag_data[0].selected_parent);
                chain_path.added.push(ctx.hash);
                sc_write.apply_changes(&mut batch, chain_path).unwrap();
            }
        }

        let mut relations_write = self.relations_stores.write();
        (0..=ctx.block_level)
            .map(|level| {
                (
                    level as usize,
                    self.parents_manager
                        .parents_at_level(header, level)
                        .iter()
                        .copied()
                        .filter(|parent| self.ghostdag_stores[level as usize].has(*parent).unwrap())
                        .collect_vec()
                        .push_if_empty(ORIGIN),
                )
            })
            .for_each(|(level, parents_by_level)| {
                if !relations_write[level].has(header.hash).unwrap() {
                    relations_write[level].insert_batch(&mut batch, header.hash, BlockHashes::new(parents_by_level)).unwrap();
                }
            });

        let statuses_write = self.statuses_store.set_batch(&mut batch, ctx.hash, StatusHeaderOnly).unwrap();

        // Write reachability data. Only at this brief moment the reachability store is locked for reads.
        // We take special care for this since reachability read queries are used throughout the system frequently.
        // Note we hold the lock until the batch is written
        let reachability_write = staging.commit(&mut batch).unwrap();

        // Flush the batch to the DB
        self.db.write(batch).unwrap();

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(reachability_write);
        drop(statuses_write);
        drop(relations_write);
        drop(hst_write);
        drop(sc_write);
    }

    pub fn process_genesis(&self) {
        // Init headers selected tip and selected chain stores
        let mut batch = WriteBatch::default();
        let mut sc_write = self.selected_chain_store.write();
        sc_write.init_with_pruning_point(&mut batch, self.genesis.hash).unwrap();
        let mut hst_write = self.headers_selected_tip_store.write();
        hst_write.set_batch(&mut batch, SortableBlock::new(self.genesis.hash, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write);
        drop(sc_write);

        // Write the genesis header
        let mut genesis_header: Header = (&self.genesis).into();
        // Force the provided genesis hash. Important for some tests which manually modify the genesis hash.
        // Note that for official nets (mainnet, testnet etc) they are guaranteed to be equal as enforced by a test in genesis.rs
        genesis_header.hash = self.genesis.hash;
        let genesis_header = Arc::new(genesis_header);
        let mut ctx = HeaderProcessingContext::new(
            self.genesis.hash,
            genesis_header.clone(),
            self.max_block_level,
            PruningPointInfo::from_genesis(self.genesis.hash),
            vec![BlockHashes::new(vec![ORIGIN])],
        );
        ctx.ghostdag_data =
            Some(self.ghostdag_managers.iter().map(|manager_by_level| Arc::new(manager_by_level.genesis_ghostdag_data())).collect());
        ctx.block_window_for_difficulty = Some(Default::default());
        ctx.block_window_for_past_median_time = Some(Default::default());
        ctx.merge_depth_root = Some(ORIGIN);
        ctx.finality_point = Some(ORIGIN);
        self.commit_header(ctx, &genesis_header);
    }

    pub fn init(&self) {
        if self.relations_stores.read()[0].has(ORIGIN).unwrap() {
            return;
        }

        let mut batch = WriteBatch::default();
        let mut relations_write = self.relations_stores.write();
        (0..=self.max_block_level)
            .for_each(|level| relations_write[level as usize].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap());
        let mut hst_write = self.headers_selected_tip_store.write();
        hst_write.set_batch(&mut batch, SortableBlock::new(ORIGIN, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write);
        drop(relations_write);
    }
}
