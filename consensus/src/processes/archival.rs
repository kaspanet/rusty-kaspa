use std::{
    collections::{BinaryHeap, HashMap},
    sync::Arc,
};

use itertools::Itertools;
use kaspa_consensus_core::{
    block::Block,
    blockstatus::BlockStatus,
    config::params::ForkActivation,
    errors::{
        archival::{ArchivalError, ArchivalResult},
        block::RuleError,
    },
    merkle::calc_hash_merkle_root,
    ArchivalBlock, BlockHashMap, BlockHashSet, BlockLevel, BlueWorkType, HashMapCustomHasher,
};
use kaspa_core::info;
use kaspa_database::prelude::{StoreResultEmptyTuple, StoreResultExtensions};
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    ThreadPool,
};

use crate::{
    consensus::storage::ConsensusStorage,
    model::stores::{
        acceptance_data::{AcceptanceDataStore, AcceptanceDataStoreReader},
        block_transactions::BlockTransactionsStore,
        headers::{HeaderStore, HeaderStoreReader},
        past_pruning_points::PastPruningPointsStoreReader,
        pruning::PruningStoreReader,
        pruning_window_root::{PruningWindowRootStore, PruningWindowRootStoreReader},
        statuses::StatusesStore,
    },
};

use super::ghostdag::ordering::SortableBlock;

#[derive(Clone)]
pub struct ArchivalManager {
    max_block_level: BlockLevel,
    genesis_hash: Hash,
    is_archival: bool,
    crescendo_activation: ForkActivation,

    storage: Arc<ConsensusStorage>,
    thread_pool: Arc<ThreadPool>,
}

// TODO: If a node switches back and fortch archival mode, some blocks might be deleted and the root might be incorrect.
// We can handle this by deleting the roots each time we turn off archival mode, or disallowing to move off archival mode
// without resetting the database.
impl ArchivalManager {
    pub fn new(
        max_block_level: BlockLevel,
        genesis_hash: Hash,
        is_archival: bool,
        crescendo_activation: ForkActivation,
        storage: Arc<ConsensusStorage>,
        thread_pool: Arc<ThreadPool>,
    ) -> Self {
        Self { storage, max_block_level, genesis_hash, is_archival, crescendo_activation, thread_pool }
    }

    pub fn add_archival_blocks(&self, blocks: Vec<ArchivalBlock>) -> ArchivalResult<()> {
        if !self.is_archival {
            return Err(ArchivalError::NotArchival);
        }

        let mut validated: HashMap<_, Block, _> = BlockHashMap::new();
        for ArchivalBlock { block, child, acceptance_data: _acceptance_data } in blocks.iter().cloned() {
            if let Some(child) = child {
                let child_header = validated.get(&child).map(|b| Ok::<_, ArchivalError>(b.header.clone())).unwrap_or_else(|| {
                    Ok(self
                        .storage
                        .headers_store
                        .get_header(child)
                        .unwrap_option()
                        .ok_or(ArchivalError::ChildNotFound(child))?
                        .clone())
                })?;

                if !child_header.direct_parents().iter().copied().contains(&block.hash()) {
                    return Err(ArchivalError::NotParentOf(block.hash(), child));
                }

                validated.insert(block.hash(), block);
            } else if !self.storage.headers_store.has(block.hash()).unwrap() {
                return Err(ArchivalError::NoHeader(block.hash()));
            }
        }

        self.thread_pool.install(|| blocks.par_iter().try_for_each(|block| self.add_archival_block(&block)))?;

        let mut status_write = self.storage.statuses_store.write();
        for block in blocks {
            status_write.set(block.block.hash(), BlockStatus::StatusUTXOPendingVerification).unwrap();
        }

        Ok(())
    }

    /// Calculates the accepted_id_merkle_root based on the current DAA score and the accepted tx ids
    /// refer KIP-15 for more details
    ///
    /// Note: since this function is used in archival nodes, we have to keep the daa_score argument
    fn calc_accepted_id_merkle_root(&self, daa_score: u64, mut accepted_tx_ids: Vec<Hash>, selected_parent: Hash) -> Hash {
        if self.crescendo_activation.is_active(daa_score) {
            kaspa_merkle::merkle_hash(
                self.storage.headers_store.get_header(selected_parent).unwrap().accepted_id_merkle_root,
                kaspa_merkle::calc_merkle_root(accepted_tx_ids.iter().copied()),
            )
        } else {
            accepted_tx_ids.sort();
            kaspa_merkle::calc_merkle_root(accepted_tx_ids.iter().copied())
        }
    }

