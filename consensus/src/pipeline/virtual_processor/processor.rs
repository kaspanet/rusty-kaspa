use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            block_transactions::{BlockTransactionsStoreReader, DbBlockTransactionsStore},
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
};
use consensus_core::{
    block::Block,
    blockhash,
    muhash::MuHashExtensions,
    tx::PopulatedTransaction,
    utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
    DomainHashMap, DomainHashSet,
};
use crossbeam_channel::Receiver;
use hashes::Hash;
use itertools::Itertools;
use kaspa_core::trace;
use muhash::MuHash;
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
    pub(super) headers_store: Arc<DbHeadersStore>,
    pub(super) block_transactions_store: Arc<DbBlockTransactionsStore>,

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
}

impl VirtualStateProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>,
        params: &Params,
        db: Arc<DB>,
        statuses_store: Arc<RwLock<DbStatusesStore>>,
        headers_store: Arc<DbHeadersStore>,
        block_transactions_store: Arc<DbBlockTransactionsStore>,
        ghostdag_manager: DbGhostdagManager,
        reachability_service: MTReachabilityService<DbReachabilityStore>,
    ) -> Self {
        Self {
            receiver,
            db,
            statuses_store,
            headers_store,
            block_transactions_store,
            ghostdag_manager,
            reachability_service,
            genesis_hash: params.genesis_hash,
            max_block_parents: params.max_block_parents,
            mergeset_size_limit: params.mergeset_size_limit,
        }
    }

    pub fn worker(self: &Arc<VirtualStateProcessor>) {
        let mut state = VirtualState::new(self.genesis_hash);
        while let Ok(task) = self.receiver.recv() {
            match task {
                BlockTask::Exit => break,
                BlockTask::Process(block, result_transmitters) => {
                    let res = self.resolve_virtual(&block, &mut state);
                    for transmitter in result_transmitters {
                        // We don't care if receivers were dropped
                        let _ = transmitter.send(res.clone());
                    }
                }
            };
        }
    }

    fn resolve_virtual(
        self: &Arc<VirtualStateProcessor>,
        block: &Arc<Block>,
        state: &mut VirtualState,
    ) -> BlockProcessResult<BlockStatus> {
        // TEMP: assert only coinbase
        // assert_eq!(block.transactions.len(), 1);
        // assert!(block.transactions[0].is_coinbase());
        // assert_eq!(self.statuses_store.read().get(block.header.hash).unwrap(), BlockStatus::StatusUTXOPendingVerification);

        let status = self.statuses_store.read().get(block.header.hash).unwrap();
        match status {
            StatusUTXOPendingVerification => {} // Proceed to resolve virtual
            StatusUTXOValid | StatusDisqualifiedFromChain => return Ok(status),
            _ => panic!("unexpected block status {:?}", status),
        }

        // Update tips
        let parents_set = DomainHashSet::from_iter(block.header.direct_parents().iter().cloned());
        state.tips.retain(|t| !parents_set.contains(t));
        state.tips.push(block.header.hash);

        // TEMP: using all tips as virtual parents
        let virtual_ghostdag_data = self.ghostdag_manager.ghostdag(&state.tips);

        if virtual_ghostdag_data.selected_parent != state.selected_tip {
            // Handle the UTXO state change

            // TODO: test finality

            let prev_selected = state.selected_tip;
            let new_selected = virtual_ghostdag_data.selected_parent;

            if new_selected != block.header.hash {
                trace!("{:?}, {}, {}, {}", state.tips, state.selected_tip, new_selected, block.header.hash);
            }

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

                let utxo_diff = state.utxo_diffs.get(&chain_hash).unwrap();
                accumulated_diff
                    .with_diff_in_place(&utxo_diff.reversed()) // Reverse
                    .unwrap();
            }

            for (selected_parent, chain_hash) in
                self.reachability_service.forward_chain_iterator(split_point, new_selected, true).map(|r| r.unwrap()).tuple_windows()
            {
                match state.utxo_diffs.entry(chain_hash) {
                    Occupied(e) => {
                        let utxo_diff = e.get();
                        accumulated_diff.with_diff_in_place(utxo_diff).unwrap();

                        // Temp logic
                        assert!(state.multiset_hashes.contains_key(&chain_hash));
                    }
                    Vacant(e) => {
                        if self.statuses_store.read().get(selected_parent).unwrap() == BlockStatus::StatusDisqualifiedFromChain {
                            self.statuses_store.write().set(chain_hash, BlockStatus::StatusDisqualifiedFromChain).unwrap();
                            continue; // TODO: optimize
                        }

                        let mut utxo_diff = UtxoDiff::default();
                        let chain_block_header = self.headers_store.get_header(chain_hash).unwrap();

                        // Temp logic
                        assert!(!state.multiset_hashes.contains_key(&chain_hash));
                        let mut multiset_hash = state.multiset_hashes.get(&selected_parent).unwrap().clone();
                        let selected_parent_transactions = self.block_transactions_store.get(selected_parent).unwrap();
                        let populated_coinbase = PopulatedTransaction::new_without_inputs(&selected_parent_transactions[0]);

                        // TODO: prefill and populate UTXO entry data for all mergeset
                        utxo_diff.add_transaction(&populated_coinbase, chain_block_header.daa_score).unwrap();
                        multiset_hash.add_transaction(&populated_coinbase, chain_block_header.daa_score);

                        accumulated_diff.with_diff_in_place(&utxo_diff).unwrap();
                        e.insert(utxo_diff);

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
                            // trace!("correct commitment: {}, {}, {}", selected_parent, chain_hash, expected_commitment);
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
                }
                BlockStatus::StatusDisqualifiedFromChain => {
                    // TODO: this means another chain needs to be checked
                }
                _ => panic!("expected utxo valid or disqualified"),
            };
            Ok(self.statuses_store.read().get(block.hash()).unwrap())
        } else {
            Ok(BlockStatus::StatusUTXOPendingVerification)
        }
    }

    pub fn process_genesis_if_needed(self: &Arc<VirtualStateProcessor>) {
        // TODO: multiset store
    }
}

/// TEMP: initial struct for holding complete virtual state in memory
struct VirtualState {
    utxo_set: UtxoCollection,            // TEMP: represents the utxo set of virtual selected parent
    utxo_diffs: DomainHashMap<UtxoDiff>, // Holds diff of this block from selected parent
    tips: Vec<Hash>,
    selected_tip: Hash,
    multiset_hashes: DomainHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            tips: vec![genesis_hash],
            selected_tip: genesis_hash,
            multiset_hashes: DomainHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
