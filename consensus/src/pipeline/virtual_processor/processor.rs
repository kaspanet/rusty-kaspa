use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_transactions::DbBlockTransactionsStore,
            block_window_cache::BlockWindowCacheStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            statuses::{
                BlockStatus::{self, StatusDisqualifiedFromChain, StatusUTXOPendingVerification, StatusUTXOValid},
                DbStatusesStore, StatusesStore, StatusesStoreReader,
            },
            DB,
        },
    },
    params::Params,
    pipeline::{deps_manager::BlockTask, virtual_processor::utxo_validation::UtxoProcessingContext},
    processes::{
        dagtraversalmanager::DagTraversalManager, difficulty::DifficultyManager, pruning::PruningManager,
        transaction_validator::TransactionValidator,
    },
};
use consensus_core::{
    block::Block,
    blockhash::{self, VIRTUAL},
    utxo::{utxo_collection::UtxoCollection, utxo_collection::UtxoCollectionExtensions, utxo_diff::UtxoDiff, utxo_view},
    BlockHashMap, BlockHashSet,
};
use hashes::Hash;
use kaspa_core::trace;
use muhash::MuHash;

use crossbeam_channel::Receiver;
use itertools::Itertools;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::{
    collections::hash_map::Entry::{Occupied, Vacant},
    iter::FromIterator,
    sync::{
        atomic::{self, AtomicBool},
        Arc,
    },
};

pub struct VirtualStateProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis_hash: Hash,
    // pub(super) timestamp_deviation_tolerance: u64,
    // pub(super) target_time_per_block: u64,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    // pub(super) genesis_bits: u32,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) transaction_validator: TransactionValidator<DbHeadersStore>,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,

    is_updating_pruning_point_or_candidate: AtomicBool,
}

