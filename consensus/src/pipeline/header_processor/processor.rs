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
    pipeline::deps_manager::{BlockProcessingMessage, BlockTaskDependencyManager, TaskId},
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
use parking_lot::RwLock;
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::sync::{atomic::Ordering, Arc};

use super::super::ProcessingCounters;

pub struct HeaderProcessingContext<'a> {
    pub hash: Hash,
    pub header: &'a Arc<Header>,
    pub pruning_info: PruningPointInfo,
    pub non_pruned_parents: Vec<BlockHashes>,

    // Staging data
    pub ghostdag_data: Option<Vec<Arc<GhostdagData>>>,
    pub block_window_for_difficulty: Option<BlockWindowHeap>,
    pub block_window_for_past_median_time: Option<BlockWindowHeap>,
    pub mergeset_non_daa: Option<BlockHashSet>,
    pub merge_depth_root: Option<Hash>,
    pub finality_point: Option<Hash>,
    pub block_level: Option<BlockLevel>,
}

impl<'a> HeaderProcessingContext<'a> {
    pub fn new(hash: Hash, header: &'a Arc<Header>, pruning_info: PruningPointInfo, non_pruned_parents: Vec<BlockHashes>) -> Self {
        Self {
            hash,
            header,
            pruning_info,
            non_pruned_parents,
            ghostdag_data: None,
            block_window_for_difficulty: None,
            mergeset_non_daa: None,
            block_window_for_past_median_time: None,
            merge_depth_root: None,
            finality_point: None,
            block_level: None,
        }
    }

    pub fn get_non_pruned_parents(&mut self) -> BlockHashes {
        self.non_pruned_parents[0].clone()
    }

    pub fn pruning_point(&self) -> Hash {
        self.pruning_info.pruning_point
    }

