use std::{collections::HashMap, sync::Arc};

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
    ArchivalBlock, BlockHashMap, BlockLevel, HashMapCustomHasher,
};
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

    fn get_pruning_window_root(&self, pp_index: u64) -> (Hash, bool) {
        let pp = self.storage.past_pruning_points_store.get(pp_index).unwrap();
        let mut write_guard = self.storage.pruning_window_root_store.write();
        let mut root = write_guard.get(pp).unwrap_option().unwrap_or(pp);
        let mut root_changed = false;

        // TODO: Check minimal index value
        let prev_pp = if pp_index > 0 { self.storage.past_pruning_points_store.get(pp_index - 1).unwrap() } else { self.genesis_hash };
        let prev_pp_blue_work = self.storage.headers_store.get_header(prev_pp).unwrap().blue_work;

        loop {
            if root == prev_pp {
                break;
            }

            let root_header = self.storage.headers_store.get_header(root).unwrap();
            if root_header.direct_parents().iter().any(|parent| !self.storage.block_transactions_store.has(*parent).unwrap()) {
                break;
            }

            // TODO: If the blue work comparison is a bottleneck, we can check it after the loop, and inside the loop check it only if the blue score is smaller.
            if root_header.blue_work <= prev_pp_blue_work {
                root = prev_pp
            } else {
                root = root_header.direct_parents()[0];
            }

            root_changed = true;
        }

        if root_changed {
            write_guard.set(pp, root).unwrap();
        }

        (root, root == prev_pp)
    }

    pub fn get_pruning_window_roots(&self) -> Vec<(u64, Hash)> {
        let pp_index = self.storage.pruning_point_store.read().get().unwrap().index;
        (1..=pp_index)
            .rev()
            .filter_map(|pp_index| {
                let (root, is_full) = self.get_pruning_window_root(pp_index);
                if !is_full {
                    Some((pp_index, root))
                } else {
                    None
                }
            })
            .collect()
    }
}
