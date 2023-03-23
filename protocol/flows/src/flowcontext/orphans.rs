use futures::future::join_all;
use indexmap::{map::Entry::Occupied, IndexMap};
use itertools::Itertools;
use kaspa_consensus_core::{
    api::{BlockValidationFuture, ConsensusApi},
    block::Block,
    blockstatus::BlockStatus,
};
use kaspa_core::{debug, info, warn};
use kaspa_hashes::Hash;
use kaspa_utils::option::OptionExtensions;
use rand::Rng;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use self::queue::ProcessQueue;

/// The maximum amount of blockLocator hashes to search for known
/// blocks. See check_orphan_resolution_range for further details
pub const ORPHAN_RESOLUTION_RANGE: u32 = 5;

/// The maximum amount of orphans allowed in the orphans pool. This number is an
/// approximation of how many orphans there can possibly be on average. It is based on:
/// 2^ORPHAN_RESOLUTION_RANGE * Ghostdag K.
/// TODO (HF): revisit when block rate changes
pub const MAX_ORPHANS: usize = 600;

/// Internal trait for abstracting the consensus dependency
pub trait ConsensusBlockProcessor {
    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFuture;
    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus>;
}

impl ConsensusBlockProcessor for dyn ConsensusApi {
    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFuture {
        self.validate_and_insert_block(block, true)
    }

    fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
        self.get_block_status(hash)
    }
}

pub struct OrphanBlocksPool<T: ConsensusBlockProcessor + ?Sized> {
    consensus: Arc<T>,
    /// NOTES:
    /// 1. We use IndexMap for cheap random eviction
    /// 2. We avoid the custom block hasher since this pool is pre-validation storage
    orphans: IndexMap<Hash, Block>,
    /// Max number of orphans to keep in the pool
    max_orphans: usize,
}

impl<T: ConsensusBlockProcessor + ?Sized> OrphanBlocksPool<T> {
    pub fn new(consensus: Arc<T>, max_orphans: usize) -> Self {
        Self { consensus, orphans: IndexMap::with_capacity(max_orphans), max_orphans }
    }

    /// Adds the provided block to the orphan pool
    pub fn add_orphan(&mut self, orphan_block: Block) {
        if self.orphans.len() == self.max_orphans {
            debug!("Orphan blocks pool size exceeded. Evicting a random orphan block.");
            // Evict a random orphan in order to keep pool size under the limit
            if let Some((evicted, _)) = self.orphans.swap_remove_index(rand::thread_rng().gen_range(0..self.max_orphans)) {
                debug!("Evicted {} from the orphan blocks pool", evicted);
            }
        }
        info!("Received a block with missing parents, adding to orphan pool: {}", orphan_block.hash());
        self.orphans.insert(orphan_block.hash(), orphan_block);
    }

