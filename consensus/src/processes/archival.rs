use std::sync::Arc;

use kaspa_consensus_core::{block::Block, BlockLevel};
use kaspa_database::prelude::StoreResultExtensions;
use kaspa_hashes::Hash;
use kaspa_pow::calc_block_level;
use thiserror::Error;

use crate::{
    consensus::storage::ConsensusStorage,
    model::stores::{
        block_transactions::BlockTransactionsStore,
        headers::{HeaderStore, HeaderStoreReader},
        past_pruning_points::PastPruningPointsStoreReader,
        pruning::PruningStoreReader,
        pruning_window_root::{PruningWindowRootStore, PruningWindowRootStoreReader},
    },
};

#[derive(Error, Debug)]
pub enum ArchivalError {
    #[error("child {0} was not found")]
    ChildNotFound(Hash),

    #[error("{0} is not a parent of {1}")]
    NotParentOf(Hash, Hash),
}
type ArchivalResult<T> = std::result::Result<T, ArchivalError>;

#[derive(Clone)]
pub struct ArchivalManager {
    max_block_level: BlockLevel,
    genesis_hash: Hash,

    storage: Arc<ConsensusStorage>,
}

impl ArchivalManager {
    pub fn new(max_block_level: BlockLevel, genesis_hash: Hash, storage: Arc<ConsensusStorage>) -> Self {
        Self { storage, max_block_level, genesis_hash }
    }

    pub fn add_archival_block(&self, block: Block, child: Hash) -> ArchivalResult<()> {
        let block_hash = block.hash();
        if self.storage.block_transactions_store.has(block_hash).unwrap() {
            return Ok(());
        }

        let _ = self
            .storage
            .headers_store
            .get_header(child)
            .unwrap_option()
            .ok_or(ArchivalError::ChildNotFound(child))?
            .direct_parents()
            .iter()
            .copied()
            .find(|parent| *parent == block_hash)
            .ok_or(ArchivalError::NotParentOf(block_hash, child))?;

        let block_level = calc_block_level(&block.header, self.max_block_level);

        // TODO: Check locks etc
        if !self.storage.headers_store.has(block_hash).unwrap() {
            self.storage.headers_store.insert(block_hash, block.header, block_level).unwrap();
        }

        // TODO: Check locks etc
        if !self.storage.block_transactions_store.has(block_hash).unwrap() {
            self.storage.block_transactions_store.insert(block_hash, block.transactions).unwrap();
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
        let prev_pp_daa_score = self.storage.headers_store.get_daa_score(prev_pp).unwrap();

        loop {
            if root == prev_pp {
                break;
            }

            let root_header = self.storage.headers_store.get_header(root).unwrap();
            if root_header.direct_parents().iter().any(|parent| !self.storage.block_transactions_store.has(*parent).unwrap()) {
                break;
            }
            if root_header.daa_score <= prev_pp_daa_score {
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
        let mut pp_index = self.storage.pruning_point_store.read().get().unwrap().index;
        let mut roots = Vec::with_capacity(pp_index as usize);
        while pp_index > 0 {
            let (root, is_full) = self.get_pruning_window_root(pp_index);
            if !is_full {
                roots.push((pp_index, root));
            }
            pp_index -= 1;
        }

        roots
    }
}
