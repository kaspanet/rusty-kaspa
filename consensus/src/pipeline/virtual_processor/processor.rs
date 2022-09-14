use crate::{
    consensus::DbGhostdagManager,
    errors::BlockProcessResult,
    model::{
        services::reachability::{MTReachabilityService, ReachabilityService},
        stores::{
            reachability::DbReachabilityStore,
            statuses::{BlockStatus, DbStatusesStore, StatusesStore, StatusesStoreReader},
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
    collections::{
        hash_map::Entry::{Occupied, Vacant},
        HashMap,
    },
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

    // Managers and services
    pub(super) ghostdag_manager: DbGhostdagManager,
    pub(super) reachability_service: MTReachabilityService<DbReachabilityStore>,
}

impl VirtualStateProcessor {
    pub fn new(
        receiver: Receiver<BlockTask>, params: &Params, db: Arc<DB>, statuses_store: Arc<RwLock<DbStatusesStore>>,
        ghostdag_manager: DbGhostdagManager, reachability_service: MTReachabilityService<DbReachabilityStore>,
    ) -> Self {
        Self {
            receiver,
            db,
            statuses_store,
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
        self: &Arc<VirtualStateProcessor>, block: &Arc<Block>, state: &mut VirtualState,
    ) -> BlockProcessResult<BlockStatus> {
        // TEMP: store all blocks in memory
        state
            .blocks
            .insert(block.header.hash, block.clone());

        // TEMP: assert only coinbase
        assert_eq!(block.transactions.len(), 1);
        // assert!(block.transactions[0].is_coinbase());

        assert_eq!(
            self.statuses_store
                .read()
                .get(block.header.hash)
                .unwrap(),
            BlockStatus::StatusUTXOPendingVerification
        );

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
            assert_eq!(new_selected, block.header.hash);

            let mut split_point = blockhash::ORIGIN;
            let mut accumulated_diff = UtxoDiff::default();

            for chain_hash in self
                .reachability_service
                .default_backward_chain_iterator(prev_selected)
            {
                let chain_hash = chain_hash.unwrap();
                if self
                    .reachability_service
                    .is_chain_ancestor_of(chain_hash, new_selected)
                {
                    split_point = chain_hash;
                    break;
                }

                let utxo_diff = state.utxo_diffs.get(&chain_hash).unwrap();
                accumulated_diff
                    .with_diff_in_place(&utxo_diff.reversed()) // Reverse
                    .unwrap();
            }

            for (selected_parent, chain_hash) in self
                .reachability_service
                .forward_chain_iterator(split_point, new_selected, true)
                .map(|r| r.unwrap())
                .tuple_windows()
            {
                match state.utxo_diffs.entry(chain_hash) {
                    Occupied(e) => {
                        let utxo_diff = e.get();
                        accumulated_diff
                            .with_diff_in_place(utxo_diff)
                            .unwrap();

                        // Temp logic
                        assert!(state.multiset_hashes.contains_key(&chain_hash));
                    }
                    Vacant(e) => {
                        if self
                            .statuses_store
                            .read()
                            .get(selected_parent)
                            .unwrap()
                            == BlockStatus::StatusDisqualifiedFromChain
                        {
                            self.statuses_store
                                .write()
                                .set(chain_hash, BlockStatus::StatusDisqualifiedFromChain)
                                .unwrap();
                            continue; // TODO: optimize
                        }

                        let mut utxo_diff = UtxoDiff::default();
                        let chain_block = state.blocks.get(&chain_hash).unwrap();
                        // TODO: prefill and populate UTXO entry data
                        utxo_diff
                            .add_transaction(&chain_block.transactions[0], chain_block.header.daa_score)
                            .unwrap(); // TODO: mergeset + utxo state tests
                        accumulated_diff
                            .with_diff_in_place(&utxo_diff)
                            .unwrap();
                        e.insert(utxo_diff);

                        // Temp logic
                        assert!(!state.multiset_hashes.contains_key(&chain_hash));
                        let mut base_multiset_hash = state
                            .multiset_hashes
                            .get(&selected_parent)
                            .unwrap()
                            .clone();
                        base_multiset_hash.add_transaction(&chain_block.transactions[0], chain_block.header.daa_score);

                        // Verify the header UTXO commitment
                        let status = if base_multiset_hash.finalize() != chain_block.header.utxo_commitment {
                            BlockStatus::StatusDisqualifiedFromChain
                        } else {
                            BlockStatus::StatusUTXOValid
                        };

                        self.statuses_store
                            .write()
                            .set(chain_hash, status)
                            .unwrap();

                        state
                            .multiset_hashes
                            .insert(chain_hash, base_multiset_hash);
                    }
                }
            }

            match self
                .statuses_store
                .read()
                .get(new_selected)
                .unwrap()
            {
                BlockStatus::StatusUTXOValid => {
                    state.selected_tip = new_selected;
                    Ok(BlockStatus::StatusUTXOValid)
                }
                BlockStatus::StatusDisqualifiedFromChain => Ok(BlockStatus::StatusDisqualifiedFromChain),
                _ => panic!("expected utxo valid or disqualified"),
            }
        } else {
            Ok(BlockStatus::StatusUTXOPendingVerification)
        }
    }
}

/// TEMP: initial struct for holding complete virtual state in memory
struct VirtualState {
    utxo_set: UtxoCollection,            // TEMP: represents the utxo set of virtual selected parent
    utxo_diffs: DomainHashMap<UtxoDiff>, // Holds diff of this block from selected parent
    blocks: DomainHashMap<Arc<Block>>,   // TEMP: for now hold all blocks in memory
    tips: Vec<Hash>,
    selected_tip: Hash,
    multiset_hashes: DomainHashMap<MuHash>,
}

impl VirtualState {
    fn new(genesis_hash: Hash) -> Self {
        Self {
            utxo_set: Default::default(),
            utxo_diffs: Default::default(),
            blocks: HashMap::new(),
            tips: vec![genesis_hash],
            selected_tip: genesis_hash,
            multiset_hashes: DomainHashMap::from([(genesis_hash, MuHash::new())]),
        }
    }
}
