use std::cmp::Ordering;

use kaspa_consensus_core::BlueWorkType;
use kaspa_hashes::Hash;
use serde::{Deserialize, Serialize};

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{ghostdag::GhostdagStoreReader, headers::HeaderStoreReader, relations::RelationsStoreReader},
};

use super::protocol::GhostdagManager;

#[derive(Eq, Clone, Serialize, Deserialize)]
pub struct SortableBlock {
    pub hash: Hash,
    pub blue_work: BlueWorkType,
}

impl SortableBlock {
    pub fn new(hash: Hash, blue_work: BlueWorkType) -> Self {
        Self { hash, blue_work }
    }
}

impl PartialEq for SortableBlock {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl PartialOrd for SortableBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        self.blue_work.cmp(&other.blue_work).then_with(|| self.hash.cmp(&other.hash))
    }
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader> GhostdagManager<T, S, U, V> {
    pub fn sort_blocks(&self, blocks: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = blocks.into_iter().collect();
        sorted_blocks
            .sort_by_cached_key(|block| SortableBlock { hash: *block, blue_work: self.ghostdag_store.get_blue_work(*block).unwrap() });
        sorted_blocks
    }
}
