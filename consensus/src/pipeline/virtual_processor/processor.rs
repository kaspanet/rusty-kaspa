use crate::{
    consensus::{DbGhostdagManager, VirtualStores},
    constants::BLOCK_VERSION,
    errors::RuleError,
    model::{
        services::{
            reachability::{MTReachabilityService, ReachabilityService},
            relations::MTRelationsService,
        },
        stores::{
            acceptance_data::{AcceptanceDataStoreReader, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::BlockWindowCacheStore,
            daa::DbDaaStore,
            depth::DbDepthStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore},
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            utxo_set::DbUtxoSetStore,
            virtual_state::{VirtualState, VirtualStateStore, VirtualStateStoreReader},
            DB,
        },
    },
    params::Params,
    pipeline::{
        deps_manager::BlockProcessingMessage, pruning_processor::processor::PruningProcessingMessage,
        virtual_processor::utxo_validation::UtxoProcessingContext, ProcessingCounters,
    },
    processes::{
        block_depth::BlockDepthManager,
        coinbase::CoinbaseManager,
        ghostdag::ordering::SortableBlock,
        parents_builder::ParentsManager,
        pruning::PruningManager,
        transaction_validator::{errors::TxResult, TransactionValidator},
        traversal_manager::DagTraversalManager,
        window::{FullWindowManager, WindowManager},
    },
};
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    block::{BlockTemplate, MutableBlock},
    blockstatus::BlockStatus::{StatusDisqualifiedFromChain, StatusUTXOValid},
    coinbase::MinerData,
    config::genesis::GenesisBlock,
    header::Header,
    merkle::calc_hash_merkle_root,
    tx::{MutableTransaction, Transaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
    BlockHashSet,
};
use kaspa_consensus_notify::{
    notification::{
        Notification, SinkBlueScoreChangedNotification, UtxosChangedNotification, VirtualChainChangedNotification,
        VirtualDaaScoreChangedNotification,
    },
    root::ConsensusNotificationRoot,
};
use kaspa_core::{debug, info, time::unix_now, trace, warn};
use kaspa_database::prelude::{StoreError, StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_notify::notifier::Notify;

use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use itertools::Itertools;
use kaspa_utils::binary_heap::BinaryHeapExtensions;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::{
    cmp::min,
    collections::{BinaryHeap, VecDeque},
    ops::Deref,
    sync::{atomic::Ordering, Arc},
};

use super::errors::{PruningImportError, PruningImportResult};

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<BlockProcessingMessage>,
    pruning_sender: CrossbeamSender<PruningProcessingMessage>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis: GenesisBlock,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    pub(super) pruning_depth: u64,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) daa_store: Arc<DbDaaStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,
    pub(super) pruning_store: Arc<RwLock<DbPruningStore>>,
    pub(super) past_pruning_points_store: Arc<DbPastPruningPointsStore>,
    pub(super) body_tips_store: Arc<RwLock<DbTipsStore>>,

    // Utxo-related stores
    pub(super) utxo_diffs_store: Arc<DbUtxoDiffsStore>,
    pub(super) utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
    pub(super) acceptance_data_store: Arc<DbAcceptanceDataStore>,
    pub virtual_stores: Arc<RwLock<VirtualStores>>,
    pruning_point_utxo_set_store: Arc<RwLock<DbUtxoSetStore>>,
    // TODO: remove all pub from stores when StoreManager is implemented

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    pub(super) window_manager: FullWindowManager<DbGhostdagStore, BlockWindowCacheStore, DbHeadersStore>,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    pub(super) parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    pub(super) depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,

    pub(crate) notification_root: Arc<ConsensusNotificationRoot>,

    // Counters
    counters: Arc<ProcessingCounters>,
}