    pub fn get_ghostdag_data(&self) -> Option<Arc<GhostdagData>> {
        Some(self.ghostdag_data.as_ref()?[0].clone())
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
            let res = self.process_header(&task.block.header, &task.trusted_ghostdag_data);

            let dependent_tasks = self.task_manager.end(task, |task, result_transmitter| {
                if res.is_err() || task.block.is_header_only() {
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

    fn process_header(
        self: &Arc<HeaderProcessor>,
        header: &Arc<Header>,
        optional_trusted_ghostdag_data: &Option<Arc<GhostdagData>>,
    ) -> BlockProcessResult<BlockStatus> {
        let is_trusted = optional_trusted_ghostdag_data.is_some();
        let status_option = self.statuses_store.read().get(header.hash).unwrap_option();

        match status_option {
            Some(StatusInvalid) => return Err(RuleError::KnownInvalid),
            Some(status) => return Ok(status),
            None => {}
        }

        // Create processing context
        let is_genesis = header.direct_parents().is_empty();
        let pruning_point = self.pruning_store.read().get().unwrap();
        let relations_read = self.relations_stores.read();
        let non_pruned_parents = (0..=self.max_block_level)
            .map(|level| {
                Arc::new(if is_genesis {
                    vec![ORIGIN]
                } else {
                    let filtered = self
                        .parents_manager
                        .parents_at_level(header, level)
                        .iter()
                        .copied()
                        .filter(|parent| relations_read[level as usize].has(*parent).unwrap())
                        .collect_vec();
                    if filtered.is_empty() {
                        vec![ORIGIN]
                    } else {
                        filtered
                    }
                })
            })
            .collect_vec();
        drop(relations_read);
        let mut ctx = HeaderProcessingContext::new(header.hash, header, pruning_point, non_pruned_parents);
        if is_trusted {
            ctx.mergeset_non_daa = Some(Default::default()); // TODO: Check that it's fine for coinbase calculations.
        }

        // Run all header validations for the new header
        self.pre_ghostdag_validation(&mut ctx, header, is_trusted)?;
        let ghostdag_data = (0..=ctx.block_level.unwrap())
            .map(|level| {
                if let Some(gd) = self.ghostdag_stores[level as usize].get_data(ctx.hash).unwrap_option() {
                    gd
                } else {
                    Arc::new(self.ghostdag_managers[level as usize].ghostdag(&ctx.non_pruned_parents[level as usize]))
                }
            })
            .collect_vec();
        ctx.ghostdag_data = Some(ghostdag_data);
        if is_trusted {
            // let gd_data = ctx.get_ghostdag_data().unwrap();
            // let merge_depth_root = self.depth_manager.calc_merge_depth_root(&gd_data, ctx.pruning_point());
            // let finality_point = self.depth_manager.calc_finality_point(&gd_data, ctx.pruning_point());
            ctx.merge_depth_root = Some(ORIGIN);
            ctx.finality_point = Some(ORIGIN);
        }

        if !is_trusted {
            // TODO: For now we skip all validations for trusted blocks, but in the future we should
            // employ some validations to avoid spam etc.
            self.pre_pow_validation(&mut ctx, header)?;
            if let Err(e) = self.post_pow_validation(&mut ctx, header) {
                self.statuses_store.write().set(ctx.hash, StatusInvalid).unwrap();
                return Err(e);
            }
        }

        self.commit_header(ctx, header);

        // Report counters
        self.counters.header_counts.fetch_add(1, Ordering::Relaxed);
        self.counters.dep_counts.fetch_add(header.direct_parents().len() as u64, Ordering::Relaxed);
        Ok(StatusHeaderOnly)
    }

    fn commit_header(self: &Arc<HeaderProcessor>, ctx: HeaderProcessingContext, header: &Arc<Header>) {
        let ghostdag_data = ctx.ghostdag_data.as_ref().unwrap();
        let pp = ctx.pruning_point();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // Write to append only stores: this requires no lock and hence done first
        // TODO: Insert all levels data
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

        self.daa_store.insert_batch(&mut batch, ctx.hash, Arc::new(ctx.mergeset_non_daa.unwrap())).unwrap();
        if !self.headers_store.has(ctx.hash).unwrap() {
            // The data might have been already written when applying the pruning proof.
            self.headers_store.insert_batch(&mut batch, ctx.hash, ctx.header.clone(), ctx.block_level.unwrap()).unwrap();
        }
        if let Some(merge_depth_root) = ctx.merge_depth_root {
            self.depth_store.insert_batch(&mut batch, ctx.hash, merge_depth_root, ctx.finality_point.unwrap()).unwrap();
        }

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());

        let has_reachability = staging.has(ctx.hash).unwrap();
        if !has_reachability {
            // Add block to staging reachability
            let reachability_parent = if ctx.non_pruned_parents[0].len() == 1 && ctx.non_pruned_parents[0][0].is_origin() {
                ORIGIN
            } else {
                ghostdag_data[0].selected_parent
            };

            let mut reachability_mergeset = ghostdag_data[0]
                .unordered_mergeset_without_selected_parent()
                .filter(|hash| self.reachability_store.read().has(*hash).unwrap()); // TODO: Use read lock only once
            reachability::add_block(&mut staging, ctx.hash, reachability_parent, &mut reachability_mergeset).unwrap();
        }

        // Non-append only stores need to use write locks.
        // Note we need to keep the lock write guards until the batch is written.
        let mut hst_write_guard = self.headers_selected_tip_store.write();
        let mut sc_write_guard = self.selected_chain_store.write();
        let prev_hst = hst_write_guard.get().unwrap();
        if SortableBlock::new(ctx.hash, header.blue_work) > prev_hst
            && reachability::is_chain_ancestor_of(&staging, pp, ctx.hash).unwrap()
        // We can't calculate chain path for blocks that do not have the pruning point in their chain, so we just skip them.
        {
            // Hint reachability about the new tip.
            reachability::hint_virtual_selected_parent(&mut staging, ctx.hash).unwrap();
            hst_write_guard.set_batch(&mut batch, SortableBlock::new(ctx.hash, header.blue_work)).unwrap();
            if ctx.hash != pp {
                let mut chain_path = self.dag_traversal_manager.calculate_chain_path(prev_hst.hash, ghostdag_data[0].selected_parent);
                chain_path.added.push(ctx.hash);
                sc_write_guard.apply_changes(&mut batch, chain_path).unwrap();
            }
        }

        let is_genesis = header.direct_parents().is_empty();
        let parents = (0..=ctx.block_level.unwrap()).map(|level| {
            Arc::new(if is_genesis {
                vec![ORIGIN]
            } else {
                self.parents_manager
                    .parents_at_level(ctx.header, level)
                    .iter()
                    .copied()
                    .filter(|parent| self.ghostdag_stores[level as usize].has(*parent).unwrap())
                    .collect_vec()
            })
        });

        let mut relations_write_guard = self.relations_stores.write();
        parents.enumerate().for_each(|(level, parent_by_level)| {
            if !relations_write_guard[level].has(header.hash).unwrap() {
                relations_write_guard[level].insert_batch(&mut batch, header.hash, parent_by_level).unwrap();
            }
        });

        let statuses_write_guard = self.statuses_store.set_batch(&mut batch, ctx.hash, StatusHeaderOnly).unwrap();

        // Write reachability data. Only at this brief moment the reachability store is locked for reads.
        // We take special care for this since reachability read queries are used throughout the system frequently.
        // Note we hold the lock until the batch is written
        let reachability_write_guard = staging.commit(&mut batch).unwrap();

        // Flush the batch to the DB
        self.db.write(batch).unwrap();

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(reachability_write_guard);
        drop(statuses_write_guard);
        drop(relations_write_guard);
        drop(hst_write_guard);
        drop(sc_write_guard);
    }

    pub fn process_genesis(self: &Arc<HeaderProcessor>) {
        // Init headers selected tip and selected chain stores
        let mut batch = WriteBatch::default();
        let mut sc_write_guard = self.selected_chain_store.write();
        sc_write_guard.init_with_pruning_point(&mut batch, self.genesis.hash).unwrap();
        let mut hst_write_guard = self.headers_selected_tip_store.write();
        hst_write_guard.set_batch(&mut batch, SortableBlock::new(self.genesis.hash, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write_guard);
        drop(sc_write_guard);

        // Write the genesis header
        let mut genesis_header: Header = (&self.genesis).into();
        // Force the provided genesis hash. Important for some tests which manually modify the genesis hash.
        // Note that for official nets (mainnet, testnet etc) they are guaranteed to be equal as enforced by a test in genesis.rs
        genesis_header.hash = self.genesis.hash;
        let genesis_header = Arc::new(genesis_header);
        let mut ctx = HeaderProcessingContext::new(
            self.genesis.hash,
            &genesis_header,
            PruningPointInfo::from_genesis(self.genesis.hash),
            vec![BlockHashes::new(vec![ORIGIN])],
        );
        ctx.ghostdag_data = Some(self.ghostdag_managers.iter().map(|m| Arc::new(m.genesis_ghostdag_data())).collect());
        ctx.block_window_for_difficulty = Some(Default::default());
        ctx.block_window_for_past_median_time = Some(Default::default());
        ctx.mergeset_non_daa = Some(Default::default());
        ctx.merge_depth_root = Some(ORIGIN);
        ctx.finality_point = Some(ORIGIN);
        ctx.block_level = Some(self.max_block_level);
        self.commit_header(ctx, &genesis_header);
    }

    pub fn init(self: &Arc<HeaderProcessor>) {
        if self.relations_stores.read()[0].has(ORIGIN).unwrap() {
            return;
        }

        let mut batch = WriteBatch::default();
        let mut relations_write_guard = self.relations_stores.write();
        (0..=self.max_block_level).for_each(|level| {
            relations_write_guard[level as usize].insert_batch(&mut batch, ORIGIN, BlockHashes::new(vec![])).unwrap()
        });
        let mut hst_write_guard = self.headers_selected_tip_store.write();
        hst_write_guard.set_batch(&mut batch, SortableBlock::new(ORIGIN, 0.into())).unwrap();
        self.db.write(batch).unwrap();
        drop(hst_write_guard);
        drop(relations_write_guard);
    }
}