    fn add_archival_block(&self, ArchivalBlock { block, child: _child, acceptance_data }: &ArchivalBlock) -> ArchivalResult<()> {
        let block = block.clone();
        let block_hash = block.hash();

        let block_level = calc_block_level(&block.header, self.max_block_level);
        let crescendo_activated = self.crescendo_activation.is_active(block.header.daa_score);
        let merkle_root = block.header.hash_merkle_root;

        if let Some((selected_parent, acceptance_data)) = acceptance_data {
            if !self.storage.acceptance_data_store.has(block_hash).unwrap() {
                let accepted_tx_ids = acceptance_data
                    .iter()
                    .flat_map(|block_data| block_data.accepted_transactions.iter().map(|tx| tx.transaction_id))
                    .collect_vec();

                let expected_accepted_id_merkle_root =
                    self.calc_accepted_id_merkle_root(block.header.daa_score, accepted_tx_ids, *selected_parent);

                if expected_accepted_id_merkle_root != block.header.accepted_id_merkle_root {
                    return Err(RuleError::BadAcceptedIDMerkleRoot(
                        block_hash,
                        block.header.accepted_id_merkle_root,
                        expected_accepted_id_merkle_root,
                    )
                    .into());
                }

                // Note: Some of the data here is not validated, like in which block the tx was accepted from, or what index, but since we only care about the order, it's ok
                self.storage.acceptance_data_store.insert(block_hash, acceptance_data.clone().into()).unwrap_or_exists();
            }
        }

        // TODO: Check locks
        self.storage.headers_store.insert(block_hash, block.header, block_level).unwrap_or_exists();

        // TODO: Check locks etc
        if !block.transactions.is_empty() && !self.storage.block_transactions_store.has(block_hash).unwrap() {
            let calculated = calc_hash_merkle_root(block.transactions.iter(), crescendo_activated);
            if calculated != merkle_root {
                return Err(RuleError::BadMerkleRoot(merkle_root, calculated).into());
            }

            self.storage.block_transactions_store.insert(block_hash, block.transactions).unwrap_or_exists();
        }

        Ok(())
    }

    // TODO: Don't go deeper in the chain for blocks that don't have acceptance data
    fn get_pruning_window_root(&self, pp_index: u64) -> Vec<Hash> {
        let pp = self.storage.past_pruning_points_store.get(pp_index).unwrap();
        let mut write_guard = self.storage.pruning_window_root_store.write();
        let current_roots = write_guard.get(pp).unwrap_option().unwrap_or(vec![pp]);

        let mut topological_heap: BinaryHeap<_> = Default::default();
        for root in current_roots.iter().copied() {
            topological_heap
                .push(SortableBlock { hash: root, blue_work: self.storage.headers_store.get_header(root).unwrap().blue_work });
        }
        let mut visited = BlockHashSet::new();

        let mut new_roots = BlockHashSet::new();
        let mut new_roots_min_bw = BlueWorkType::MAX;
        loop {
            let Some(SortableBlock { hash: current, .. }) = topological_heap.pop() else {
                break;
            };

            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            // TODO (relaxed): Prevent header double-fetch (not important since it's probably cached)
            let header = self.storage.headers_store.get_header(current).unwrap();
            // TODO: Maybe it's better to check block status?
            if header.direct_parents().iter().any(|parent| !self.storage.block_transactions_store.has(*parent).unwrap()) {
                new_roots.insert(header.hash);
                new_roots_min_bw = new_roots_min_bw.min(header.blue_work);
                continue;
            }

            for parent in header.direct_parents() {
                topological_heap.push(SortableBlock {
                    hash: *parent,
                    blue_work: self.storage.headers_store.get_header(*parent).expect("checked above").blue_work,
                });
            }
        }

        // // We want the new_roots to only have chain blocks with acceptance data, so we find the earliest chain block that has acceptance data,
        // // and remove all chain blocks below it that don't have acceptance data.
        // let mut current = pp;
        // let (chain_root, chain_root_sp) = loop {
        //     if current == self.genesis_hash {
        //         break (current, None);
        //     }

        //     let header = self.storage.headers_store.get_header(current).unwrap();
        //     if header.direct_parents().iter().any(|parent| !self.storage.block_transactions_store.has(*parent).unwrap()) {
        //         break (current, None);
        //     }
        //     let selected_parent = header
        //         .direct_parents()
        //         .iter()
        //         .copied()
        //         .map(|parent| {
        //             let parent_header = self.storage.headers_store.get_header(parent).unwrap();
        //             SortableBlock { hash: parent, blue_work: parent_header.blue_work }
        //         })
        //         .max()
        //         .expect("we checked above if block transactions exist, so we also expect the header to exist")
        //         .hash;
        //     if !self.storage.acceptance_data_store.has(selected_parent).unwrap() {
        //         break (current, Some(selected_parent));
        //     }

        //     current = selected_parent;
        // };

        // // We remove chain blocks without acceptance data from new_roots
        // if let Some(chain_root_sp) = chain_root_sp {
        //     if !new_roots.contains(&chain_root) {
        //         let mut current = chain_root_sp;
        //         loop {
        //             let header = self.storage.headers_store.get_header(current).unwrap();
        //             if header.blue_work < new_roots_min_bw {
        //                 break;
        //             }

        //             new_roots.remove(&current);

        //             let Some(selected_parent) = header
        //                 .direct_parents()
        //                 .iter()
        //                 .copied()
        //                 .map(|parent| {
        //                     self.storage
        //                         .headers_store
        //                         .get_header(parent)
        //                         .unwrap_option()
        //                         .map(|h| SortableBlock { hash: parent, blue_work: h.blue_work })
        //                 })
        //                 .reduce(|a, b| {
        //                     if a.is_none() || b.is_none() {
        //                         return None;
        //                     }
        //                     let a = a.unwrap();
        //                     let b = b.unwrap();
        //                     if a.blue_work > b.blue_work {
        //                         Some(a)
        //                     } else {
        //                         Some(b)
        //                     }
        //                 })
        //                 .and_then(|s| s.map(|s| s.hash))
        //             else {
        //                 break;
        //             };

        //             current = selected_parent;
        //         }
        //     }
        // }

        let new_roots_vec = new_roots.iter().copied().collect_vec();
        if BlockHashSet::from_iter(current_roots.into_iter()) != new_roots {
            write_guard.set(pp, new_roots.into_iter().collect_vec()).unwrap();
        }

        new_roots_vec
    }

