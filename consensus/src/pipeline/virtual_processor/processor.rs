use crate::{
    consensus::DbGhostdagManager,
    constants::{self, store_names},
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
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
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
use consensus_core::utxo::{utxo_diff::UtxoDiff, utxo_view::UtxoViewComposition};
use hashes::Hash;
use kaspa_core::trace;
use muhash::MuHash;

use crossbeam_channel::Receiver;
use itertools::Itertools;
use kaspa_utils::option::OptionExtensions;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::{ops::Deref, sync::Arc};

pub struct VirtualStateProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis_hash: Hash,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    pruning_depth: u64,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,

    // Utxo-related stores
    pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub(super) virtual_utxo_store: Arc<DbUtxoSetStore>,
    pub(super) virtual_state_store: Arc<RwLock<DbVirtualStateStore>>,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
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
        body_tips_store: Arc<RwLock<DbTipsStore>>,
        // Utxo-related stores
        utxo_diffs_store: Arc<DbUtxoDiffsStore>,
        utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
        acceptance_data_store: Arc<DbAcceptanceDataStore>,
        // Managers
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
        difficulty_manager: DifficultyManager<DbHeadersStore>,
        transaction_validator: TransactionValidator,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    ) -> Self {
        Self {
            receiver,
            thread_pool,

            genesis_hash: params.genesis_hash,
            max_block_parents: params.max_block_parents,
            difficulty_window_size: params.difficulty_window_size,
            mergeset_size_limit: params.mergeset_size_limit,
            pruning_depth: params.pruning_depth,

            db: db.clone(),
            statuses_store,
            headers_store,
            ghostdag_store,
            block_transactions_store,
            pruning_store,
            past_pruning_points_store,
            body_tips_store,
            utxo_diffs_store,
            utxo_multisets_store,
            acceptance_data_store,
            // TODO: build in consensus, decide about locking
            virtual_utxo_store: Arc::new(DbUtxoSetStore::new(
                db.clone(),
                constants::perf::UTXO_CACHE_SIZE,
                store_names::VIRTUAL_UTXO_SET,
            )),
            virtual_state_store: Arc::new(RwLock::new(DbVirtualStateStore::new(db))),

            ghostdag_manager,
            reachability_service,
            dag_traversal_manager,
            difficulty_manager,
            transaction_validator,
            pruning_manager,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(first_task) = self.receiver.recv() {
            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info
            let tasks: Vec<BlockTask> = std::iter::once(first_task).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", tasks.len());

            self.resolve_virtual();

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

    fn resolve_virtual(self: &Arc<Self>) {
        let prev_state = self.virtual_state_store.read().get().unwrap();

        // TODO: pick virtual parents from body tips according to pruning rules
        let virtual_parents = self.body_tips_store.read().get().unwrap().iter().copied().collect_vec();

        // TODO: check finality violation
        // TODO: handle disqualified chain loop
        // TODO: coinbase validation
        // TODO: acceptance data format
        // TODO: refactor this methods into multiple methods

        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);

        let prev_selected = prev_state.ghostdag_data.selected_parent;
        let new_selected = virtual_ghostdag_data.selected_parent;

        let mut split_point: Option<Hash> = None;
        let mut accumulated_diff = prev_state.utxo_diff.clone().to_reversed();

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(prev_selected) {
            if self.reachability_service.is_chain_ancestor_of(current, new_selected) {
                split_point = Some(current);
                break;
            }

            let mergeset_diff = self.utxo_diffs_store.get(current).unwrap();
            // Apply the diff in reverse
            accumulated_diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
        }

        let split_point = split_point.expect("chain iterator was expected to reach the reorg split point");

        // Walk back up to the new virtual selected parent candidate
        for (selected_parent, current) in
            self.reachability_service.forward_chain_iterator(split_point, new_selected, true).tuple_windows()
        {
            match self.utxo_diffs_store.get(current) {
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

                    let selected_parent_multiset_hash = self.utxo_multisets_store.get(selected_parent).unwrap();
                    let selected_parent_utxo_view = self.virtual_utxo_store.as_ref().compose(&accumulated_diff);

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
                let selected_parent_utxo_view = self.virtual_utxo_store.as_ref().compose(&accumulated_diff);
                let mut ctx = UtxoProcessingContext::new(virtual_ghostdag_data.clone(), selected_parent_multiset_hash);

                // Calc virtual DAA score
                let window = self.dag_traversal_manager.block_window(virtual_ghostdag_data.clone(), self.difficulty_window_size);
                let (virtual_daa_score, _) = self
                    .difficulty_manager
                    .calc_daa_score_and_added_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_ghostdag_data);
                self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, virtual_daa_score);

                // Update the accumulated diff
                accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

                // Build the new virtual state
                let new_virtual_state =
                    VirtualState::new(virtual_parents, virtual_ghostdag_data, virtual_daa_score, ctx.multiset_hash, ctx.mergeset_diff);

                let mut batch = WriteBatch::default();

                // Apply the accumulated diff to the virtual UTXO set
                self.virtual_utxo_store.write_diff_batch(&mut batch, &accumulated_diff).unwrap();

                // Update virtual state
                let mut write_guard = self.virtual_state_store.write();
                write_guard.set_batch(&mut batch, new_virtual_state).unwrap();

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

        self.maybe_update_pruning_point_and_candidate()
    }

    fn commit_utxo_state(self: &Arc<Self>, current: Hash, mergeset_diff: UtxoDiff, multiset: MuHash, acceptance_data: AcceptanceData) {
        let mut batch = WriteBatch::default();
        self.utxo_diffs_store.insert_batch(&mut batch, current, Arc::new(mergeset_diff)).unwrap();
        self.utxo_multisets_store.insert_batch(&mut batch, current, multiset).unwrap();
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
    }

    fn maybe_update_pruning_point_and_candidate(self: &Arc<Self>) {
        let virtual_sp = self.virtual_state_store.read().get().unwrap().ghostdag_data.selected_parent;
        if virtual_sp == self.genesis_hash {
            return;
        }

        let ghostdag_data = self.ghostdag_store.get_compact_data(virtual_sp).unwrap();
        let pruning_read_guard = self.pruning_store.upgradable_read();
        let current_pruning_info = pruning_read_guard.get().unwrap();
        let current_pp_bs = self.ghostdag_store.get_blue_score(current_pruning_info.pruning_point).unwrap();
        let (new_pruning_point, new_candidate) = self.pruning_manager.next_pruning_point_and_candidate_by_block_hash(
            ghostdag_data,
            None,
            current_pruning_info.candidate,
            current_pruning_info.pruning_point,
        );

        if new_pruning_point != current_pruning_info.pruning_point {
            let mut past_pruning_points_to_add = Vec::new();
            for current in self.reachability_service.backward_chain_iterator(virtual_sp, current_pruning_info.pruning_point, false) {
                let current_header = self.headers_store.get_compact_header_data(current).unwrap();
                if current_header.pruning_point == current_pruning_info.pruning_point
                    || current_header.blue_score < current_pp_bs + self.pruning_depth
                {
                    break;
                }

                if !past_pruning_points_to_add.last().has_value_and(|hash| **hash == current_header.pruning_point) {
                    past_pruning_points_to_add.push(current_header.pruning_point);
                }
            }

            let mut batch = WriteBatch::default();
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            for (i, past_pp) in past_pruning_points_to_add.iter().copied().rev().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_pruning_info.index + i as u64 + 1, past_pp).unwrap();
            }
            let new_pp_index = current_pruning_info.index + past_pruning_points_to_add.len() as u64;
            write_guard.set_batch(&mut batch, new_pruning_point, new_candidate, new_pp_index).unwrap();
            self.db.write(batch).unwrap();
            // TODO: Move PP UTXO etc
        } else if new_candidate != current_pruning_info.candidate {
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            write_guard.set(new_pruning_point, new_candidate, current_pruning_info.index).unwrap();
        }
    }

    pub fn process_genesis_if_needed(self: &Arc<Self>) {
        let status = self.statuses_store.read().get(self.genesis_hash).unwrap();
        match status {
            StatusUTXOPendingVerification => {
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
                    Err(err) => panic!("unexpected store error {}", err),
                }
            }
            _ => panic!("unexpected genesis status {:?}", status),
        }
    }
}
