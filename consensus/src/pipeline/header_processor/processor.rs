use crate::{
    errors::BlockProcessResult,
    model::{
        services::{reachability::MTReachabilityService, relations::MTRelationsService},
        stores::{
            block_window_cache::{BlockWindowCacheStore, BlockWindowHeap},
            daa::DbDaaStore,
            ghostdag::{DbGhostdagStore, GhostdagData},
            headers::DbHeadersStore,
            pruning::DbPruningStore,
            reachability::{DbReachabilityStore, StagingReachabilityStore},
            relations::DbRelationsStore,
            statuses::{
                BlockStatus::{StatusHeaderOnly, StatusInvalid},
                DbStatusesStore, StatusesStore, StatusesStoreReader,
            },
            DB,
        },
    },
    params::Params,
    processes::{
        dagtraversalmanager::DagTraversalManager, difficulty::DifficultyManager, ghostdag::protocol::GhostdagManager,
        pastmediantime::PastMedianTimeManager, reachability::inquirer as reachability,
    },
    test_helpers::header_from_precomputed_hash,
};
use consensus_core::{block::Block, blockhash::BlockHashes, header::Header};
use crossbeam::select;
use crossbeam_channel::Receiver;
use hashes::Hash;
use parking_lot::{Condvar, Mutex, RwLock};
use rocksdb::WriteBatch;
use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    sync::{atomic::Ordering, Arc},
};

use super::super::ProcessingCounters;

pub struct HeaderProcessingContext<'a> {
    pub hash: Hash,
    pub header: &'a Header,

    /// Mergeset w/o selected parent
    pub mergeset: Option<BlockHashes>,

    // Staging data
    pub ghostdag_data: Option<Arc<GhostdagData>>,
    pub block_window_for_difficulty: Option<BlockWindowHeap>,
    pub block_window_for_past_median_time: Option<BlockWindowHeap>,
    pub daa_added_blocks: Option<Vec<Hash>>,

    // Cache
    non_pruned_parents: Option<BlockHashes>,
}

impl<'a> HeaderProcessingContext<'a> {
    pub fn new(hash: Hash, header: &'a Header) -> Self {
        Self {
            hash,
            header,
            mergeset: None,
            ghostdag_data: None,
            non_pruned_parents: None,
            block_window_for_difficulty: None,
            daa_added_blocks: None,
            block_window_for_past_median_time: None,
        }
    }

    pub fn get_non_pruned_parents(&mut self) -> BlockHashes {
        if let Some(parents) = self.non_pruned_parents.clone() {
            return parents;
        }

        let non_pruned_parents = Arc::new(self.header.direct_parents().clone()); // TODO: Exclude pruned parents
        self.non_pruned_parents = Some(non_pruned_parents.clone());
        non_pruned_parents
    }
}

pub enum BlockTask {
    Exit,
    Process(Arc<Block>),
}

pub struct HeaderProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // Config
    pub(super) genesis_hash: Hash,
    pub(super) timestamp_deviation_tolerance: u64,
    pub(super) target_time_per_block: u64,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    // ghostdag_k: KType,

    // DB
    db: Arc<DB>,

    // Stores
    relations_store: Arc<RwLock<DbRelationsStore>>,
    reachability_store: Arc<RwLock<DbReachabilityStore>>,
    ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) pruning_store: Arc<RwLock<DbPruningStore>>,
    pub(super) block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
    pub(super) block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,
    pub(super) daa_store: Arc<DbDaaStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,

    // Managers and services
    ghostdag_manager: GhostdagManager<
        DbGhostdagStore,
        MTRelationsService<DbRelationsStore>,
        MTReachabilityService<DbReachabilityStore>,
    >,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,

    /// Holds pending block hashes and their dependent blocks
    pending: Mutex<HashMap<Hash, Vec<Arc<Block>>>>,

    // Counters
    counters: Arc<ProcessingCounters>,

    // Used to signal that workers are available/idle
    ready_signal: Condvar,
    idle_signal: Condvar,

    // Threshold to the number of pending items above which we wait for
    // workers to complete some work before queuing further work
    ready_threshold: usize,
}