    /// Returns whether this block is in the orphan pool.
    pub fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans.contains_key(&hash)
    }

    /// Returns the orphan roots of the provided orphan. Orphan roots are ancestors of this orphan which are
    /// not in the orphan pool AND do not exist consensus-wise or are header-only. Given an orphan relayed by
    /// a peer, these blocks should be the next-in-line to be requested from that peer.
    pub fn get_orphan_roots(&self, orphan: Hash) -> Option<Vec<Hash>> {
        if !self.orphans.contains_key(&orphan) {
            return None;
        }
        let mut roots = Vec::new();
        let mut queue = VecDeque::from([orphan]);
        let mut visited = HashSet::from([orphan]); // We avoid the custom block hasher here. See comment on `orphans` above.
        while let Some(current) = queue.pop_front() {
            if let Some(block) = self.orphans.get(&current) {
                for parent in block.header.direct_parents().iter().copied() {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }
            } else {
                let status = self.consensus.get_block_status(current);
                if status.is_none_or(|s| s.is_header_only()) {
                    // Block is not in the orphan pool nor does its body exist consensus-wise, so it is a root
                    roots.push(current);
                }
            }
        }
        Some(roots)
    }

    pub async fn unorphan_blocks(&mut self, root: Hash) -> Vec<Block> {
        self.orphans.remove(&root); // Try removing the root, just in case it was previously an orphan
        let mut process_queue = ProcessQueue::from(self.iterate_child_orphans(root).collect());
        let mut processing = HashMap::new();
        while let Some(orphan_hash) = process_queue.dequeue() {
            // If the entry does not exist it means it was processed on a previous iteration
            if let Occupied(entry) = self.orphans.entry(orphan_hash) {
                let processable =
                    entry.get().header.direct_parents().iter().copied().all(|p| {
                        processing.contains_key(&p) || self.consensus.get_block_status(p).has_value_and(|s| !s.is_header_only())
                    });
                if processable {
                    let orphan_block = entry.remove();
                    processing.insert(orphan_hash, (orphan_block.clone(), self.consensus.validate_and_insert_block(orphan_block)));
                    process_queue.enqueue_chunk(self.iterate_child_orphans(orphan_hash));
                }
            }
        }
        let mut unorphaned_blocks = Vec::with_capacity(processing.len());
        let (blocks, jobs): (Vec<_>, Vec<_>) = processing.into_values().unzip();
        let results = join_all(jobs).await;
        for (block, result) in blocks.into_iter().zip(results) {
            match result {
                Ok(_) => unorphaned_blocks.push(block),
                Err(e) => warn!("Validation failed for orphan block {}: {}", block.hash(), e),
            }
        }
        match unorphaned_blocks.len() {
            0 => {}
            1 => info!("Unorphaned block {}", unorphaned_blocks[0].hash()),
            n => info!("Unorphaned {} blocks: {}", n, unorphaned_blocks.iter().map(|b| b.hash()).format(", ")),
        }
        unorphaned_blocks
    }

    fn iterate_child_orphans(&self, hash: Hash) -> impl Iterator<Item = Hash> + '_ {
        // TODO: consider optimizing by holding a list of child dependencies for each orphan
        self.orphans.iter().filter_map(
            move |(&orphan_hash, orphan_block)| {
                if orphan_block.header.direct_parents().contains(&hash) {
                    Some(orphan_hash)
                } else {
                    None
                }
            },
        )
    }
}

mod queue {
    use super::Hash;
    use std::collections::{HashSet, VecDeque};

    /// A simple deque backed by a set for efficient duplication filtering
    pub struct ProcessQueue {
        deque: VecDeque<Hash>,
        set: HashSet<Hash>,
    }

    impl ProcessQueue {
        pub fn from(set: HashSet<Hash>) -> Self {
            Self { deque: set.iter().copied().collect(), set }
        }

        pub fn enqueue_chunk<I: IntoIterator<Item = Hash>>(&mut self, iter: I) {
            for item in iter {
                if self.set.insert(item) {
                    self.deque.push_back(item);
                }
            }
        }

        pub fn dequeue(&mut self) -> Option<Hash> {
            if let Some(item) = self.deque.pop_front() {
                self.set.remove(&item);
                Some(item)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::errors::block::BlockProcessResult;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockProcessor {
        processed: RefCell<HashSet<Hash>>,
    }

    async fn block_process_mock() -> BlockProcessResult<BlockStatus> {
        Ok(BlockStatus::StatusUTXOPendingVerification)
    }

    impl ConsensusBlockProcessor for MockProcessor {
        fn validate_and_insert_block(&self, block: Block) -> BlockValidationFuture {
            self.processed.borrow_mut().insert(block.hash());
            Box::pin(block_process_mock())
        }

        fn get_block_status(&self, hash: Hash) -> Option<BlockStatus> {
            self.processed.borrow().get(&hash).map(|_| BlockStatus::StatusUTXOPendingVerification)
        }
    }

    #[tokio::test]
    async fn test_orphan_pool_basics() {
        let max_orphans = 10;
        let consensus = Arc::new(MockProcessor::default());
        let mut pool = OrphanBlocksPool::new(consensus, max_orphans);

        let roots = vec![8.into(), 9.into()];
        let a = Block::from_precomputed_hash(8.into(), vec![]);
        let b = Block::from_precomputed_hash(9.into(), vec![]);
        let c = Block::from_precomputed_hash(10.into(), roots.clone());
        let d = Block::from_precomputed_hash(11.into(), vec![10.into()]);

        pool.add_orphan(c.clone());
        pool.add_orphan(d.clone());

        assert_eq!(pool.get_orphan_roots(d.hash()).unwrap(), roots);

        pool.consensus.validate_and_insert_block(a.clone()).await.unwrap();
        pool.consensus.validate_and_insert_block(b.clone()).await.unwrap();

        assert_eq!(
            pool.unorphan_blocks(8.into()).await.into_iter().map(|b| b.hash()).collect::<HashSet<_>>(),
            HashSet::from([10.into(), 11.into()])
        );
        assert!(pool.orphans.is_empty());

        drop((a, b, c, d));
    }
}
