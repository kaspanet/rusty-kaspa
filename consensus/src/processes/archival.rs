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
    ArchivalBlock, BlockHashMap, BlockHashSet, BlockLevel, HashMapCustomHasher,
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
        block_transactions::BlockTransactionsStore,
        headers::{HeaderStore, HeaderStoreReader},
        past_pruning_points::PastPruningPointsStoreReader,
        pruning::PruningStoreReader,
        pruning_window_root::{PruningWindowRootStore, PruningWindowRootStoreReader},
        statuses::{StatusesStore, StatusesStoreReader},
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
        for ArchivalBlock { block, child } in blocks.iter().cloned() {
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

        self.thread_pool.install(|| blocks.par_iter().try_for_each(|block| self.add_archival_block(block.block.clone())))?;

        let mut status_write = self.storage.statuses_store.write();
        for block in blocks {
            status_write.set(block.block.hash(), BlockStatus::StatusUTXOPendingVerification).unwrap();
        }

        Ok(())
    }

    fn add_archival_block(&self, block: Block) -> ArchivalResult<()> {
        let block_hash = block.hash();
        if let Some(status) = self.storage.statuses_store.read().get(block_hash).unwrap_option() {
            if status.has_block_body() {
                return Ok(());
            }
        }

        let block_level = calc_block_level(&block.header, self.max_block_level);
        let crescendo_activated = self.crescendo_activation.is_active(block.header.daa_score);
        let merkle_root = block.header.hash_merkle_root;

        // TODO: Check locks etc
        if !self.storage.headers_store.has(block_hash).unwrap() {
            self.storage.headers_store.insert(block_hash, block.header, block_level).unwrap();
        }

        // TODO: Check locks etc
        if !block.transactions.is_empty() {
            let calculated = calc_hash_merkle_root(block.transactions.iter(), crescendo_activated);
            if calculated != merkle_root {
                return Err(RuleError::BadMerkleRoot(merkle_root, calculated).into());
            }

            self.storage.block_transactions_store.insert(block_hash, block.transactions).unwrap_or_exists();
        }

        Ok(())
    }

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
                continue;
            }

            for parent in header.direct_parents() {
                topological_heap.push(SortableBlock {
                    hash: *parent,
                    blue_work: self.storage.headers_store.get_header(*parent).expect("checked above").blue_work,
                });
            }
        }

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
            let mut unvisited_roots = BlockHashSet::from_iter(self.get_pruning_window_root(pp_index).into_iter());

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
        }
    }
}
