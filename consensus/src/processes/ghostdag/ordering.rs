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
pub struct SortableBlock<WorkType = BlueWorkType> {
    pub hash: Hash,
    pub blue_work: WorkType, // TODO: Rename to blue_work_or_score
}

impl<WorkType> SortableBlock<WorkType> {
    pub fn new(hash: Hash, blue_work: WorkType) -> Self {
        Self { hash, blue_work }
    }
}

impl<WorkType> PartialEq for SortableBlock<WorkType> {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash
    }
}

impl<WorkType: Ord> PartialOrd for SortableBlock<WorkType> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<WorkType: Ord> Ord for SortableBlock<WorkType> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.blue_work.cmp(&other.blue_work).then_with(|| self.hash.cmp(&other.hash))
    }
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService, V: HeaderStoreReader, const USE_BLUE_WORK: bool>
    GhostdagManager<T, S, U, V, USE_BLUE_WORK>
{
    pub fn sort_blocks(&self, blocks: impl IntoIterator<Item = Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = blocks.into_iter().collect();
        sorted_blocks
            .sort_by_cached_key(|block| SortableBlock { hash: *block, blue_work: self.ghostdag_store.get_blue_work(*block).unwrap() });
        sorted_blocks
    }
}
