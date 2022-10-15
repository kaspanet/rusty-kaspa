use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            acceptance_data::{AcceptanceData, DbAcceptanceDataStore},
            block_transactions::DbBlockTransactionsStore,
            block_window_cache::BlockWindowCacheStore,
            errors::StoreError,
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore, PastPruningPointsStoreReader},
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            statuses::{
                BlockStatus::{self, StatusDisqualifiedFromChain, StatusUTXOPendingVerification, StatusUTXOValid},
                DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader,
            },
            utxo_differences::{DbUtxoDifferencesStore, UtxoDifferencesStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            utxo_set::DbUtxoSetStore,
            virtual_state::{DbVirtualStateStore, VirtualState, VirtualStateStore, VirtualStateStoreReader},
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
    utxo::{utxo_diff::UtxoDiff, utxo_view},
    BlockHashSet,
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
    iter::FromIterator,
    ops::Deref,
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

    // Utxo-related stores
    pub(super) utxo_differences_store: Arc<DbUtxoDifferencesStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_utxo_store: Arc<DbUtxoSetStore>,
    pub(super) virtual_state_store: Arc<RwLock<DbVirtualStateStore>>,

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
        params: &Params,
        db: Arc<DB>,
        // Stores
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
        headers_store: Arc<DbHeadersStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        // Utxo-related stores
        utxo_differences_store: Arc<DbUtxoDifferencesStore>,
        utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
        acceptance_data_store: Arc<DbAcceptanceDataStore>,
        // Managers
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

            genesis_hash: params.genesis_hash,
            max_block_parents: params.max_block_parents,
            difficulty_window_size: params.difficulty_window_size,
            mergeset_size_limit: params.mergeset_size_limit,

            db: db.clone(),
            statuses_store,
            headers_store,
            ghostdag_store,
            block_transactions_store,
            pruning_store,
            past_pruning_points_store,
            utxo_differences_store,
            utxo_multisets_store,
            acceptance_data_store,
            virtual_utxo_store: Arc::new(DbUtxoSetStore::new(db.clone(), 10_000, b"virtual-utxo-set")), // TODO: build in consensus, decide about locking
            virtual_state_store: Arc::new(RwLock::new(DbVirtualStateStore::new(db))),

            ghostdag_manager,
            reachability_service,
            dag_traversal_manager,
            difficulty_manager,
            transaction_validator,
            pruning_manager,

            is_updating_pruning_point_or_candidate: false.into(),
        }
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(first_task) = self.receiver.recv() {
            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info
            let tasks: Vec<BlockTask> = std::iter::once(first_task).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", tasks.len());

            let mut blocks = tasks.iter().map_while(|t| if let BlockTask::Process(b, _) = t { Some(b) } else { None });
            self.resolve_virtual(&mut blocks).unwrap();

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

    fn resolve_virtual<'a>(self: &Arc<Self>, blocks: &mut impl Iterator<Item = &'a Arc<Block>>) -> BlockProcessResult<()> {
        let mut state = self.virtual_state_store.read().get().unwrap().as_ref().clone();
        for block in blocks {
            let status = self.statuses_store.read().get(block.header.hash).unwrap();
            match status {
                StatusUTXOPendingVerification | StatusUTXOValid | StatusDisqualifiedFromChain => {}
                _ => panic!("unexpected block status {:?}", status),
            }

            // Update tips
            let parents_set = BlockHashSet::from_iter(block.header.direct_parents().iter().cloned());
            state.parents.retain(|t| !parents_set.contains(t));
            state.parents.push(block.header.hash);
        }

        // TODO: header/body tips stores
        // TODO: check finality violation
        // TODO: coinbase validation
        // TODO: acceptance data format

        // TODO: pick virtual parents from body tips according to pruning rules
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&state.parents);

        let prev_selected = state.ghostdag_data.selected_parent;
        let new_selected = virtual_ghostdag_data.selected_parent;

        let mut split_point = blockhash::ORIGIN;
        let mut accumulated_diff = state.utxo_diff.clone().to_reversed();

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(prev_selected) {
            if self.reachability_service.is_chain_ancestor_of(current, new_selected) {
                split_point = current;
                break;
            }

            let mergeset_diff = self.utxo_differences_store.get(current).unwrap();
            // Apply the diff in reverse
            accumulated_diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
        }

        // Walk back up to the new virtual selected parent candidate
        for (selected_parent, current) in
            self.reachability_service.forward_chain_iterator(split_point, new_selected, true).tuple_windows()
        {
            match self.utxo_differences_store.get(current) {
                Ok(mergeset_diff) => {
                    accumulated_diff.with_diff_in_place(mergeset_diff.deref()).unwrap();
                }
                Err(StoreError::KeyNotFound(_)) => {
                    if self.statuses_store.read().get(selected_parent).unwrap() == StatusDisqualifiedFromChain {
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                        continue; // TODO: optimize
                    }

                    let header = self.headers_store.get_header(current).unwrap();
                    let mergeset_data = self.ghostdag_store.get_data(current).unwrap();
                    let pov_daa_score = header.daa_score;

                    let selected_parent_multiset_hash = self.utxo_multisets_store.get(mergeset_data.selected_parent).unwrap();
                    let selected_parent_utxo_view =
                        utxo_view::compose_one_diff_layer(self.virtual_utxo_store.deref(), &accumulated_diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data, selected_parent_multiset_hash);

                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, pov_daa_score);
                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &header);

                    if let Err(rule_error) = res {
                        trace!("{:?}", rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                    } else {
                        // Accumulate
                        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(current, ctx.mergeset_diff, ctx.multiset_hash, AcceptanceData {});
                        // TODO: AcceptanceData
                    }
                }
                Err(err) => panic!("unexpected error {}", err),
            }
        }

        match self.statuses_store.read().get(new_selected).unwrap() {
            BlockStatus::StatusUTXOValid => {
                // Calc the new virtual UTXO diff
                let selected_parent_multiset_hash = self.utxo_multisets_store.get(virtual_ghostdag_data.selected_parent).unwrap();
                let selected_parent_utxo_view = utxo_view::compose_one_diff_layer(self.virtual_utxo_store.deref(), &accumulated_diff);
                let mut ctx = UtxoProcessingContext::new(virtual_ghostdag_data.clone(), selected_parent_multiset_hash);

                // Calc virtual DAA score
                let window = self.dag_traversal_manager.block_window(virtual_ghostdag_data.clone(), self.difficulty_window_size);
                let (virtual_daa_score, _) = self
                    .difficulty_manager
                    .calc_daa_score_and_added_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_ghostdag_data);
                self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, virtual_daa_score);

                // Update the accumulated diff
                accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

                // Update the new virtual state
                state.ghostdag_data = virtual_ghostdag_data.as_ref().clone();
                state.utxo_diff = ctx.mergeset_diff;

                let mut batch = WriteBatch::default();

                // Apply the accumulated diff to the virtual UTXO set
                self.virtual_utxo_store.write_diff_batch(&mut batch, &accumulated_diff).unwrap();

                // Update virtual state
                let mut write_guard = self.virtual_state_store.write();
                write_guard.set_batch(&mut batch, state).unwrap();

                // Flush the batch changes
                self.db.write(batch).unwrap();

                // Calling the drops explicitly after the batch is written in order to avoid possible errors.
                drop(write_guard);
            }
            BlockStatus::StatusDisqualifiedFromChain => {
                // TODO: this means another chain needs to be checked
            }
            _ => panic!("expected utxo valid or disqualified {}", new_selected),
        }
        Ok(())
    }

    fn commit_utxo_state(self: &Arc<Self>, current: Hash, mergeset_diff: UtxoDiff, multiset: MuHash, acceptance_data: AcceptanceData) {
        let mut batch = WriteBatch::default();
        self.utxo_differences_store.insert_batch(&mut batch, current, Arc::new(mergeset_diff)).unwrap();
        self.utxo_multisets_store.insert_batch(&mut batch, current, multiset).unwrap();
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
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
        let status = self.statuses_store.read().get(self.genesis_hash).unwrap();
        match status {
            StatusUTXOPendingVerification => {
                // TODO: consider using a batch write
                self.virtual_state_store
                    .write()
                    .set(VirtualState::from_genesis(
                        self.genesis_hash,
                        self.ghostdag_manager.ghostdag(&[self.genesis_hash]).as_ref().clone(),
                    ))
                    .unwrap();
                self.commit_utxo_state(self.genesis_hash, UtxoDiff::default(), MuHash::new(), AcceptanceData {});
                match self.past_pruning_points_store.insert(0, self.genesis_hash) {
                    Ok(()) => {}
                    Err(StoreError::KeyAlreadyExists(_)) => {
                        // If already exists, make sure the store was initialized correctly
                        match self.past_pruning_points_store.get(0) {
                            Ok(hash) => assert_eq!(hash, self.genesis_hash, "first pruning point is not genesis"),
                            Err(err) => panic!("unexpected error {}", err),
                        }
                    }
                    Err(err) => panic!("unexpected error {}", err),
                }
            }
            _ => panic!("unexpected genesis status {:?}", status),
        }
    }
}
