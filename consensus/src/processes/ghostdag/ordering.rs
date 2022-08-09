use std::{cmp::Ordering, collections::HashSet};

use hashes::Hash;
use misc::uint256::Uint256;

use crate::model::{
    services::reachability::ReachabilityService,
    stores::{ghostdag::GhostdagStoreReader, relations::RelationsStoreReader},
};

use super::protocol::GhostdagManager;

#[derive(Eq)]
pub struct SortableBlock {
    pub hash: Hash,
    pub blue_work: Uint256,
}

impl PartialEq for SortableBlock {
    fn eq(&self, other: &Self) -> bool {
        self.hash == other.hash && self.blue_work == other.blue_work
    }
}

impl PartialOrd for SortableBlock {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SortableBlock {
    fn cmp(&self, other: &Self) -> Ordering {
        let res = self.blue_work.cmp(&other.blue_work);
        match res {
            Ordering::Equal => self.hash.cmp(&other.hash),
            _ => res,
        }
    }
}

impl<T: GhostdagStoreReader, S: RelationsStoreReader, U: ReachabilityService> GhostdagManager<T, S, U> {
    pub fn sort_blocks(&self, blocks: HashSet<Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<Hash> = Vec::from_iter(blocks.iter().cloned());
        sorted_blocks.sort_by_cached_key(|block| SortableBlock {
            hash: *block,
            blue_work: self
                .ghostdag_store
                .get_blue_work(*block, false)
                .unwrap(),
        });
        sorted_blocks
    }
}
