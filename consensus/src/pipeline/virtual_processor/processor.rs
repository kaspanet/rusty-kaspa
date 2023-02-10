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
            acceptance_data::{AcceptanceData, DbAcceptanceDataStore},
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            block_window_cache::BlockWindowCacheStore,
            daa::DbDaaStore,
            depth::DbDepthStore,
            errors::StoreError,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
            past_pruning_points::{DbPastPruningPointsStore, PastPruningPointsStore, PastPruningPointsStoreReader},
            pruning::{DbPruningStore, PruningStore, PruningStoreReader},
            reachability::DbReachabilityStore,
            relations::{DbRelationsStore, RelationsStoreReader},
            statuses::{DbStatusesStore, StatusesStore, StatusesStoreBatchExtensions, StatusesStoreReader},
            tips::{DbTipsStore, TipsStoreReader},
            utxo_diffs::{DbUtxoDiffsStore, UtxoDiffsStoreReader},
            utxo_multisets::{DbUtxoMultisetsStore, UtxoMultisetsStoreReader},
            utxo_set::{DbUtxoSetStore, UtxoSetStore},
            virtual_state::{VirtualState, VirtualStateStore, VirtualStateStoreReader},
            DB,
        },
    },
    params::Params,
    pipeline::{deps_manager::BlockProcessingMessage, virtual_processor::utxo_validation::UtxoProcessingContext},
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
use consensus_core::{
    block::{BlockTemplate, MutableBlock},
    blockstatus::BlockStatus::{self, StatusDisqualifiedFromChain, StatusUTXOPendingVerification, StatusUTXOValid},
    coinbase::{BlockRewardData, MinerData},
    events::ConsensusEvent,
    header::Header,
    merkle::calc_hash_merkle_root,
    muhash::MuHashExtensions,
    tx::{MutableTransaction, Transaction},
    tx::{PopulatedTransaction, TransactionOutpoint, UtxoEntry, ValidatedTransaction},
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
};
use hashes::Hash;
use kaspa_core::{debug, info, trace};
use muhash::MuHash;

use async_channel::Sender as AsyncSender; // to avoid confusion with crossbeam
use crossbeam_channel::Receiver as CrossbeamReceiver; // to aviod confusion with async_channel
use itertools::Itertools;
use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use rayon::ThreadPool;
use rocksdb::WriteBatch;
use std::{
    cmp::{min, Reverse},
    collections::HashSet,
    collections::VecDeque,
    ops::Deref,
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::errors::{VirtualProcessorError, VirtualProcessorResult};

pub struct VirtualStateProcessor {
    // Channels
    receiver: CrossbeamReceiver<BlockProcessingMessage>,
    consensus_sender: AsyncSender<ConsensusEvent>,

    // Thread pool
    pub(super) thread_pool: Arc<ThreadPool>,

    // DB
    db: Arc<DB>,

    // Config
    pub(super) genesis_hash: Hash,
    pub(super) genesis_bits: u32,
    pub(super) genesis_timestamp: u64,
    pub(super) max_block_parents: u8,
    pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    pub(super) pruning_depth: u64,
    process_genesis: bool,

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
    pub(super) pruning_point_utxo_set_store: Arc<DbUtxoSetStore>,
    // TODO: remove all pub from stores when StoreManager is implemented

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) relations_service: MTRelationsService<DbRelationsStore>,
    pub(super) dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) difficulty_manager: DifficultyManager<DbHeadersStore>,
    pub(super) coinbase_manager: CoinbaseManager,
    pub(super) transaction_validator: TransactionValidator,
    pub(super) past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
    pub(super) pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
    pub(super) parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, DbRelationsStore>,
    pub(super) depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,
}

