use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
            ghostdag::{DbGhostdagStore, GhostdagStoreReader},
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
    pipeline::deps_manager::BlockTask,
    processes::transaction_validator::TransactionValidator,
};
use consensus_core::{
    block::Block,
    blockhash,
    muhash::MuHashExtensions,
    tx::ValidatedTransaction,
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
    ops::Deref,
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
        let mut state = VirtualState::new(self.genesis_hash);
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

        if virtual_ghostdag_data.selected_parent != state.selected_parent {
            // Handle the UTXO state change

            // TODO: test finality

            let prev_selected = state.selected_parent;
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
            for (selected_parent, chain_hash) in
                self.reachability_service.forward_chain_iterator(split_point, new_selected, true).map(|r| r.unwrap()).tuple_windows()
            {
                match state.utxo_diffs.entry(chain_hash) {
                    Occupied(e) => {
                        let mergeset_diff = e.get();
                        accumulated_diff.with_diff_in_place(mergeset_diff).unwrap();

                        // Temp logic
                        assert!(state.multiset_hashes.contains_key(&chain_hash));
                    }
                    Vacant(e) => {
                        if self.statuses_store.read().get(selected_parent).unwrap() == StatusDisqualifiedFromChain {
                            self.statuses_store.write().set(chain_hash, StatusDisqualifiedFromChain).unwrap();
                            continue; // TODO: optimize
                        }

                        let mut mergeset_diff = UtxoDiff::default();
                        let chain_block_header = self.headers_store.get_header(chain_hash).unwrap();
                        let pov_daa_score = chain_block_header.daa_score;

                        // Temp logic
                        assert!(!state.multiset_hashes.contains_key(&chain_hash));
                        let mut multiset_hash = state.multiset_hashes.get(&selected_parent).unwrap().clone();

                        let mergeset_data = self.ghostdag_store.get_data(chain_hash).unwrap();

                        let selected_parent_transactions = self.block_transactions_store.get(selected_parent).unwrap();
                        let validated_coinbase = ValidatedTransaction::new_coinbase(&selected_parent_transactions[0]);

                        mergeset_diff.add_transaction(&validated_coinbase, pov_daa_score).unwrap();
                        multiset_hash.add_transaction(&validated_coinbase, pov_daa_score);

                        let mut accepted_tx_ids = vec![validated_coinbase.id()];
                        let mut mergeset_fees = BlockHashMap::with_capacity(mergeset_data.mergeset_size());

                        for merged_block in mergeset_data.consensus_ordered_mergeset(self.ghostdag_store.deref()) {
                            let txs = self.block_transactions_store.get(merged_block).unwrap();

                            // Create a layered UTXO view from the base UTXO set + the 2 diff layers
                            let composed_view = utxo_view::compose_two_diff_layers(&state.utxo_set, &accumulated_diff, &mergeset_diff);

                            // Validate transactions in current UTXO context
                            let validated_transactions = self.validate_transactions_in_parallel(&txs, &composed_view, pov_daa_score);

                            let mut block_fee = 0u64;
                            for validated_tx in validated_transactions {
                                mergeset_diff.add_transaction(&validated_tx, pov_daa_score).unwrap();
                                multiset_hash.add_transaction(&validated_tx, pov_daa_score);
                                accepted_tx_ids.push(validated_tx.id());
                                block_fee += validated_tx.calculated_fee;
                            }
                            mergeset_fees.insert(merged_block, block_fee);
                        }

                        let composed_view = utxo_view::compose_two_diff_layers(&state.utxo_set, &accumulated_diff, &mergeset_diff);
                        let res = self.verify_utxo_validness_requirements(
                            &composed_view,
                            &chain_block_header,
                            &mergeset_data,
                            &mut multiset_hash,
                            accepted_tx_ids,
                            mergeset_fees,
                        );

                        if let Err(rule_error) = res {
                            trace!("{:?}", rule_error);
                            self.statuses_store.write().set(chain_hash, StatusDisqualifiedFromChain).unwrap();
                        } else {
                            accumulated_diff.with_diff_in_place(&mergeset_diff).unwrap();
                            e.insert(mergeset_diff);
                            state.multiset_hashes.insert(chain_hash, multiset_hash);
                            // TODO: batch write
                            self.statuses_store.write().set(chain_hash, StatusUTXOValid).unwrap();
                        }
                    }
                }
            }

            match self.statuses_store.read().get(new_selected).unwrap() {
                BlockStatus::StatusUTXOValid => {
                    state.selected_parent = new_selected;

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
    selected_parent: Hash,
    multiset_hashes: BlockHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            virtual_parents: vec![genesis_hash],
            selected_parent: genesis_hash,
            multiset_hashes: BlockHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
