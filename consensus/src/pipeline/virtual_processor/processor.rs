use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_transactions::DbBlockTransactionsStore,
            ghostdag::{DbGhostdagStore, GhostdagData, GhostdagStoreReader},
            headers::{DbHeadersStore, HeaderStoreReader},
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
    processes::transaction_validator::TransactionValidator,
};
use consensus_core::{
    block::Block,
    blockhash,
    utxo::{utxo_collection::UtxoCollection, utxo_collection::UtxoCollectionExtensions, utxo_diff::UtxoDiff, utxo_view},
    BlockHashMap, BlockHashSet,
};
use hashes::Hash;
use kaspa_core::trace;
use muhash::MuHash;

use crossbeam_channel::Receiver;
use itertools::Itertools;
use parking_lot::RwLock;
use std::{
    collections::hash_map::Entry::{Occupied, Vacant},
    iter::FromIterator,
    sync::Arc,
};

pub struct VirtualStateProcessor {
    // Channels
    receiver: Receiver<BlockTask>,

    // Config
    pub(super) genesis_hash: Hash,
    // pub(super) timestamp_deviation_tolerance: u64,
    // pub(super) target_time_per_block: u64,
    pub(super) max_block_parents: u8,
    // pub(super) difficulty_window_size: usize,
    pub(super) mergeset_size_limit: u64,
    // pub(super) genesis_bits: u32,

    // DB
    db: Arc<DB>,

    // Stores
    pub(super) statuses_store: Arc<RwLock<DbStatusesStore>>,
    pub(super) ghostdag_store: Arc<DbGhostdagStore>,
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
    pub(super) transaction_validator: TransactionValidator<DbHeadersStore>,
}

impl VirtualStateProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>,
        params: &Params,
        db: Arc<DB>,
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        ghostdag_store: Arc<DbGhostdagStore>,
        headers_store: Arc<DbHeadersStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
        transaction_validator: TransactionValidator<DbHeadersStore>,
    ) -> Self {
        Self {
            receiver,
            db,
            statuses_store,
            headers_store,
            ghostdag_store,
            block_transactions_store,
            ghostdag_manager,
            reachability_service,
            genesis_hash: params.genesis_hash,
            max_block_parents: params.max_block_parents,
            mergeset_size_limit: params.mergeset_size_limit,
            transaction_validator,
        }
    }

    pub fn worker(self: &Arc<VirtualStateProcessor>) {
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
        self: &Arc<VirtualStateProcessor>,
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

        if virtual_ghostdag_data.selected_parent != state.ghostdag_data.selected_parent {
            // Handle the UTXO state change

            // TODO: test finality

            let prev_selected = state.ghostdag_data.selected_parent;
            let new_selected = virtual_ghostdag_data.selected_parent;

            let mut split_point = blockhash::ORIGIN;
            let mut accumulated_diff = UtxoDiff::default();

            // Walk down to the reorg split point
            for chain_hash in self.reachability_service.default_backward_chain_iterator(prev_selected).map(|r| r.unwrap()) {
                if self.reachability_service.is_chain_ancestor_of(chain_hash, new_selected) {
                    split_point = chain_hash;
                    break;
                }

                let mergeset_diff = state.utxo_diffs.get(&chain_hash).unwrap();
                // Apply the diff in reverse
                accumulated_diff.with_diff_in_place(&mergeset_diff.reversed()).unwrap();
            }

            // Walk back up to the new virtual selected parent candidate
            for (selected_parent, current) in
                self.reachability_service.forward_chain_iterator(split_point, new_selected, true).map(|r| r.unwrap()).tuple_windows()
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

                        let mut ctx = UtxoProcessingContext::new(&mergeset_data, selected_parent_multiset_hash);

                        self.calculate_utxo_state(&mut ctx, &selected_parent_utxo_view, pov_daa_score);
                        let res = self.verify_expected_utxo_state(&mut ctx, &selected_parent_utxo_view, &header);

                        if let Err(rule_error) = res {
                            trace!("{:?}", rule_error);
                            self.statuses_store.write().set(current, StatusDisqualifiedFromChain).unwrap();
                        } else {
                            accumulated_diff.with_diff_in_place(&ctx.mergeset_diff).unwrap();
                            e.insert(ctx.mergeset_diff);
                            state.multiset_hashes.insert(current, ctx.multiset_hash);
                            // TODO: batch write
                            self.statuses_store.write().set(current, StatusUTXOValid).unwrap();
                        }
                    }
                }
            }

            match self.statuses_store.read().get(new_selected).unwrap() {
                BlockStatus::StatusUTXOValid => {
                    state.ghostdag_data = virtual_ghostdag_data;

                    // Apply the accumulated diff
                    state.utxo_set.remove_many(&accumulated_diff.remove);
                    state.utxo_set.add_many(&accumulated_diff.add);
                }
                BlockStatus::StatusDisqualifiedFromChain => {
                    // TODO: this means another chain needs to be checked
                }
                _ => panic!("expected utxo valid or disqualified"),
            };
            Ok(())
        } else {
            Ok(())
        }
    }

    pub fn process_genesis_if_needed(self: &Arc<VirtualStateProcessor>) {
        // TODO: multiset store
    }
}

/// TEMP: initial struct for holding complete virtual state in memory
struct VirtualState {
    utxo_set: UtxoCollection,           // TEMP: represents the utxo set of virtual selected parent
    utxo_diffs: BlockHashMap<UtxoDiff>, // Holds diff of this block from selected parent
    virtual_parents: Vec<Hash>,
    ghostdag_data: Arc<GhostdagData>,
    multiset_hashes: BlockHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash, initial_ghostdag_data: Arc<GhostdagData>) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            virtual_parents: vec![genesis_hash],
            ghostdag_data: initial_ghostdag_data,
            multiset_hashes: BlockHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