impl VirtualStateProcessor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        receiver: CrossbeamReceiver<BlockProcessingMessage>,
        thread_pool: Arc<ThreadPool>,
        consensus_sender: AsyncSender<ConsensusEvent>,
        params: &Params,
        process_genesis: bool,
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
        pruning_point_utxo_set_store: Arc<DbUtxoSetStore>,
        // Managers
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        relations_service: MTRelationsService<DbRelationsStore>,
        dag_traversal_manager: DagTraversalManager<DbGhostdagStore, BlockWindowCacheStore>,
        difficulty_manager: DifficultyManager<DbHeadersStore>,
        coinbase_manager: CoinbaseManager,
        transaction_validator: TransactionValidator,
        past_median_time_manager: PastMedianTimeManager<DbHeadersStore, DbGhostdagStore, BlockWindowCacheStore>,
        pruning_manager: PruningManager<DbGhostdagStore, DbReachabilityStore, DbHeadersStore, DbPastPruningPointsStore>,
        parents_manager: ParentsManager<DbHeadersStore, DbReachabilityStore, DbRelationsStore>,
        depth_manager: BlockDepthManager<DbDepthStore, DbReachabilityStore, DbGhostdagStore>,
    ) -> Self {
        Self {
            receiver,
            thread_pool,

            consensus_sender,

            genesis_hash: params.genesis_hash,
            genesis_bits: params.genesis_bits,
            genesis_timestamp: params.genesis_timestamp,
            max_block_parents: params.max_block_parents,
            difficulty_window_size: params.difficulty_window_size,
            mergeset_size_limit: params.mergeset_size_limit,
            pruning_depth: params.pruning_depth,
            process_genesis,

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
        }
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
                    BlockProcessingMessage::Process(task, result_transmitters) => {
                        for transmitter in result_transmitters {
                            // We don't care if receivers were dropped
                            let _ = transmitter.send(Ok(statuses_read.get(task.block.hash()).unwrap()));
                        }
                    }
                };
            }
        }
    }

    pub fn resolve_virtual(self: &Arc<Self>) {
        // TODO: check finality violation
        // TODO: handle disqualified chain loop
        // TODO: acceptance data format
        // TODO: refactor this methods into multiple methods

        let pruning_read_guard = self.pruning_store.upgradable_read();
        let pruning_point = pruning_read_guard.pruning_point().unwrap();
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
                        info!("{:?}", rule_error);
                        self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                    } else {
                        // Accumulate
                        accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                        // Commit UTXO data for current chain block
                        self.commit_utxo_state(current, ctx.mergeset_diff, ctx.multiset_hash, AcceptanceData {});
                        // TODO: AcceptanceData
                    }
                }
                Err(err) => panic!("unexpected error {err}"),
            }
        }

        // NOTE: inlining this within the match captures the statuses store lock and should be avoided.
        // TODO: wrap statuses store lock within a service
        let new_selected_status = self.statuses_store.read().get(new_selected).unwrap();
        match new_selected_status {
            BlockStatus::StatusUTXOValid => {
                let (virtual_parents, virtual_ghostdag_data) = self.pick_virtual_parents(new_selected, tips, pruning_point);
                assert_eq!(virtual_ghostdag_data.selected_parent, new_selected);

                // Calc the new virtual UTXO diff
                let selected_parent_multiset_hash = self.utxo_multisets_store.get(virtual_ghostdag_data.selected_parent).unwrap();
                let selected_parent_utxo_view = (&virtual_read.utxo_set).compose(&accumulated_diff);
                let mut ctx = UtxoProcessingContext::new((&virtual_ghostdag_data).into(), selected_parent_multiset_hash);

                // Calc virtual DAA score, difficulty bits and past median time
                let window = self.dag_traversal_manager.block_window(&virtual_ghostdag_data, self.difficulty_window_size);
                let (virtual_daa_score, mergeset_non_daa) = self
                    .difficulty_manager
                    .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_ghostdag_data);
                let virtual_bits = self.difficulty_manager.calculate_difficulty_bits(&window);
                let virtual_past_median_time = self.past_median_time_manager.calc_past_median_time(&virtual_ghostdag_data).0;

                // Calc virtual UTXO state relative to selected parent
                self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, virtual_daa_score);

                // Update the accumulated diff
                accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();

                // Build the new virtual state
                let new_virtual_state = VirtualState::new(
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
                );

                let mut batch = WriteBatch::default();
                let mut virtual_write = RwLockUpgradableReadGuard::upgrade(virtual_read);

                // Apply the accumulated diff to the virtual UTXO set
                virtual_write.utxo_set.write_diff_batch(&mut batch, &accumulated_diff).unwrap();

                // Update virtual state
                virtual_write.state.set_batch(&mut batch, new_virtual_state.clone()).unwrap();

                // Flush the batch changes
                self.db.write(batch).unwrap();
                // Calling the drops explicitly after the batch is written in order to avoid possible errors.
                drop(virtual_write);

                // we try_send on consenus sender since this is without blocking.
                if self.consensus_sender.receiver_count() > 0 {
                    // idea here is to not send or panic when we have dropped receivers, i.e. when event processor is not required, (such as in testing cases).
                    match self.consensus_sender.try_send(ConsensusEvent::VirtualChangeSet(Arc::new(new_virtual_state.into()))) {
                        // see, `consensus::store::model::virtual_state` -> `impl From<VirtualState> for VirtualChangeSetConsensusEvent ` for conversion
                        Ok(_) => (),
                        Err(err) => panic!("event processor unreachable: {err}"),
                    }
                }
            }
            BlockStatus::StatusDisqualifiedFromChain => {
                // TODO: this means another chain needs to be checked
            }
            _ => panic!("expected utxo valid or disqualified {new_selected}"),
        }

        // TODO: Make a separate pruning processor and send to its channel here
        self.advance_pruning_point_and_candidate_if_possible(pruning_read_guard)
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
        let current_pruning_point_bs = self.headers_store.get_blue_score(current_pruning_point).unwrap();
        let expected_pruning_point = if ghostdag_data.blue_score < current_pruning_point_bs + self.pruning_depth {
            // If the pruning point is not in pruning depth, it means we're still in IBD, so we can't look for a more up to date pruning point.
            current_pruning_point
        } else {
            self.pruning_manager.expected_header_pruning_point(ghostdag_data.to_compact(), self.pruning_store.read().get().unwrap())
        };
        debug!("The expected pruning point based on the curent virtual parents is {expected_pruning_point}");
        let merge_depth_root = self.depth_manager.calc_merge_depth_root(&ghostdag_data, expected_pruning_point);
        let mut kosherizing_blues: Option<Vec<Hash>> = None;
        let mut bad_reds = Vec::new();

        //
        // Note that the code below optimizes for the usual case where there are no merge bound violating blocks.
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
        let accepted_id_merkle_root = merkle::calc_merkle_root(virtual_state.accepted_tx_ids.iter().copied());
        let utxo_commitment = virtual_state.multiset.clone().finalize();
        // Past median time is the exclusive lower bound for valid block time, so we increase by 1 to get the valid min
        let min_block_time = virtual_state.past_median_time + 1;
        let now = SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        let header = Header::new(
            version,
            parents_by_level,
            hash_merkle_root,
            accepted_id_merkle_root,
            utxo_commitment,
            u64::max(min_block_time, now),
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

    fn advance_pruning_point_and_candidate_if_possible(
        self: &Arc<Self>,
        pruning_read_guard: RwLockUpgradableReadGuard<DbPruningStore>,
    ) {
        let virtual_sp = self.virtual_stores.read().state.get().unwrap().ghostdag_data.selected_parent;
        if virtual_sp == self.genesis_hash {
            return;
        }

        let ghostdag_data = self.ghostdag_store.get_compact_data(virtual_sp).unwrap();
        let current_pruning_info = pruning_read_guard.get().unwrap();
        let (new_pruning_points, new_candidate) = self.pruning_manager.next_pruning_points_and_candidate_by_ghostdag_data(
            ghostdag_data,
            None,
            current_pruning_info.candidate,
            current_pruning_info.pruning_point,
        );

        if !new_pruning_points.is_empty() {
            let mut batch = WriteBatch::default();
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            for (i, past_pp) in new_pruning_points.iter().copied().enumerate() {
                self.past_pruning_points_store.insert_batch(&mut batch, current_pruning_info.index + i as u64 + 1, past_pp).unwrap();
            }
            let new_pp_index = current_pruning_info.index + new_pruning_points.len() as u64;
            write_guard.set_batch(&mut batch, *new_pruning_points.last().unwrap(), new_candidate, new_pp_index).unwrap();
            self.db.write(batch).unwrap();
            // TODO: Move PP UTXO etc
        } else if new_candidate != current_pruning_info.candidate {
            let mut write_guard = RwLockUpgradableReadGuard::upgrade(pruning_read_guard);
            write_guard.set(current_pruning_info.pruning_point, new_candidate, current_pruning_info.index).unwrap();
        }
    }

    pub fn process_genesis_if_needed(self: &Arc<Self>) {
        if !self.process_genesis {
            return;
        }

        let status = self.statuses_store.read().get(self.genesis_hash).unwrap();
        match status {
            StatusUTXOPendingVerification => {
                let txs = self.block_transactions_store.get(self.genesis_hash).unwrap();
                self.virtual_stores
                    .write()
                    .state
                    .set(VirtualState::from_genesis(
                        self.genesis_hash,
                        self.genesis_bits,
                        self.genesis_timestamp,
                        vec![txs[0].id()],
                        self.ghostdag_manager.ghostdag(&[self.genesis_hash]),
                    ))
                    .unwrap();
                self.commit_utxo_state(self.genesis_hash, UtxoDiff::default(), MuHash::new(), AcceptanceData {});
                match self.past_pruning_points_store.insert(0, self.genesis_hash) {
                    Ok(()) => {}
                    Err(StoreError::KeyAlreadyExists(_)) => {
                        // If already exists, make sure the store was initialized correctly
                        match self.past_pruning_points_store.get(0) {
                            Ok(hash) => assert_eq!(hash, self.genesis_hash, "first pruning point is not genesis"),
                            Err(err) => panic!("unexpected error {err}"),
                        }
                    }
                    Err(err) => panic!("unexpected store error {err}"),
                }
            }
            StatusUTXOValid => {}
            _ => panic!("unexpected genesis status {status:?}"),
        }
    }

    pub fn import_pruning_point_utxo_set(
        &self,
        new_pruning_point: Hash,
        imported_utxo_multiset: &mut MuHash,
    ) -> VirtualProcessorResult<()> {
        info!("Importing the UTXO set of the pruning point {}", new_pruning_point);
        let new_pruning_point_header = self.headers_store.get_header(new_pruning_point).unwrap();
        let imported_utxo_multiset_hash = imported_utxo_multiset.finalize();
        if imported_utxo_multiset_hash != new_pruning_point_header.utxo_commitment {
            return Err(VirtualProcessorError::ImportedMultisetHashMismatch(
                new_pruning_point_header.utxo_commitment,
                imported_utxo_multiset_hash,
            ));
        }

        let new_pruning_point_transactions = self.block_transactions_store.get(new_pruning_point).unwrap();
        let new_pruning_point_daa_score = new_pruning_point_header.daa_score;
        let mut total_fee = 0;
        let mut virtual_multiset = imported_utxo_multiset.clone();
        let virtual_parents = vec![new_pruning_point];
        let virtual_gd = self.ghostdag_manager.ghostdag(&virtual_parents);
        let window = self.dag_traversal_manager.block_window(&virtual_gd, self.difficulty_window_size);
        let (virtual_daa_score, mergeset_non_daa) = self
            .difficulty_manager
            .calc_daa_score_and_non_daa_mergeset_blocks(&mut window.iter().map(|item| item.0.hash), &virtual_gd);

        for tx in new_pruning_point_transactions.iter() {
            let res: VirtualProcessorResult<Vec<_>> = tx
                .inputs
                .iter()
                .map(|input| {
                    if let Some(entry) = self.pruning_point_utxo_set_store.get(&input.previous_outpoint) {
                        Ok(entry)
                    } else {
                        Err(VirtualProcessorError::NewPruningPointTxMissingUTXOEntry(tx.id()))
                    }
                })
                .collect();
            let entries = res?;
            let populated_tx = PopulatedTransaction::new(tx, entries);
            let res = if tx.is_coinbase() {
                Ok(0)
            } else {
                self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, new_pruning_point_daa_score)
            };

            if let Err(e) = res {
                return Err(VirtualProcessorError::NewPruningPointTxError(tx.id(), e));
            } else {
                let tx_fee = res.unwrap();
                total_fee += tx_fee;
                let validated_tx = ValidatedTransaction::new(populated_tx, tx_fee);
                virtual_multiset.add_transaction(&validated_tx, virtual_daa_score);
            }
        }
        self.statuses_store.write().set(new_pruning_point, StatusUTXOValid).unwrap();

        {
            let mut batch = WriteBatch::default();
            self.utxo_multisets_store.insert_batch(&mut batch, new_pruning_point, imported_utxo_multiset.clone()).unwrap();
            self.db.write(batch).unwrap();
        }

        let virtual_bits = self.difficulty_manager.calculate_difficulty_bits(&window);
        let accepted_tx_ids = new_pruning_point_transactions.iter().map(|tx| tx.id()).collect_vec();

        let coinbase_data = self.coinbase_manager.deserialize_coinbase_payload(&new_pruning_point_transactions[0].payload).unwrap();
        let mut mergeset_rewards = BlockHashMap::new();
        mergeset_rewards.insert(
            new_pruning_point,
            BlockRewardData::new(coinbase_data.subsidy, total_fee, coinbase_data.miner_data.script_public_key),
        );

        let new_pp_spent_outpoints: HashSet<TransactionOutpoint> =
            new_pruning_point_transactions.iter().flat_map(|tx| tx.inputs.iter().map(|input| input.previous_outpoint)).collect();
        let mut to_remove_diff = Vec::new();
        for (outpoint, entry) in self.pruning_point_utxo_set_store.iterator().map(|iter_result| iter_result.unwrap()) {
            if new_pp_spent_outpoints.contains(&outpoint) {
                to_remove_diff.push((outpoint, (*entry).clone()));
            }
            // TODO: Write in actual batches
            self.virtual_stores.write().utxo_set.write_many(&[(outpoint, (*entry).clone())]).unwrap();
        }

        let new_pp_added_utxos = new_pruning_point_transactions
            .iter()
            .flat_map(|tx| {
                tx.outputs.iter().enumerate().map(|(index, output)| {
                    (
                        TransactionOutpoint { transaction_id: tx.id(), index: index as u32 },
                        UtxoEntry {
                            amount: output.value,
                            script_public_key: output.script_public_key.clone(),
                            block_daa_score: virtual_daa_score,
                            is_coinbase: tx.is_coinbase(),
                        },
                    )
                })
            })
            .collect_vec();
        self.virtual_stores.write().utxo_set.write_many(&new_pp_added_utxos).unwrap();

        let virtual_past_median_time = self.past_median_time_manager.calc_past_median_time(&virtual_gd).0;
        let new_virtual_state = VirtualState {
            parents: virtual_parents,
            ghostdag_data: virtual_gd,
            daa_score: virtual_daa_score,
            bits: virtual_bits,
            multiset: virtual_multiset,
            utxo_diff: UtxoDiff { add: new_pp_added_utxos.into_iter().collect(), remove: to_remove_diff.into_iter().collect() },
            accepted_tx_ids,
            mergeset_rewards,
            mergeset_non_daa,
            past_median_time: virtual_past_median_time,
        };
        self.virtual_stores.write().state.set(new_virtual_state).unwrap();

        Ok(())
    }
}

enum MergesetIncreaseResult {
    Accepted { increase_size: u64 },
    Rejected { new_candidate: Hash },
}
