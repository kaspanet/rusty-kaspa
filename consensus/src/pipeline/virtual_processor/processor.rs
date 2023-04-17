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
        difficulty::DifficultyManager,
        ghostdag::ordering::SortableBlock,
        parents_builder::ParentsManager,
        past_median_time::PastMedianTimeManager,
        pruning::PruningManager,
        transaction_validator::{errors::TxResult, TransactionValidator},
        traversal_manager::DagTraversalManager,
    },
};
use kaspa_consensus_core::{
    acceptance_data::AcceptanceData,
    block::{BlockTemplate, MutableBlock},
    blockstatus::BlockStatus::{self, StatusDisqualifiedFromChain, StatusUTXOValid},
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
use kaspa_core::{debug, info, time::unix_now, trace};
use kaspa_database::prelude::{StoreError, StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_muhash::MuHash;
use kaspa_notify::notifier::Notify;

use crossbeam_channel::{Receiver as CrossbeamReceiver, Sender as CrossbeamSender};
use itertools::Itertools;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::{
    cmp::{min, Reverse},
    collections::VecDeque,
    ops::Deref,
    sync::{atomic::Ordering, Arc},
    time::{Duration, SystemTime},
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
    pub(super) dag_traversal_manager:
        DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore, DbReachabilityStore, MTRelationsService<DbRelationsStore>>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) past_median_time_manager: PastMedianTimeManager<
        DbHeadersStore,
        DbGhostdagStore,
        BlockWindowCacheStore,
        DbReachabilityStore,
        MTRelationsService<DbRelationsStore>,
    >,
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
        dag_traversal_manager: DagTraversalManager<
            DbGhostdagStore,
            BlockWindowCacheStore,
            DbReachabilityStore,
            MTRelationsService<DbRelationsStore>,
        >,
        difficulty_manager: DifficultyManager<DbHeadersStore>,
        coinbase_manager: CoinbaseManager,
        transaction_validator: TransactionValidator,
        past_median_time_manager: PastMedianTimeManager<
            DbHeadersStore,
            DbGhostdagStore,
            BlockWindowCacheStore,
            DbReachabilityStore,
            MTRelationsService<DbRelationsStore>,
        >,
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
            difficulty_manager,
            coinbase_manager,
            transaction_validator,
            past_median_time_manager,
            pruning_manager,
            parents_manager,
            depth_manager,
            notification_root,
            counters,
        }
    }

    #[inline(always)]
    pub fn notification_root(self: &Arc<Self>) -> Arc<ConsensusNotificationRoot> {
        self.notification_root.clone()
    }

    pub fn worker(self: &Arc<Self>) {
        'outer: while let Ok(first_msg) = self.receiver.recv() {
            // Once a task arrived, collect all pending tasks from the channel.
            // This is done since virtual processing is not a per-block
            // operation, so it benefits from max available info

            let update_virtual =
                if let BlockProcessingMessage::Process(ref task, _) = first_msg { task.update_virtual } else { false };
            let messages: Vec<BlockProcessingMessage> = std::iter::once(first_msg).chain(self.receiver.try_iter()).collect();
            trace!("virtual processor received {} tasks", messages.len());

            if update_virtual {
                self.resolve_virtual();
            }

            let statuses_read = self.statuses_store.read();
            for msg in messages {
                match msg {
                    BlockProcessingMessage::Exit => break 'outer,
                    BlockProcessingMessage::Process(task, result_transmitter) => {
                        // We don't care if receivers were dropped
                        let _ = result_transmitter.send(Ok(statuses_read.get(task.block.hash()).unwrap()));
                    }
                };
            }
        }

        // Pass the exit signal on to the following processor
        self.pruning_sender.send(PruningProcessingMessage::Exit).unwrap();
    }

    pub fn resolve_virtual(self: &Arc<Self>) {
        // TODO: check finality violation
        // TODO: handle disqualified chain loop
        // TODO: refactor this methods into multiple methods

        let pruning_point = self.pruning_store.read().pruning_point().unwrap();
        let virtual_read = self.virtual_stores.upgradable_read();
        let prev_state = virtual_read.state.get().unwrap();
        let tips = self.body_tips_store.read().get().unwrap().iter().copied().collect_vec();
        let new_selected = self.ghostdag_manager.find_selected_parent(&mut tips.iter().copied());
        let prev_selected = prev_state.ghostdag_data.selected_parent;

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
        debug!("resolve_virtual found split point: {split_point}");

        // Walk back up to the new virtual selected parent candidate
        let mut last_log_index = 0;
        let mut last_log_time = SystemTime::now();
        let mut chain_block_counter = 0;
        for (i, (selected_parent, current)) in
            self.reachability_service.forward_chain_iterator(split_point, new_selected, true).tuple_windows().enumerate()
        {
            let now = SystemTime::now();
            let passed = now.duration_since(last_log_time).unwrap();
            if passed > Duration::new(10, 0) {
                info!("UTXO validated {} chain blocks in the last {} seconds (total {})", i - last_log_index, passed.as_secs(), i);
                last_log_time = now;
                last_log_index = i;
            }

            debug!("UTXO validation for {current}");
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
                    let selected_parent_utxo_view = (&virtual_read.utxo_set).compose(&accumulated_diff);

                    let mut ctx = UtxoProcessingContext::new(mergeset_data.into(), selected_parent_multiset_hash);

                    self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, pov_daa_score);
                    let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &header);

                    if let Err(rule_error) = res {
                        info!("Block {} is disqualified from virtual chain: {:?}", current, rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                    } else {
                        // Accumulate
                        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
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

        // NOTE: inlining this within the match captures the statuses store lock and should be avoided.
        // TODO: wrap statuses store lock within a service
        let new_selected_status = self.statuses_store.read().get(new_selected).unwrap();
        match new_selected_status {
            BlockStatus::StatusUTXOValid => {
                let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_selected, tips, pruning_point);
                assert_eq!(virtual_ghostdag_data.selected_parent, new_selected);

                let selected_parent_multiset = self.utxo_multisets_store.get(virtual_ghostdag_data.selected_parent).unwrap();
                let new_virtual_state = self
                    .calculate_and_commit_virtual_state(
                        virtual_read,
                        virtual_parents,
                        virtual_ghostdag_data,
                        selected_parent_multiset,
                        &mut accumulated_diff,
                    )
                    .expect("all possible rule errors are unexpected here");

                // Update the pruning processor about the virtual state change
                let sink_ghostdag_data = self.ghostdag_store.get_compact_data(new_selected).unwrap();
                self.pruning_sender.send(PruningProcessingMessage::Process { sink_ghostdag_data }).unwrap();

                // Emit notifications
                let accumulated_diff = Arc::new(accumulated_diff);
                let virtual_parents = Arc::new(new_virtual_state.parents.clone());
                let _ = self
                    .notification_root
                    .notify(Notification::UtxosChanged(UtxosChangedNotification::new(accumulated_diff, virtual_parents)));
                let _ = self.notification_root().notify(Notification::SinkBlueScoreChanged(SinkBlueScoreChangedNotification::new(
                    new_virtual_state.ghostdag_data.blue_score,
                )));
                let _ = self.notification_root().notify(Notification::VirtualDaaScoreChanged(
                    VirtualDaaScoreChangedNotification::new(new_virtual_state.daa_score),
                ));
                // TODO: As an optimization, calculate the chain path as part of the loop on the chain iterator above.
                let chain_path = self.dag_traversal_manager.calculate_chain_path(prev_selected, new_selected);
                // TODO: Fetch acceptance data only if there's a subscriber for the below notification.
                let added_chain_blocks_acceptance_data =
                    chain_path.added.iter().copied().map(|added| self.acceptance_data_store.get(added).unwrap()).collect_vec();
                let _ = self.notification_root().notify(Notification::VirtualChainChanged(VirtualChainChangedNotification::new(
                    chain_path.added.into(),
                    chain_path.removed.into(),
                    Arc::new(added_chain_blocks_acceptance_data),
                )));
            }
            BlockStatus::StatusDisqualifiedFromChain => {
                // TODO: this means another chain needs to be checked
            }
            _ => panic!("expected utxo valid or disqualified {new_selected}"),
        }
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
        let window = self.dag_traversal_manager.block_window(&virtual_ghostdag_data, self.difficulty_window_size)?;
        let (virtual_daa_score, mergeset_non_daa) = self
            .difficulty_manager
            .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_ghostdag_data);
        let virtual_bits = self.difficulty_manager.calculate_difficulty_bits(&window);
        let virtual_past_median_time = self.past_median_time_manager.calc_past_median_time(&virtual_ghostdag_data)?.0;

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

    /// Picks the virtual parents according to virtual parent selection pruning constrains.
    /// Assumes `selected_parent` is a UTXO-valid block, and that `candidates` are an antichain
    /// containing `selected_parent` s.t. it is the block with highest blue work amongst them.  
    fn pick_virtual_parents(&self, selected_parent: Hash, candidates: Vec<Hash>, pruning_point: Hash) -> (Vec<Hash>, GhostdagData) {
        // TODO: tests
        let max_block_parents = self.max_block_parents as usize;

        // Limit to max_block_parents*3 candidates, that way we don't go over thousands of tips when the network isn't healthy.
        // There's no specific reason for a factor of 3, and its not a consensus rule, just an estimation saying we probably
        // don't want to consider and calculate 3 times the amount of candidates for the set of parents.
        let max_candidates = max_block_parents * 3;
        let mut candidates = candidates
            .into_iter()
            .filter(|&h| h != selected_parent) // Filter the selected parent since we already know it must be included
            .map(|block| Reverse(SortableBlock { hash: block, blue_work: self.ghostdag_store.get_blue_work(block).unwrap() }))
            .k_smallest(max_candidates) // Takes the k largest blocks by blue work in descending order
            .map(|s| s.0.hash)
            .collect::<VecDeque<_>>();
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

        let pruning_point = self
            .pruning_manager
            .expected_header_pruning_point(virtual_state.ghostdag_data.to_compact(), self.pruning_store.read().get().unwrap());
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
        let parents_by_level = self.parents_manager.calc_block_parents(pruning_point, &virtual_state.parents);
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
            pruning_point,
        );
        let selected_parent_timestamp = self.headers_store.get_timestamp(virtual_state.ghostdag_data.selected_parent).unwrap();
        Ok(BlockTemplate::new(MutableBlock::new(header, txs), miner_data, coinbase.has_red_reward, selected_parent_timestamp))
    }

    pub fn init(self: &Arc<Self>) {
        let pp_read_guard = self.pruning_store.upgradable_read();

        // Ensure that some pruning point is registered
        if pp_read_guard.pruning_point().unwrap_option().is_none() {
            self.past_pruning_points_store.insert(0, self.genesis.hash).unwrap_and_ignore_key_already_exists();
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
        self.past_pruning_points_store.insert(0, self.genesis.hash).unwrap_and_ignore_key_already_exists();
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