    pub fn get_pruning_window_roots(&self) -> Vec<(u64, Vec<Hash>)> {
        let pp_index = self.storage.pruning_point_store.read().get().unwrap().index;
        (1..=pp_index)
            .rev()
            .map(|pp_index| {
                let roots = self.get_pruning_window_root(pp_index);
                (pp_index, roots)
            })
            .collect()
    }

    pub fn check_pruning_window_roots_consistency(&self) {
        if !self.is_archival {
            return;
        }

        let current_pp_index = self.storage.pruning_point_store.read().get().unwrap().index;
        for pp_index in (1..=current_pp_index).rev() {
            let pp = self.storage.past_pruning_points_store.get(pp_index).unwrap();
            info!("Checking consistency of pruning window root for pruning point {} ({})", pp_index, pp);
            let pp_roots = BlockHashSet::from_iter(self.get_pruning_window_root(pp_index).into_iter());
            let mut unvisited_roots = pp_roots.clone();

            let mut topological_heap: BinaryHeap<_> = Default::default();
            let mut visited = BlockHashSet::new();

            topological_heap.push(SortableBlock { hash: pp, blue_work: self.storage.headers_store.get_header(pp).unwrap().blue_work });

            loop {
                let Some(sblock) = topological_heap.pop() else {
                    break;
                };
                let hash = sblock.hash;
                if visited.contains(&hash) {
                    continue;
                }

                visited.insert(hash);

                if unvisited_roots.contains(&hash) {
                    unvisited_roots.remove(&hash);
                    if unvisited_roots.len() == 0 {
                        break;
                    }
                } else {
                    assert!(self.storage.block_transactions_store.has(hash).unwrap());
                }

                let header = self.storage.headers_store.get_header(hash).unwrap();
                for parent in header.direct_parents() {
                    let Some(parent_header) = self.storage.headers_store.get_header(*parent).unwrap_option() else {
                        // Note: when skipping a non existing parent we can't be sure that all the future of root is accessible. For now we only validate that the root is reachable.
                        continue;
                    };
                    topological_heap.push(SortableBlock { hash: *parent, blue_work: parent_header.blue_work });
                }
            }

            assert!(unvisited_roots.is_empty());

            // Check that all pp_roots chain blocks contain acceptance data
            let mut current = pp;
            loop {
                assert!(
                    current == pp || !pp_roots.contains(&current) || self.storage.acceptance_data_store.has(current).unwrap(),
                    "current == pp: {} || !pp_roots.contains(&current): {} || self.storage.acceptance_data_store.has(current).unwrap(): {}",
                    current == pp,
                    !pp_roots.contains(&current),
                    self.storage.acceptance_data_store.has(current).unwrap()
                );
                let header = self.storage.headers_store.get_header(current).unwrap();
                if header.direct_parents().iter().any(|parent| !self.storage.block_transactions_store.has(*parent).unwrap()) {
                    break;
                }
                let Some(selected_parent) = header
                    .direct_parents()
                    .iter()
                    .copied()
                    .map(|parent| {
                        let parent_header = self.storage.headers_store.get_header(parent).unwrap();
                        SortableBlock { hash: parent, blue_work: parent_header.blue_work }
                    })
                    .max()
                    .map(|s| s.hash)
                else {
                    break;
                };

                current = selected_parent;
            }
        }
    }
}