impl VirtualStateProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<BlockProcessingMessage>,
        pruning_sender: CrossbeamSender<PruningProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        params: &Params,
        db: Arc<DB>,
        // Stores
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
        headers_store: Arc<DbHeadersStore>,
        daa_store: Arc<DbDaaStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        pruning_store: Arc<RwLock<DbPruningStore>>,
        past_pruning_points_store: Arc<DbPastPruningPointsStore>,
        body_tips_store: Arc<RwLock<DbTipsStore>>,
        // Utxo-related stores
        utxo_diffs_store: Arc<DbUtxoDiffsStore>,
        utxo_multisets_store: Arc<DbUtxoMultisetsStore>,
        acceptance_data_store: Arc<DbAcceptanceDataStore>,
        // Virtual-related stores
        virtual_stores: Arc<RwLock<VirtualStores>>,
        pruning_point_utxo_set_store: Arc<RwLock<DbUtxoSetStore>>,
        // Managers
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        relations_service: MTRelationsService<DbRelationsStore>,
        dag_traversal_manager: DagTraversalManager<DbGhostdagStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
        window_manager: FullWindowManager<DbGhostdagStore, BlockWindowCacheStore, DbHeadersStore>,
        coinbase_manager: CoinbaseManager,
        transaction_validator: TransactionValidator,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
        parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
        depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,
        notification_root: Arc<ConsensusNotificationRoot>,
        counters: Arc<ProcessingCounters>,
    ) -> Self {
        Self {
            receiver,
            pruning_sender,
            thread_pool,

            genesis: params.genesis.clone(),
            max_block_parents: params.max_block_parents,
            difficulty_window_size: params.difficulty_window_size,
            mergeset_size_limit: params.mergeset_size_limit,
            pruning_depth: params.pruning_depth,

            db,
            statuses_store,
            headers_store,
            ghostdag_store,
            daa_store,
            block_transactions_store,
            pruning_store,
            past_pruning_points_store,
            body_tips_store,
            utxo_diffs_store,
            utxo_multisets_store,
            acceptance_data_store,
            virtual_stores,
            pruning_point_utxo_set_store,
            ghostdag_manager,
            reachability_service,
            relations_service,
            dag_traversal_manager,
            window_manager,
            coinbase_manager,
            transaction_validator,
            pruning_manager,
            parents_manager,
            depth_manager,
            notification_root,
            counters,
        }
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(msg) = self.receiver.recv() {
            if msg.is_exit_message() {
                break;
            }

            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info

            let messages: Vec<BlockProcessingMessage> = std::iter::once(msg).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", messages.len());

            self.resolve_virtual();

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    BlockProcessingMessage::Exit => break 'outer,
                    BlockProcessingMessage::Process(task, result_transmitter) => {
                        // We don't care if receivers were dropped
                        let _ = result_transmitter.send(Ok(statuses_read.get(task.block().hash()).unwrap()));
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    fn resolve_virtual(self: &Arc<Self>) {
        let pruning_point = self.pruning_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.upgradable_read();
        let prev_state = virtual_read.state.get().unwrap();
        let finality_point = self.virtual_finality_point(&prev_state.ghostdag_data, pruning_point);
        let tips = self.body_tips_store.read().get().unwrap().iter().copied().collect_vec();
        let prev_sink = prev_state.ghostdag_data.selected_parent;
        let mut accumulated_diff = prev_state.utxo_diff.clone().to_reversed();

        let (new_sink, virtual_parent_candidates) =
            self.sink_search_algorithm(&virtual_read, &mut accumulated_diff, prev_sink, tips, finality_point, pruning_point);
        let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_sink, virtual_parent_candidates, pruning_point);
        assert_eq!(virtual_ghostdag_data.selected_parent, new_sink);

        let sink_multiset = self.utxo_multisets_store.get(new_sink).unwrap();
        let new_virtual_state = self
            .calculate_and_commit_virtual_state(
                virtual_read,
                virtual_parents,
                virtual_ghostdag_data,
                sink_multiset,
                &mut accumulated_diff,
            )
            .expect("all possible rule errors are unexpected here");

        // Update the pruning processor about the virtual state change
        let sink_ghostdag_data = self.ghostdag_store.get_compact_data(new_sink).unwrap();
        self.pruning_sender.send(PruningProcessingMessage::Process { sink_ghostdag_data }).unwrap();

        // Emit notifications
        let accumulated_diff = Arc::new(accumulated_diff);
        let virtual_parents = Arc::new(new_virtual_state.parents.clone());
        let _ = self
            .notification_root
            .notify(Notification::UtxosChanged(UtxosChangedNotification::new(accumulated_diff, virtual_parents)));
        let _ = self
            .notification_root
            .notify(Notification::SinkBlueScoreChanged(SinkBlueScoreChangedNotification::new(sink_ghostdag_data.blue_score)));
        let _ = self
            .notification_root
            .notify(Notification::VirtualDaaScoreChanged(VirtualDaaScoreChangedNotification::new(new_virtual_state.daa_score)));
        let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_sink, new_sink);
        // TODO: Fetch acceptance data only if there's a subscriber for the below notification.
        let added_chain_blocks_acceptance_data =
            chain_path.added.iter().copied().map(|added| self.acceptance_data_store.get(added).unwrap()).collect_vec();
        let _ = self.notification_root.notify(Notification::VirtualChainChanged(VirtualChainChangedNotification::new(
            chain_path.added.into(),
            chain_path.removed.into(),
            Arc::new(added_chain_blocks_acceptance_data),
        )));
    }

    fn virtual_finality_point(&self, virtual_ghostdag_data: &GhostdagData, pruning_point: Hash) -> Hash {
        let finality_point = self.depth_manager.calc_finality_point(virtual_ghostdag_data, pruning_point);
        if self.reachability_service.is_chain_ancestor_of(pruning_point, finality_point) {
            finality_point
        } else {
            // At the beginning of IBD when virtual finality point might be below the pruning point
            // or disagreeing with the pruning point chain, we take the pruning point itself as the finality point
            pruning_point
        }
    }

    /// Calculates the UTXO state of `to` starting from the state of `from`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `from` from virtual.
    /// The function returns the top-most UTXO-valid block on `chain(to)` which is ideally
    /// `to` itself (with the exception of returning `from` if `to` is already known to be UTXO disqualified).
    /// When returning it is guaranteed that `diff` holds the diff of the returned block from virtual
    fn calculate_utxo_state_relatively(&self, stores: &VirtualStores, diff: &mut UtxoDiff, from: Hash, to: Hash) -> Hash {
        // Avoid reorging if disqualified status is already known
        if self.statuses_store.read().get(to).unwrap() == StatusDisqualifiedFromChain {
            return from;
        }

        let mut split_point: Option<Hash> = None;

        // Walk down to the reorg split point
        for current in self.reachability_service.default_backward_chain_iterator(from) {
            if self.reachability_service.is_chain_ancestor_of(current, to) {
                split_point = Some(current);
                break;
            }

            let mergeset_diff = self.utxo_diffs_store.get(current).unwrap();
            // Apply the diff in reverse
            diff.with_diff_in_place(&mergeset_diff.as_reversed()).unwrap();
        }

        let split_point = split_point.expect("chain iterator was expected to reach the reorg split point");
        debug!("VIRTUAL PROCESSOR, found split point: {split_point}");

        // A variable holding the most recent UTXO-valid block on `chain(to)` (note that it's maintained such
        // that 'diff' is always its UTXO diff from virtual)
        let mut diff_point = split_point;

        // Walk back up to the new virtual selected parent candidate
        let mut chain_block_counter = 0;
        for (selected_parent, current) in self.reachability_service.forward_chain_iterator(split_point, to, true).tuple_windows() {
            if selected_parent != diff_point {
                // This indicates that the selected parent is disqualified, propagate up and continue
                self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                continue;
            }

            match self.utxo_diffs_store.get(current) {
                Ok(mergeset_diff) => {
                    diff.with_diff_in_place(mergeset_diff.deref()).unwrap();
                    diff_point = current;
                }
                Err(StoreError::KeyNotFound(_)) => {
                    if self.statuses_store.read().get(current).unwrap() == StatusDisqualifiedFromChain {
                        // Current block is already known to be disqualified
                        continue;
                    }

                    let header = self.headers_store.get_header(current).unwrap();
                    let mergeset_data = self.ghostdag_store.get_data(current).unwrap();
                    let pov_daa_score = header.daa_score;

                    let selected_parent_multiset_hash = self.utxo_multisets_store.get(selected_parent).unwrap();
                    let selected_parent_utxo_view = (&stores.utxo_set).compose(&*diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data.into(), selected_parent_multiset_hash);

                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, pov_daa_score);
                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &header);

                    if let Err(rule_error) = res {
                        info!("Block {} is disqualified from virtual chain: {}", current, rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                    } else {
                        debug!("VIRTUAL PROCESSOR, UTXO validated for {current}");

                        // Accumulate the diff
                        diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        // Update the diff point
                        diff_point = current;
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(current, ctx.mergeset_diff, ctx.multiset_hash, ctx.mergeset_acceptance_data);
                        // Count the number of UTXO-processed chain blocks
                        chain_block_counter += 1;
                    }
                }
                Err(err) => panic!("unexpected error {err}"),
            }
        }
        // Report counters
        self.counters.chain_block_counts.fetch_add(chain_block_counter, Ordering::Relaxed);

        diff_point
    }

    fn commit_utxo_state(&self, current: Hash, mergeset_diff: UtxoDiff, multiset: MuHash, acceptance_data: AcceptanceData) {
        let mut batch = WriteBatch::default();
        self.utxo_diffs_store.insert_batch(&mut batch, current, Arc::new(mergeset_diff)).unwrap();
        self.utxo_multisets_store.insert_batch(&mut batch, current, multiset).unwrap();
        self.acceptance_data_store.insert_batch(&mut batch, current, Arc::new(acceptance_data)).unwrap();
        let write_guard = self.statuses_store.set_batch(&mut batch, current, StatusUTXOValid).unwrap();
        self.db.write(batch).unwrap();
        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(write_guard);
    }

    fn calculate_and_commit_virtual_state(
        &self,
        virtual_read: RwLockUpgradableReadGuard<'_, VirtualStores>,
        virtual_parents: Vec<Hash>,
        virtual_ghostdag_data: GhostdagData,
        selected_parent_multiset: MuHash,
        accumulated_diff: &mut UtxoDiff,
    ) -> Result<Arc<VirtualState>, RuleError> {
        let selected_parent_utxo_view = (&virtual_read.utxo_set).compose(&*accumulated_diff);
        let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset);

        // Calc virtual DAA score, difficulty bits and past median time
        let (window, virtual_daa_score, mergeset_non_daa) =
            self.window_manager.block_window_with_daa_score_and_non_daa_mergeset(&virtual_ghostdag_data)?;
        let virtual_bits = self.window_manager.calculate_difficulty_bits(&virtual_ghostdag_data, &window);
        let virtual_past_median_time = self.window_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

        // Calc virtual UTXO state relative to selected parent
        self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, virtual_daa_score);

        // Update the accumulated diff
        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

        // Build the new virtual state
        let new_virtual_state = Arc::new(VirtualState::new(
            virtual_parents,
            virtual_daa_score,
            virtual_bits,
            virtual_past_median_time,
            ctx.multiset_hash,
            ctx.mergeset_diff,
            ctx.accepted_tx_ids,
            ctx.mergeset_rewards,
            mergeset_non_daa,
            virtual_ghostdag_data,
        ));

        let mut batch = WriteBatch::default();
        let mut virtual_write = RwLockUpgradableReadGuard::upgrade(virtual_read);

        // Apply the accumulated diff to the virtual UTXO set
        virtual_write.utxo_set.write_diff_batch(&mut batch, accumulated_diff).unwrap();

        // Update virtual state
        virtual_write.state.set_batch(&mut batch, new_virtual_state.clone()).unwrap();

        // Flush the batch changes
        self.db.write(batch).unwrap();

        // Calling the drops explicitly after the batch is written in order to avoid possible errors.
        drop(virtual_write);

        Ok(new_virtual_state)
    }

    /// Returns the max number of tips to consider as virtual parents in a single virtual resolve operation
    fn max_virtual_parent_candidates(&self) -> usize {
        // Limit to max_block_parents x 3 candidates. This way we avoid going over thousands of tips when the network isn't healthy.
        // There's no specific reason for a factor of 3, and its not a consensus rule, just an estimation for reducing the amount
        // of candidates considered.
        self.max_block_parents as usize * 3
    }

    /// Searches for the next valid sink block (SINK = Virtual selected parent). The search is performed
    /// in the inclusive past of `tips`.
    /// The provided `diff` is assumed to initially hold the UTXO diff of `prev_sink` from virtual.
    /// The function returns with `diff` being the diff of the new sink from previous virtual.
    /// In addition to the found sink the function also returns a queue of additional virtual
    /// parent candidates ordered in descending blue work order.
    fn sink_search_algorithm(
        &self,
        stores: &VirtualStores,
        diff: &mut UtxoDiff,
        prev_sink: Hash,
        tips: Vec<Hash>,
        finality_point: Hash,
        pruning_point: Hash,
    ) -> (Hash, VecDeque<Hash>) {
        // TODO: tests

        let mut heap = tips
            .into_iter()
            .map(|block| SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() })
            .collect::<BinaryHeap<_>>();

        // The initial diff point is the previous sink
        let mut diff_point = prev_sink;

        // We maintain the following invariant: `heap` is an antichain.
        // It holds at step 0 since tips are an antichain, and remains through the loop
        // since we check that every pushed block is not in the past of current heap
        // (and it can't be in the future by induction)
        loop {
            let candidate = heap.pop().expect("valid sink must exist").hash;
            if self.reachability_service.is_chain_ancestor_of(finality_point, candidate) {
                diff_point = self.calculate_utxo_state_relatively(stores, diff, diff_point, candidate);
                if diff_point == candidate {
                    // This indicates that candidate has valid UTXO state and that `diff` represents its diff from virtual
                    return (candidate, heap.into_sorted_iter().take(self.max_virtual_parent_candidates()).map(|s| s.hash).collect());
                } else {
                    debug!("Block candidate {} has invalid UTXO state and is ignored from Virtual chain.", candidate)
                }
            } else if finality_point != pruning_point {
                // `finality_point == pruning_point` indicates we are at IBD start hence no warning required
                warn!("Finality Violation Detected. Block {} violates finality and is ignored from Virtual chain.", candidate);
            }
            for parent in self.relations_service.get_parents(candidate).unwrap().iter().copied() {
                if !self.reachability_service.is_dag_ancestor_of_any(parent, &mut heap.iter().map(|sb| sb.hash)) {
                    heap.push(SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() });
                }
            }
        }
    }

    /// Picks the virtual parents according to virtual parent selection pruning constrains.
    /// Assumes:
    ///     1. `selected_parent` is a UTXO-valid block
    ///     2. `candidates` are an antichain ordered in descending blue work order
    ///     3. `candidates` do not contain `selected_parent` and `selected_parent.blue work > max(candidates.blue_work)`  
    fn pick_virtual_parents(
        &self,
        selected_parent: Hash,
        mut candidates: VecDeque<Hash>,
        pruning_point: Hash,
    ) -> (Vec<Hash>, GhostdagData) {
        // TODO: tests
        let max_block_parents = self.max_block_parents as usize;

        // Prioritize half the blocks with highest blue work and half with lowest, so the network will merge splits faster.
        if candidates.len() >= max_block_parents {
            let max_additional_parents = max_block_parents - 1; // We already have the selected parent
            let mut j = candidates.len() - 1;
            for i in max_additional_parents / 2..max_additional_parents {
                candidates.swap(i, j);
                j -= 1;
            }
        }

        let mut virtual_parents = Vec::with_capacity(min(max_block_parents, candidates.len() + 1));
        virtual_parents.push(selected_parent);
        let mut mergeset_size = 1; // Count the selected parent

        // Try adding parents as long as mergeset size and number of parents limits are not reached
        while let Some(candidate) = candidates.pop_front() {
            if mergeset_size >= self.mergeset_size_limit || virtual_parents.len() >= max_block_parents {
                break;
            }
            match self.mergeset_increase(&virtual_parents, candidate, self.mergeset_size_limit - mergeset_size) {
                MergesetIncreaseResult::Accepted { increase_size } => {
                    mergeset_size += increase_size;
                    virtual_parents.push(candidate);
                }
                MergesetIncreaseResult::Rejected { new_candidate } => {
                    // If we already have a candidate in the past of new candidate then skip.
                    if self.reachability_service.is_any_dag_ancestor(&mut candidates.iter().copied(), new_candidate) {
                        continue; // TODO: not sure this test is needed if candidates invariant as antichain is kept
                    }
                    // Remove all candidates which are in the future of the new candidate
                    candidates.retain(|&h| !self.reachability_service.is_dag_ancestor_of(new_candidate, h));
                    candidates.push_back(new_candidate);
                }
            }
        }
        assert!(mergeset_size <= self.mergeset_size_limit);
        assert!(virtual_parents.len() <= max_block_parents);
        self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
    }

    fn mergeset_increase(&self, selected_parents: &[Hash], candidate: Hash, budget: u64) -> MergesetIncreaseResult {
        /*
        Algo:
            Traverse past(candidate) \setminus past(selected_parents) and make
            sure the increase in mergeset size is within the available budget
        */

        let candidate_parents = self.relations_service.get_parents(candidate).unwrap();
        let mut queue: VecDeque<_> = candidate_parents.iter().copied().collect();
        let mut visited: BlockHashSet = queue.iter().copied().collect();
        let mut mergeset_increase = 1u64; // Starts with 1 to count for the candidate itself

        while let Some(current) = queue.pop_front() {
            if self.reachability_service.is_dag_ancestor_of_any(current, &mut selected_parents.iter().copied()) {
                continue;
            }
            mergeset_increase += 1;
            if mergeset_increase > budget {
                return MergesetIncreaseResult::Rejected { new_candidate: current };
            }

            let current_parents = self.relations_service.get_parents(current).unwrap();
            for &parent in current_parents.iter() {
                if visited.insert(parent) {
                    queue.push_back(parent);
                }
            }
        }
        MergesetIncreaseResult::Accepted { increase_size: mergeset_increase }
    }

    fn remove_bounded_merge_breaking_parents(
        &self,
        mut virtual_parents: Vec<Hash>,
        current_pruning_point: Hash,
    ) -> (Vec<Hash>, GhostdagData) {
        let mut ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, current_pruning_point);
        let mut kosherizing_blues: Option<Vec<Hash>> = None;
        let mut bad_reds = Vec::new();

        //
        // Note that the code below optimizes for the usual case where there are no merge-bound-violating blocks.
        //

        // Find red blocks violating the merge bound and which are not kosherized by any blue
        for red in ghostdag_data.mergeset_reds.iter().copied() {
            if self.reachability_service.is_dag_ancestor_of(merge_depth_root, red) {
                continue;
            }
            // Lazy load the kosherizing blocks since this case is extremely rare
            if kosherizing_blues.is_none() {
                kosherizing_blues = Some(self.depth_manager.kosherizing_blues(&ghostdag_data, merge_depth_root).collect());
            }
            if !self.reachability_service.is_dag_ancestor_of_any(red, &mut kosherizing_blues.as_ref().unwrap().iter().copied()) {
                bad_reds.push(red);
            }
        }

        if !bad_reds.is_empty() {
            // Remove all parents which lead to merging a bad red
            virtual_parents.retain(|&h| !self.reachability_service.is_any_dag_ancestor(&mut bad_reds.iter().copied(), h));
            // Recompute ghostdag data since parents changed
            ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);
        }

        (virtual_parents, ghostdag_data)
    }

    pub fn validate_mempool_transaction_and_populate(&self, mutable_tx: &mut MutableTransaction) -> TxResult<()> {
        self.transaction_validator.validate_tx_in_isolation(&mutable_tx.tx)?;

        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;
        let virtual_daa_score = virtual_state.daa_score;
        let virtual_past_median_time = virtual_state.past_median_time;

        self.transaction_validator.utxo_free_tx_validation(&mutable_tx.tx, virtual_daa_score, virtual_past_median_time)?;
        self.validate_mempool_transaction_in_utxo_context(mutable_tx, virtual_utxo_view, virtual_daa_score)?;

        Ok(())
    }

    fn validate_block_template_transaction(
        &self,
        tx: &Transaction,
        virtual_state: &VirtualState,
        utxo_view: &impl UtxoView,
    ) -> TxResult<()> {
        // No need to validate the transaction in isolation since we rely on the mining manager to submit transactions
        // which were previously validated through `validate_mempool_transaction_and_populate`, hence we only perform
        // in-context validations
        self.transaction_validator.utxo_free_tx_validation(tx, virtual_state.daa_score, virtual_state.past_median_time)?;
        self.validate_transaction_in_utxo_context(tx, utxo_view, virtual_state.daa_score)?;
        Ok(())
    }

    pub fn build_block_template(&self, miner_data: MinerData, mut txs: Vec<Transaction>) -> Result<BlockTemplate, RuleError> {
        // TODO: tests
        let virtual_read = self.virtual_stores.read();
        let virtual_state = virtual_read.state.get().unwrap();
        let virtual_utxo_view = &virtual_read.utxo_set;

        // Search for invalid transactions. This can happen since the mining manager calling this function is not atomically in sync with virtual state
        let mut invalid_transactions = Vec::new();
        for tx in txs.iter() {
            if let Err(e) = self.validate_block_template_transaction(tx, &virtual_state, virtual_utxo_view) {
                invalid_transactions.push((tx.id(), e))
            }
        }
        if !invalid_transactions.is_empty() {
            return Err(RuleError::InvalidTransactionsInNewBlock(invalid_transactions));
        }
        // At this point we can safely drop the read lock
        drop(virtual_read);

        let pruning_info = self.pruning_store.read().get().unwrap();
        let header_pruning_point =
            self.pruning_manager.expected_header_pruning_point(virtual_state.ghostdag_data.to_compact(), pruning_info);
        let coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                virtual_state.daa_score,
                miner_data.clone(),
                &virtual_state.ghostdag_data,
                &virtual_state.mergeset_rewards,
                &virtual_state.mergeset_non_daa,
            )
            .unwrap();
        txs.insert(0, coinbase.tx);
        let version = BLOCK_VERSION;
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_info.pruning_point, &virtual_state.parents);
        let hash_merkle_root = calc_hash_merkle_root(txs.iter());
        let accepted_id_merkle_root = kaspa_merkle::calc_merkle_root(virtual_state.accepted_tx_ids.iter().copied());
        let utxo_commitment = virtual_state.multiset.clone().finalize();
        // Past median time is the exclusive lower bound for valid block time, so we increase by 1 to get the valid min
        let min_block_time = virtual_state.past_median_time + 1;
        let header = Header::new(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            u64::max(min_block_time, unix_now()),
            virtual_state.bits,
            0,
            virtual_state.daa_score,
            virtual_state.ghostdag_data.blue_work,
            virtual_state.ghostdag_data.blue_score,
            header_pruning_point,
        );
        let selected_parent_timestamp = self.headers_store.get_timestamp(virtual_state.ghostdag_data.selected_parent).unwrap();
        Ok(BlockTemplate::new(MutableBlock::new(header, txs), miner_data, coinbase.has_red_reward, selected_parent_timestamp))
    }

    pub fn init(self: &Arc<Self>) {
        let pp_read_guard = self.pruning_store.upgradable_read();

        // Ensure that some pruning point is registered
        if pp_read_guard.pruning_point().unwrap_option().is_none() {
            self.past_pruning_points_store.insert(0, self.genesis.hash).unwrap_or_exists();
            RwLockUpgradableReadGuard::upgrade(pp_read_guard).set(self.genesis.hash, self.genesis.hash, 0).unwrap();
        }
    }

    pub fn process_genesis(self: &Arc<Self>) {
        // Init virtual and pruning stores
        self.virtual_stores
            .write()
            .state
            .set(Arc::new(VirtualState::from_genesis(&self.genesis, self.ghostdag_manager.ghostdag(&[self.genesis.hash]))))
            .unwrap();
        self.past_pruning_points_store.insert(0, self.genesis.hash).unwrap_or_exists();
        self.pruning_store.write().set(self.genesis.hash, self.genesis.hash, 0).unwrap();

        // Write the UTXO state of genesis
        self.commit_utxo_state(self.genesis.hash, UtxoDiff::default(), MuHash::new(), AcceptanceData::default());
    }

    pub fn import_pruning_point_utxo_set(
        &self,
        new_pruning_point: Hash,
        imported_utxo_multiset: &mut MuHash,
    ) -> PruningImportResult<()> {
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).unwrap();
        let imported_utxo_multiset_hash = imported_utxo_multiset.finalize();
        if imported_utxo_multiset_hash != new_pruning_point_header.utxo_commitment {
            return Err(PruningImportError::ImportedMultisetHashMismatch(
                new_pruning_point_header.utxo_commitment,
                imported_utxo_multiset_hash,
            ));
        }

        {
            // Copy the pruning-point UTXO set into virtual's UTXO set
            let pruning_point_utxo_set = self.pruning_point_utxo_set_store.read();
            let mut virtual_write = self.virtual_stores.write();

            virtual_write.utxo_set.clear().unwrap();
            for chunk in &pruning_point_utxo_set.iterator().map(|iter_result| iter_result.unwrap()).chunks(1000) {
                virtual_write.utxo_set.write_from_iterator_without_cache(chunk).unwrap();
            }
        }

        let virtual_read = self.virtual_stores.upgradable_read();

        // Validate transactions of the pruning point itself
        let new_pruning_point_transactions = self.block_transactions_store.get(new_pruning_point).unwrap();
        let validated_transactions = self.validate_transactions_in_parallel(
            &new_pruning_point_transactions,
            &virtual_read.utxo_set,
            new_pruning_point_header.daa_score,
        );
        if validated_transactions.len() < new_pruning_point_transactions.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(PruningImportError::NewPruningPointTxErrors);
        }

        {
            // Submit partial UTXO state for the pruning point.
            // Note we only have and need the multiset; acceptance data and utxo-diff are irrelevant.
            let mut batch = WriteBatch::default();
            self.utxo_multisets_store.insert_batch(&mut batch, new_pruning_point, imported_utxo_multiset.clone()).unwrap();
            let statuses_write = self.statuses_store.set_batch(&mut batch, new_pruning_point, StatusUTXOValid).unwrap();
            self.db.write(batch).unwrap();
            drop(statuses_write);
        }

        // Calculate the virtual state, treating the pruning point as the only virtual parent
        let virtual_parents = vec![new_pruning_point];
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&virtual_parents);

        self.calculate_and_commit_virtual_state(
            virtual_read,
            virtual_parents,
            virtual_ghostdag_data,
            imported_utxo_multiset.clone(),
            &mut UtxoDiff::default(),
        )?;

        Ok(())
    }
}

enum MergesetIncreaseResult {
    Accepted { increase_size: u64 },
    Rejected { new_candidate: Hash },
}
