use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
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
    pipeline::deps_manager::BlockTask,
    processes::transaction_validator::TransactionValidator,
};
use consensus_core::{
    block::Block,
    blockhash,
    header::Header,
    muhash::MuHashExtensions,
    tx::{PopulatedTransaction, Transaction, ValidatedTransaction},
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

            let mut split_point = blockhash::ORIGIN;
            let mut accumulated_diff = UtxoDiff::default();

            for chain_hash in self.reachability_service.default_backward_chain_iterator(prev_selected) {
                let chain_hash = chain_hash.unwrap();
                if self.reachability_service.is_chain_ancestor_of(chain_hash, new_selected) {
                    split_point = chain_hash;
                    break;
                }

                let mergeset_diff = state.utxo_diffs.get(&chain_hash).unwrap();
                accumulated_diff.with_diff_in_place(&mergeset_diff.reversed()).unwrap();
                // Reversing
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
                        if self.statuses_store.read().get(selected_parent).unwrap() == StatusDisqualifiedFromChain {
                            self.statuses_store.write().set(chain_hash, StatusDisqualifiedFromChain).unwrap();
                            continue; // TODO: optimize
                        }

                        let mut mergeset_diff = UtxoDiff::default();
                        let chain_block_header = self.headers_store.get_header(chain_hash).unwrap();

                        // Temp logic
                        assert!(!state.multiset_hashes.contains_key(&chain_hash));
                        let mut multiset_hash = state.multiset_hashes.get(&selected_parent).unwrap().clone();

                        let mergeset_data = self.ghostdag_store.get_data(chain_hash).unwrap();

                        let selected_parent_transactions = self.block_transactions_store.get(selected_parent).unwrap();
                        let validated_coinbase = ValidatedTransaction::new_coinbase(&selected_parent_transactions[0]);

                        mergeset_diff.add_transaction(&validated_coinbase, chain_block_header.daa_score).unwrap();
                        multiset_hash.add_transaction(&validated_coinbase, chain_block_header.daa_score);

                        let mut accepted_tx_ids = vec![validated_coinbase.id()];
                        let mut mergeset_fees = BlockHashMap::with_capacity(mergeset_data.mergeset_size());

                        for merged_block in mergeset_data.consensus_ordered_mergeset(self.ghostdag_store.deref()) {
                            let txs = self.block_transactions_store.get(merged_block).unwrap();

                            // Create a layered UTXO view from the base UTXO set + the 2 diff layers
                            let composed_view = utxo_view::compose_two_diff_layers(&state.utxo_set, &accumulated_diff, &mergeset_diff);

                            // Validate transactions in current UTXO context
                            let validated_transactions =
                                self.validate_transactions_in_parallel(&txs, &composed_view, merged_block, chain_hash);

                            let mut block_fee = 0u64;
                            for validated_tx in validated_transactions {
                                mergeset_diff.add_transaction(&validated_tx, chain_block_header.daa_score).unwrap();
                                multiset_hash.add_transaction(&validated_tx, chain_block_header.daa_score);
                                accepted_tx_ids.push(validated_tx.id());
                                block_fee += validated_tx.calculated_fee;
                            }
                            mergeset_fees.insert(merged_block, block_fee);
                        }

                        let composed_view = utxo_view::compose_two_diff_layers(&state.utxo_set, &accumulated_diff, &mergeset_diff);
                        let status = self.verify_utxo_validness_requirements(
                            &composed_view,
                            &chain_block_header,
                            &mergeset_data,
                            &mut multiset_hash,
                            accepted_tx_ids,
                            mergeset_fees,
                        );

                        if status == StatusUTXOValid {
                            accumulated_diff.with_diff_in_place(&mergeset_diff).unwrap();
                            e.insert(mergeset_diff);
                            state.multiset_hashes.insert(chain_hash, multiset_hash);
                        }

                        // TODO: batch write
                        self.statuses_store.write().set(chain_hash, status).unwrap();
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

    /// Verify that the current block fully respects its own UTXO view. We define a block as
    /// UTXO valid if all the following conditions hold:
    ///     1. The block header includes the expected `utxo_commitment`.
    ///     2. The block header includes the expected `accepted_id_merkle_root`.
    ///     3. The coinbase transaction rewards the mergeset blocks correctly.
    ///     4. All block transactions are valid against its own UTXO view.
    fn verify_utxo_validness_requirements<V: UtxoView + Sync>(
        self: &Arc<VirtualStateProcessor>,
        utxo_view: &V,
        header: &Header,
        mergeset_data: &GhostdagData,
        multiset_hash: &mut MuHash,
        mut accepted_tx_ids: Vec<Hash>,
        mergeset_fees: BlockHashMap<u64>,
    ) -> BlockStatus {
        // Verify header UTXO commitment
        let expected_commitment = multiset_hash.finalize();
        if expected_commitment != header.utxo_commitment {
            trace!("wrong commitment: {}, {}, {}", header.hash, expected_commitment, header.utxo_commitment);
            return StatusDisqualifiedFromChain;
        } else {
            trace!("correct commitment: {}, {}", header.hash, expected_commitment);
        }

        // Verify header accepted_id_merkle_root
        accepted_tx_ids.sort();
        let expected_accepted_id_merkle_root = merkle::calc_merkle_root(accepted_tx_ids.iter().copied());
        if expected_accepted_id_merkle_root != header.accepted_id_merkle_root {
            trace!("wrong accepted_id_merkle_root: {}, {}", expected_accepted_id_merkle_root, header.accepted_id_merkle_root);
            return StatusDisqualifiedFromChain;
        }

        let txs = self.block_transactions_store.get(header.hash).unwrap();
        let coinbase = &txs[0];

        // Verify coinbase transaction
        // TODO: verify coinbase using `mergeset_fees`
        // TODO: build expected coinbase

        // Verify all transactions are valid in context (TODO: skip validation when becoming selected parent)
        let validated_transactions = self.validate_transactions_in_parallel(&txs, &utxo_view, header.hash, header.hash);
        if validated_transactions.len() < txs.len() - 1 {
            // Some transactions were invalid
            return StatusDisqualifiedFromChain;
        }

        StatusUTXOValid
    }

    /// Validates transactions against the provided `utxo_view` and returns a vector with all transactions
    /// which passed the validation
    fn validate_transactions_in_parallel<'a, V: UtxoView + Sync>(
        self: &Arc<VirtualStateProcessor>,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        merged_block: Hash,
        merging_block: Hash,
    ) -> Vec<ValidatedTransaction<'a>> {
        txs
            .par_iter() // We can do this in parallel without complications since block body validation already ensured 
                        // that all txs in the block are independent 
            .skip(1) // Skip the coinbase tx. 
            .filter_map(|tx| self.validate_transaction_in_utxo_context(tx, &utxo_view, merged_block, merging_block))
            .collect()
    }

    /// Attempts to populate the transaction with UTXO entries and performs all tx validations
    fn validate_transaction_in_utxo_context<'a>(
        self: &Arc<VirtualStateProcessor>,
        transaction: &'a Transaction,
        utxo_view: &impl UtxoView,
        merged_block: Hash,
        merging_block: Hash,
    ) -> Option<ValidatedTransaction<'a>> {
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = utxo_view.get(&input.previous_outpoint) {
                entries.push(entry.clone());
            } else {
                trace!("missing entry for block {} and outpoint {}", merged_block, input.previous_outpoint);
                break;
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
            Ok(calculated_fee) => Some(populated_tx.to_validated(calculated_fee)), // TODO: collect fee info and verify coinbase transaction of `chain_block`
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
