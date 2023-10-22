use indexmap::{map::Entry::Occupied, IndexMap};
use kaspa_consensus_core::{
    api::{BlockValidationFuture, BlockValidationFutures},
    block::Block,
};
use kaspa_consensusmanager::ConsensusProxy;
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_utils::option::OptionExtensions;
use rand::Rng;
use std::collections::{HashMap, HashSet, VecDeque};

use super::process_queue::ProcessQueue;

struct OrphanBlock {
    /// The actual block
    block: Block,

    /// A set of child orphans loosely maintained such that any block in the
    /// orphan pool which has this block as a direct parent will be in the set, however
    /// items are never removed, so this set might contain evicted hashes as well
    children: HashSet<Hash>,
}

impl OrphanBlock {
    fn new(block: Block, children: HashSet<Hash>) -> Self {
        Self { block, children }
    }
}

pub struct OrphanBlocksPool {
    /// NOTES:
    /// 1. We use IndexMap for cheap random eviction
    /// 2. We avoid the custom block hasher since this pool is pre-validation storage
    orphans: IndexMap<Hash, OrphanBlock>,
    /// Max number of orphans to keep in the pool
    max_orphans: usize,
}

impl OrphanBlocksPool {
    pub fn new(max_orphans: usize) -> Self {
        Self { orphans: IndexMap::with_capacity(max_orphans), max_orphans }
    }

    /// Adds the provided block to the orphan pool
    pub fn add_orphan(&mut self, orphan_block: Block) {
        let orphan_hash = orphan_block.hash();
        if self.orphans.contains_key(&orphan_hash) {
            return;
        }
        if self.orphans.len() == self.max_orphans {
            debug!("Orphan blocks pool size exceeded. Evicting a random orphan block.");
            // Evict a random orphan in order to keep pool size under the limit
            if let Some((evicted, _)) = self.orphans.swap_remove_index(rand::thread_rng().gen_range(0..self.max_orphans)) {
                debug!("Evicted {} from the orphan blocks pool", evicted);
            }
        }
        for parent in orphan_block.header.direct_parents() {
            if let Some(entry) = self.orphans.get_mut(parent) {
                entry.children.insert(orphan_hash);
            }
        }
        self.orphans.insert(orphan_block.hash(), OrphanBlock::new(orphan_block, self.iterate_child_orphans(orphan_hash).collect()));
    }

    /// Returns whether this block is in the orphan pool.
    pub fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans.contains_key(&hash)
    }

    /// Returns the orphan roots of the provided orphan. Orphan roots are ancestors of this orphan which are
    /// not in the orphan pool AND do not exist consensus-wise or are header-only. Given an orphan relayed by
    /// a peer, these blocks should be the next-in-line to be requested from that peer.
    pub async fn get_orphan_roots(&self, consensus: &ConsensusProxy, orphan: Hash) -> Option<Vec<Hash>> {
        if !self.orphans.contains_key(&orphan) {
            return None;
        }

        let mut roots = Vec::new();
        let mut queue = VecDeque::from([orphan]);
        let mut visited = HashSet::from([orphan]); // We avoid the custom block hasher here. See comment on `orphans` above.
        while let Some(current) = queue.pop_front() {
            if let Some(block) = self.orphans.get(&current) {
                for parent in block.block.header.direct_parents().iter().copied() {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }
            } else {
                let status = consensus.async_get_block_status(current).await;
                if status.is_none_or(|s| s.is_header_only()) {
                    // Block is not in the orphan pool nor does its body exist consensus-wise, so it is a root
                    roots.push(current);
                }
            }
        }
        Some(roots)
    }

    pub async fn unorphan_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        root: Hash,
    ) -> (Vec<Block>, Vec<BlockValidationFuture>, Vec<BlockValidationFuture>) {
        let root_entry = self.orphans.remove(&root); // Try removing the root just in case it was previously an orphan
        let mut process_queue =
            ProcessQueue::from(root_entry.map(|e| e.children).unwrap_or_else(|| self.iterate_child_orphans(root).collect()));
        let mut processing = HashMap::new();
        while let Some(orphan_hash) = process_queue.dequeue() {
            if let Occupied(entry) = self.orphans.entry(orphan_hash) {
                let mut processable = true;
                for p in entry.get().block.header.direct_parents().iter().copied() {
                    if !processing.contains_key(&p) && consensus.async_get_block_status(p).await.is_none_or(|s| s.is_header_only()) {
                        processable = false;
                        break;
                    }
                }
                if processable {
                    let orphan_block = entry.remove();
                    let BlockValidationFutures { block_task, virtual_state_task } =
                        consensus.validate_and_insert_block(orphan_block.block.clone());
                    processing.insert(orphan_hash, (orphan_block.block, block_task, virtual_state_task));
                    process_queue.enqueue_chunk(orphan_block.children);
                }
            }
        }
        itertools::multiunzip(processing.into_values())
    }

    fn iterate_child_orphans(&self, hash: Hash) -> impl Iterator<Item = Hash> + '_ {
        self.orphans.iter().filter_map(move |(&orphan_hash, orphan_block)| {
            if orphan_block.block.header.direct_parents().contains(&hash) {
                Some(orphan_hash)
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::try_join_all;
    use kaspa_consensus_core::{
        api::{BlockValidationFutures, ConsensusApi},
        blockstatus::BlockStatus,
        errors::block::BlockProcessResult,
    };
    use kaspa_consensusmanager::{ConsensusInstance, SessionLock};
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[derive(Default)]
    struct MockProcessor {
        processed: Arc<RwLock<HashSet<Hash>>>,
    }

    async fn block_process_mock() -> BlockProcessResult<BlockStatus> {
        Ok(BlockStatus::StatusUTXOPendingVerification)
    }

    impl ConsensusApi for MockProcessor {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
            self.processed.write().insert(block.hash());
            BlockValidationFutures { block_task: Box::pin(block_process_mock()), virtual_state_task: Box::pin(block_process_mock()) }
        }

        fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
            self.processed.read().get(&hash).map(|_| BlockStatus::StatusUTXOPendingVerification)
        }
    }

    #[tokio::test]
    async fn test_orphan_pool_basics() {
        let max_orphans = 10;
        let ci = ConsensusInstance::new(SessionLock::new(), Arc::new(MockProcessor::default()));
        let consensus = ci.session().await;
        let mut pool = OrphanBlocksPool::new(max_orphans);

        let roots = vec![8.into(), 9.into()];
        let a = Block::from_precomputed_hash(8.into(), vec![]);
        let b = Block::from_precomputed_hash(9.into(), vec![]);
        let c = Block::from_precomputed_hash(10.into(), roots.clone());
        let d = Block::from_precomputed_hash(11.into(), vec![10.into()]);

        pool.add_orphan(c.clone());
        pool.add_orphan(d.clone());

        assert_eq!(pool.get_orphan_roots(&consensus, d.hash()).await.unwrap(), roots);

        consensus.validate_and_insert_block(a.clone()).virtual_state_task.await.unwrap();
        consensus.validate_and_insert_block(b.clone()).virtual_state_task.await.unwrap();

        let (blocks, _, virtual_state_tasks) = pool.unorphan_blocks(&consensus, 8.into()).await;
        try_join_all(virtual_state_tasks).await.unwrap();
        assert_eq!(blocks.into_iter().map(|b| b.hash()).collect::<HashSet<_>>(), HashSet::from([10.into(), 11.into()]));
        assert!(pool.orphans.is_empty());

        drop((a, b, c, d));
    }
}
