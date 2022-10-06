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
    tx::{PopulatedTransaction, Transaction},
    utxo::{
        utxo_collection::UtxoCollection,
        utxo_collection::UtxoCollectionExtensions,
        utxo_diff::UtxoDiff,
        utxo_view::{self, UtxoView},
    },
    BlockHashMap, BlockHashSet,
};
use crossbeam_channel::Receiver;
use hashes::Hash;
use itertools::Itertools;
use kaspa_core::trace;
use muhash::MuHash;
use parking_lot::RwLock;
use rayon::prelude::*;
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
            state.tips.retain(|t| !parents_set.contains(t));
            state.tips.push(block.header.hash);
        }

        // TEMP: using all tips as virtual parents
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&state.tips);

        if virtual_ghostdag_data.selected_parent != state.selected_tip {
            // Handle the UTXO state change

            // TODO: test finality

            let prev_selected = state.selected_tip;
            let new_selected = virtual_ghostdag_data.selected_parent;

            // if new_selected != block.header.hash {
            //     trace!("{:?}, {}, {}, {}", state.tips, state.selected_tip, new_selected, block.header.hash);
            // }

            // TEMP:
            // assert_eq!(new_selected, block.header.hash);

            let mut split_point = blockhash::ORIGIN;
            let mut accumulated_diff = UtxoDiff::default();

            for chain_hash in self.reachability_service.default_backward_chain_iterator(prev_selected) {
                let chain_hash = chain_hash.unwrap();
                if self.reachability_service.is_chain_ancestor_of(chain_hash, new_selected) {
                    split_point = chain_hash;
                    break;
                }

                let mergeset_diff = state.utxo_diffs.get(&chain_hash).unwrap();
                accumulated_diff
                    .with_diff_in_place(&mergeset_diff.reversed()) // Reverse
                    .unwrap();
            }

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
                        if self.statuses_store.read().get(selected_parent).unwrap() == BlockStatus::StatusDisqualifiedFromChain {
                            self.statuses_store.write().set(chain_hash, BlockStatus::StatusDisqualifiedFromChain).unwrap();
                            continue; // TODO: optimize
                        }

                        let mut mergeset_diff = UtxoDiff::default();
                        let chain_block_header = self.headers_store.get_header(chain_hash).unwrap();

                        // Temp logic
                        assert!(!state.multiset_hashes.contains_key(&chain_hash));
                        let mut multiset_hash = state.multiset_hashes.get(&selected_parent).unwrap().clone();

                        let mergeset_data = self.ghostdag_store.get_data(chain_hash).unwrap();

                        let selected_parent_transactions = self.block_transactions_store.get(selected_parent).unwrap();
                        let populated_coinbase = PopulatedTransaction::new_without_inputs(&selected_parent_transactions[0]);

                        mergeset_diff.add_transaction(&populated_coinbase, chain_block_header.daa_score).unwrap();
                        multiset_hash.add_transaction(&populated_coinbase, chain_block_header.daa_score);

                        for merged_block in mergeset_data.consensus_ordered_mergeset(self.ghostdag_store.deref()) {
                            let txs = self.block_transactions_store.get(merged_block).unwrap();

                            // Create a layered view from the base UTXO set + the 2 diff layers
                            let composed_view = utxo_view::compose_two_diff_layers(&state.utxo_set, &accumulated_diff, &mergeset_diff);

                            let valid_populated_txs: Vec<PopulatedTransaction> = txs
                                .par_iter() // We can do this in parallel without complications since block body validation already ensured 
                                            // that all txs in the block are independent 
                                .skip(1) // Skip the coinbase tx. Note we already processed the selected parent coinbase
                                .filter_map(|tx| self.process_transaction(tx, &composed_view, merged_block, chain_hash))
                                .collect();

                            for populated_tx in valid_populated_txs {
                                mergeset_diff.add_transaction(&populated_tx, chain_block_header.daa_score).unwrap();
                                multiset_hash.add_transaction(&populated_tx, chain_block_header.daa_score);
                            }
                        }

                        accumulated_diff.with_diff_in_place(&mergeset_diff).unwrap();
                        e.insert(mergeset_diff);

                        // Verify the header UTXO commitment
                        let expected_commitment = multiset_hash.finalize();
                        let status = if expected_commitment != chain_block_header.utxo_commitment {
                            trace!(
                                "wrong commitment: {}, {}, {}, {}",
                                selected_parent,
                                chain_hash,
                                expected_commitment,
                                chain_block_header.utxo_commitment
                            );
                            BlockStatus::StatusDisqualifiedFromChain
                        } else {
                            trace!("correct commitment: {}, {}, {}", selected_parent, chain_hash, expected_commitment);
                            BlockStatus::StatusUTXOValid
                        };

                        // TODO: batch write
                        self.statuses_store.write().set(chain_hash, status).unwrap();
                        state.multiset_hashes.insert(chain_hash, multiset_hash);
                    }
                }
            }

            match self.statuses_store.read().get(new_selected).unwrap() {
                BlockStatus::StatusUTXOValid => {
                    state.selected_tip = new_selected;

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

    /// Attempts to populate the transaction with UTXO entries and performs all tx validations
    fn process_transaction<'a>(
        self: &Arc<VirtualStateProcessor>,
        transaction: &'a Transaction,
        composed_view: &impl UtxoView,
        merged_block: Hash,
        merging_block: Hash,
    ) -> Option<PopulatedTransaction<'a>> {
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = composed_view.get(&input.previous_outpoint) {
                entries.push(entry.clone());
            } else {
                trace!("missing entry for block {} and outpoint {}", merged_block, input.previous_outpoint);
            }
        }
        if entries.len() < transaction.inputs.len() {
            // Missing inputs
            return None;
        }
        let populated_tx = PopulatedTransaction::new(transaction, entries);
        let res = self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, merging_block);
        // TODO: pass DAA score instead of hash to function above ^^
        match res {
            Ok(fee) => Some(populated_tx), // TODO: collect fee info and verify coinbase transaction of `chain_block`
            Err(tx_rule_error) => {
                trace!("tx rule error {} for block {} and tx {}", tx_rule_error, merged_block, transaction.id());
                None // TODO: add to acceptance data as unaccepted tx
            }
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
    tips: Vec<Hash>,
    selected_tip: Hash,
    multiset_hashes: BlockHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            tips: vec![genesis_hash],
            selected_tip: genesis_hash,
            multiset_hashes: BlockHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