impl HeaderProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: Receiver<BlockTask>, params: &Params, db: Arc<DB>, relations_store: Arc<RwLock<DbRelationsStore>>,
        reachability_store: Arc<RwLock<DbReachabilityStore>>, ghostdag_store: Arc<DbGhostdagStore>,
        headers_store: Arc<DbHeadersStore>, daa_store: Arc<DbDaaStore>, statuses_store: Arc<RwLock<DbStatusesStore>>,
        pruning_store: Arc<RwLock<DbPruningStore>>, block_window_cache_for_difficulty: Arc<BlockWindowCacheStore>,
        block_window_cache_for_past_median_time: Arc<BlockWindowCacheStore>,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        relations_service: Arc<MTRelationsService<DbRelationsStore>>,
        past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
        dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self {
            receiver,
            genesis_hash: params.genesis_hash,
            difficulty_window_size: params.difficulty_window_size,
            db,
            relations_store,
            reachability_store,
            ghostdag_store: ghostdag_store.clone(),
            statuses_store,
            pruning_store,
            daa_store,
            headers_store: headers_store.clone(),
            block_window_cache_for_difficulty,
            block_window_cache_for_past_median_time,
            ghostdag_manager: GhostdagManager::new(
                params.genesis_hash,
                params.ghostdag_k,
                ghostdag_store,
                relations_service,
                reachability_service.clone(),
            ),
            dag_traversal_manager,
            difficulty_manager: DifficultyManager::new(
                headers_store,
                params.genesis_bits,
                params.difficulty_window_size,
            ),
            reachability_service,
            past_median_time_manager,
            pending: Mutex::new(HashMap::new()),
            counters,
            ready_signal: Condvar::new(),
            idle_signal: Condvar::new(),

            // Note: If we ever switch to a non-global thread-pool,
            // then `num_threads` should be taken from that specific pool
            ready_threshold: rayon::current_num_threads() * 4,
            timestamp_deviation_tolerance: params.timestamp_deviation_tolerance,
            target_time_per_block: params.target_time_per_block,
            max_block_parents: params.max_block_parents,
        }
    }

    pub fn worker(self: &Arc<HeaderProcessor>) {
        loop {
            select! {
                recv(self.receiver) -> data => {
                    if let Ok(task) = data {
                        match task {
                            BlockTask::Exit => break,
                            BlockTask::Process(block) => {

                                let mut pending = self.pending.lock();

                                if let Vacant(e) = pending.entry(block.header.hash) {
                                    e.insert(Vec::new());
                                    if pending.len() > self.ready_threshold {
                                        // If the number of pending items is already too large,
                                        // wait for workers to signal readiness.
                                        self.ready_signal.wait(&mut pending);
                                    }

                                    let processor = self.clone();
                                    rayon::spawn(move || {
                                        processor.queue_block(block);
                                    });
                                }
                            }
                        };
                    } else {
                        // All senders are dropped
                        break;
                    }
                }
            }
        }

        // Wait until all workers are idle before exiting
        let mut pending = self.pending.lock();
        if !pending.is_empty() {
            self.idle_signal.wait(&mut pending);
        }
    }

    fn queue_block(self: &Arc<HeaderProcessor>, block: Arc<Block>) {
        let hash = block.header.hash;

        {
            // Lock pending manager. The contention around the manager is
            // expected to be negligible in header processing time
            let mut pending = self.pending.lock();

            for parent in block.header.direct_parents().iter() {
                if let Some(deps) = pending.get_mut(parent) {
                    deps.push(block);
                    return; // The block will be reprocessed once the pending parent completes processing
                }
            }
        }

        // TODO: report duplicate block to job sender
        if self.header_was_processed(hash) {
            return;
        }

        // TODO: report missing parents to job sender (currently will panic for missing keys)

        self.process_header(&block.header).unwrap(); // TODO: Handle error properly

        let mut pending = self.pending.lock();
        let deps = pending
            .remove(&hash)
            .expect("processed block is expected to be in pending map");

        if pending.len() == self.ready_threshold {
            self.ready_signal.notify_one();
        }

        if pending.is_empty() {
            self.idle_signal.notify_one();
        }

        for dep in deps {
            let processor = self.clone();
            rayon::spawn(move || processor.queue_block(dep));
        }
    }

    fn header_was_processed(self: &Arc<HeaderProcessor>, hash: Hash) -> bool {
        self.statuses_store.read().has(hash).unwrap()
    }

    fn process_header(self: &Arc<HeaderProcessor>, header: &Header) -> BlockProcessResult<()> {
        // Create processing context
        let mut ctx = HeaderProcessingContext::new(header.hash, header);

        // Run GHOSTDAG for the new header
        self.ghostdag_manager
            .add_block(&mut ctx, header.hash); // TODO: Run GHOSTDAG for all block levels

        //
        // TODO: imp all remaining header validation and processing steps :)
        //
        self.pre_pow_validation(&mut ctx, header)?;

        if let Err(e) = self.post_pow_validation(&mut ctx, header) {
            self.statuses_store
                .write()
                .set(ctx.hash, StatusInvalid)
                .unwrap();
            return Err(e);
        }

        self.commit_header(ctx, header);

        // Report counters
        self.counters
            .header_counts
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .dep_counts
            .fetch_add(header.direct_parents().len() as u64, Ordering::Relaxed);
        Ok(())
    }

    fn commit_header(self: &Arc<HeaderProcessor>, ctx: HeaderProcessingContext, header: &Header) {
        let ghostdag_data = ctx.ghostdag_data.unwrap();

        // Create staging reachability store. We use an upgradable read here to avoid concurrent
        // staging reachability operations. PERF: we assume that reachability processing time << header processing
        // time, and thus serializing this part will do no harm. However this should be benchmarked. The
        // alternative is to create a separate ReachabilityProcessor and to manage things more tightly.
        let mut staging = StagingReachabilityStore::new(self.reachability_store.upgradable_read());

        // Add block to staging reachability
        reachability::add_block(
            &mut staging,
            ctx.hash,
            ghostdag_data.selected_parent,
            &mut ctx.mergeset.unwrap().iter().cloned(),
        )
        .unwrap();
        // Hint reachability about the new tip.
        // TODO: imp header tips store and call this only for an actual header selected tip
        reachability::hint_virtual_selected_parent(&mut staging, ctx.hash).unwrap();

        // Create a DB batch writer
        let mut batch = WriteBatch::default();

        // Write to append only stores: this requires no lock
        self.ghostdag_store
            .insert_batch(&mut batch, ctx.hash, ghostdag_data)
            .unwrap();
        self.block_window_cache_for_difficulty
            .insert(ctx.hash, Arc::new(ctx.block_window_for_difficulty.unwrap()));
        self.block_window_cache_for_past_median_time
            .insert(ctx.hash, Arc::new(ctx.block_window_for_past_median_time.unwrap()));
        self.daa_store
            .insert_batch(&mut batch, ctx.hash, Arc::new(ctx.daa_added_blocks.unwrap()))
            .unwrap();
        self.headers_store
            .insert_batch(&mut batch, ctx.hash, Arc::new(ctx.header.clone()))
            .unwrap();

        // Non-append only stores need to use a write lock
        self.relations_store
            .write()
            .insert_batch(&mut batch, header.hash, BlockHashes::new(header.direct_parents().clone()))
            .unwrap(); // Note we lock until the batch is written

        self.statuses_store
            .write()
            .set_batch(&mut batch, ctx.hash, StatusHeaderOnly)
            .unwrap();

        // Write reachability data. Only at this brief moment the reachability store is locked for reads.
        // We take special care for this since reachability read queries are used throughout the system frequently.
        let write_guard = staging.commit(&mut batch).unwrap(); // Note we hold the lock until the batch is written

        // Flush the batch to the DB
        self.db.write(batch).unwrap();
    }

    pub fn process_genesis_if_needed(self: &Arc<HeaderProcessor>) {
        if self.header_was_processed(self.genesis_hash) {
            return;
        }
        let header = header_from_precomputed_hash(self.genesis_hash, vec![]); // TODO
        let mut ctx = HeaderProcessingContext::new(self.genesis_hash, &header);
        self.ghostdag_manager
            .add_genesis_if_needed(&mut ctx);
        ctx.block_window_for_difficulty = Some(Default::default());
        ctx.block_window_for_past_median_time = Some(Default::default());
        ctx.daa_added_blocks = Some(Default::default());
        self.commit_header(ctx, &header);
    }
}