impl VirtualStateProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: Receiver<BlockTask>,
        thread_pool: Arc<ThreadPool>,
        db: Arc<DB>,
        params: &Params,
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
        headers_store: Arc<DbHeadersStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
        difficulty_manager: DifficultyManager<DbHeadersStore>,
        transaction_validator: TransactionValidator<DbHeadersStore>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    ) -> Self {
        Self {
            receiver,
            thread_pool,
            db,
            statuses_store,
            headers_store,
            ghostdag_store,
            block_transactions_store,
            pruning_store,
            past_pruning_points_store,
            ghostdag_manager,
            reachability_service,
            genesis_hash: params.genesis_hash,
            max_block_parents: params.max_block_parents,
            difficulty_window_size: params.difficulty_window_size,
            mergeset_size_limit: params.mergeset_size_limit,
            dag_traversal_manager,
            difficulty_manager,
            transaction_validator,
            pruning_manager,
            is_updating_pruning_point_or_candidate: false.into(),
        }
    }

    pub fn worker(self: &Arc<Self>) {
        let mut state = VirtualState::new(self.genesis_hash, self.ghostdag_manager.ghostdag(&[self.genesis_hash]));
        'outer: while let Ok(first_task) = self.receiver.recv() {
            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info
            let tasks: Vec<BlockTask> = std::iter::once(first_task).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", tasks.len());

            let mut blocks = tasks.iter().map_while(|t| if let BlockTask::Process(b, _) = t { Some(b) } else { None });
            self.resolve_virtual(&mut blocks, &mut state).unwrap();

            let statuses_read = self.statuses_store.read();
            for task in tasks {
                match task {
                    BlockTask::Exit => break 'outer,
                    BlockTask::Process(block, result_transmitters) => {
                        for transmitter in result_transmitters {
                            // We don't care if receivers were dropped
                            let _ = transmitter.send(Ok(statuses_read.get(block.hash()).unwrap()));
                        }
                    }
                };
            }
        }
    }

    fn resolve_virtual<'a>(
        self: &Arc<Self>,
        blocks: &mut impl Iterator<Item = &'a Arc<Block>>,
        state: &mut VirtualState,
    ) -> BlockProcessResult<()> {
        for block in blocks {
            let status = self.statuses_store.read().get(block.header.hash).unwrap();
            match status {
                StatusUTXOPendingVerification | StatusUTXOValid | StatusDisqualifiedFromChain => {}
                _ => panic!("unexpected block status {:?}", status),
            }

            // Update tips
            let parents_set = BlockHashSet::from_iter(block.header.direct_parents().iter().cloned());
            state.virtual_parents.retain(|t| !parents_set.contains(t));
            state.virtual_parents.push(block.header.hash);
        }

        // TEMP: using all tips as virtual parents
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&state.virtual_parents);

        // TODO: check finality violation
        // TODO: can return if virtual parents did not change

        let prev_selected = state.ghostdag_data.selected_parent;
        let new_selected = virtual_ghostdag_data.selected_parent;

        let mut split_point = blockhash::ORIGIN;
        let mut accumulated_diff = state.virtual_diff.clone().to_reversed();

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(prev_selected) {
            if self.reachability_service.is_chain_ancestor_of(current, new_selected) {
                split_point = current;
                break;
            }

            let mergeset_diff = state.utxo_diffs.get(&current).unwrap();
            // Apply the diff in reverse
            accumulated_diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
        }

        // Walk back up to the new virtual selected parent candidate
        for (selected_parent, current) in
            self.reachability_service.forward_chain_iterator(split_point, new_selected, true).tuple_windows()
        {
            match state.utxo_diffs.entry(current) {
                Occupied(e) => {
                    let mergeset_diff = e.get();
                    accumulated_diff.with_diff_in_place(mergeset_diff).unwrap();

                    // Temp logic
                    assert!(state.multiset_hashes.contains_key(&current));
                }
                Vacant(e) => {
                    if self.statuses_store.read().get(selected_parent).unwrap() == StatusDisqualifiedFromChain {
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                        continue; // TODO: optimize
                    }

                    let header = self.headers_store.get_header(current).unwrap();
                    let mergeset_data = self.ghostdag_store.get_data(current).unwrap();
                    let pov_daa_score = header.daa_score;

                    // Temp logic
                    assert!(!state.multiset_hashes.contains_key(&current));

                    let selected_parent_multiset_hash = &state.multiset_hashes.get(&mergeset_data.selected_parent).unwrap();
                    let selected_parent_utxo_view = utxo_view::compose_one_diff_layer(&state.utxo_set, &accumulated_diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data, selected_parent_multiset_hash);

                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, pov_daa_score);
                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &header);

                    if let Err(rule_error) = res {
                        trace!("{:?}", rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                    } else {
                        // TODO: batch write
                        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        e.insert(ctx.mergeset_diff);
                        state.multiset_hashes.insert(current, ctx.multiset_hash);
                        self.statuses_store.write().set(current, StatusUTXOValid).unwrap();
                    }
                }
            }
        }

        match self.statuses_store.read().get(new_selected).unwrap() {
            BlockStatus::StatusUTXOValid => {
                // TODO: batch write

                // Calc new virtual diff
                let selected_parent_multiset_hash = &state.multiset_hashes.get(&virtual_ghostdag_data.selected_parent).unwrap();
                let selected_parent_utxo_view = utxo_view::compose_one_diff_layer(&state.utxo_set, &accumulated_diff);
                let mut ctx = UtxoProcessingContext::new(virtual_ghostdag_data.clone(), selected_parent_multiset_hash);

                // Calc virtual DAA score
                let window = self.dag_traversal_manager.block_window(virtual_ghostdag_data.clone(), self.difficulty_window_size);
                let (virtual_daa_score, _) = self
                    .difficulty_manager
                    .calc_daa_score_and_added_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_ghostdag_data);
                self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, virtual_daa_score);

                // Update the accumulated diff
                accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

                // Store new virtual data
                state.virtual_diff = ctx.mergeset_diff;
                state.ghostdag_data = virtual_ghostdag_data;

                // Apply the accumulated diff
                state.utxo_set.remove_many(&accumulated_diff.remove);
                state.utxo_set.add_many(&accumulated_diff.add);
            }
            BlockStatus::StatusDisqualifiedFromChain => {
                // TODO: this means another chain needs to be checked
            }
            _ => panic!("expected utxo valid or disqualified {}", new_selected),
        }
        Ok(())
    }

    fn maybe_update_pruning_point_and_candidate(self: &Arc<Self>) {
        if self
            .is_updating_pruning_point_or_candidate
            .compare_exchange(false, true, atomic::Ordering::Acquire, atomic::Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        {
            let pruning_read_guard = self.pruning_store.upgradable_read();
            let current_pp = pruning_read_guard.pruning_point().unwrap();
            let current_pp_candidate = pruning_read_guard.pruning_point_candidate().unwrap();
            let virtual_gd = self.ghostdag_store.get_compact_data(VIRTUAL).unwrap();
            let (new_pruning_point, new_candidate) = self.pruning_manager.next_pruning_point_and_candidate_by_block_hash(
                virtual_gd,
                None,
                current_pp_candidate,
                current_pp,
            );

            if new_pruning_point != current_pp {
                let mut batch = WriteBatch::default();
                let new_pp_index = pruning_read_guard.pruning_point_index().unwrap() + 1;
                let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
                write_guard.set_batch(&mut batch, new_pruning_point, new_candidate, new_pp_index).unwrap();
                self.past_pruning_points_store.insert_batch(&mut batch, new_pp_index, new_pruning_point).unwrap();
                self.db.write(batch).unwrap();
                // TODO: Move PP UTXO etc
            } else if new_candidate != current_pp_candidate {
                let pp_index = pruning_read_guard.pruning_point_index().unwrap();
                let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
                write_guard.set(new_pruning_point, new_candidate, pp_index).unwrap();
            }
        }

        self.is_updating_pruning_point_or_candidate.store(false, atomic::Ordering::Release);
    }

    pub fn process_genesis_if_needed(self: &Arc<Self>) {
        // TODO: multiset store
        let status = self.statuses_store.read().get(self.genesis_hash).unwrap();
        match status {
            StatusUTXOPendingVerification => {
                self.past_pruning_points_store.insert(0, self.genesis_hash).unwrap();
                self.statuses_store.write().set(self.genesis_hash, StatusUTXOValid).unwrap();
            }
            _ => panic!("unexpected genesis status {:?}", status),
        }
    }
}

/// TEMP: initial struct for holding complete virtual state in memory
struct VirtualState {
    utxo_set: UtxoCollection,           // TEMP: represents the utxo set of virtual selected parent
    utxo_diffs: BlockHashMap<UtxoDiff>, // Holds diff of this block from selected parent
    virtual_diff: UtxoDiff,
    virtual_parents: Vec<Hash>,
    ghostdag_data: Arc<GhostdagData>,
    multiset_hashes: BlockHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash, initial_ghostdag_data: Arc<GhostdagData>) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            virtual_diff: UtxoDiff::default(), // Virtual diff is initially empty since genesis receives no reward
            virtual_parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            multiset_hashes: BlockHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
