use crate::{
    misc::uint256::Uint256,
    model::{
        api::hash::Hash,
        stores::{ghostdag::GhostdagStore, reachability::ReachabilityStore, relations::RelationsStore},
    },
};

use std::{cmp::Ordering, collections::HashSet};

use super::protocol::{GhostdagManager, StoreAccess};

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

impl<T: GhostdagStore, S: RelationsStore, U: ReachabilityStore, V: StoreAccess<T, S, U>> GhostdagManager<T, S, U, V> {
    pub fn sort_blocks(sa: &V, blocks: HashSet<Hash>) -> Vec<Hash> {
        let mut sorted_blocks: Vec<SortableBlock> = blocks
            .iter()
            .map(|block| SortableBlock {
                hash: *block,
                blue_work: sa
                    .ghostdag_store()
                    .get_blue_work(*block, false)
                    .unwrap(),
            })
            .collect();
        sorted_blocks.sort();
        sorted_blocks
            .iter()
            .map(|block| block.hash)
            .collect()
    }
}
