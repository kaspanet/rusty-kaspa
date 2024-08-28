use indexmap::{map::Entry::Occupied, IndexMap};
use kaspa_consensus_core::{
    api::{BlockValidationFuture, BlockValidationFutures},
    block::Block,
};
use kaspa_consensusmanager::{BlockProcessingBatch, ConsensusProxy};
use kaspa_core::debug;
use kaspa_hashes::Hash;
use kaspa_utils::option::OptionExtensions;
use rand::Rng;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    iter::once,
};

use super::process_queue::ProcessQueue;

/// The output of an orphan pool block query
#[derive(Debug)]
pub enum OrphanOutput {
    /// Block is orphan with the provided missing roots
    Roots(Vec<Hash>),
    /// Block has no missing roots (but it might have known orphan ancestors which are returned
    /// along with their corresponding consensus processing tasks)
    NoRoots(BlockProcessingBatch),
    /// The block does not exist in the orphan pool
    Unknown,
}

#[derive(Debug)]
enum FindRootsOutput {
    /// Block is orphan with the provided missing roots and a possible set of known orphan ancestors
    Roots(Vec<Hash>, HashSet<Hash>),
    /// Block has no missing roots (but it might have known orphan ancestors)
    NoRoots(HashSet<Hash>),
}

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
    /// The log base 2 of `max_orphans`
    max_orphans_log: usize,
}

impl OrphanBlocksPool {
    pub fn new(max_orphans: usize) -> Self {
        Self {
            orphans: IndexMap::with_capacity(max_orphans),
            max_orphans,
            max_orphans_log: (max_orphans as f64).log2().ceil() as usize,
        }
    }

    /// Adds the provided block to the orphan pool. Returns None if the block is already
    /// in the pool or if the pool chose not to keep it for any reason
    pub async fn add_orphan(&mut self, consensus: &ConsensusProxy, orphan_block: Block) -> Option<OrphanOutput> {
        let orphan_hash = orphan_block.hash();
        if self.orphans.contains_key(&orphan_hash) {
            return None;
        }
        orphan_block.asses_for_cache()?;
        let (roots, orphan_ancestors) =
            match self.get_orphan_roots(consensus, orphan_block.header.direct_parents().iter().copied().collect()).await {
                FindRootsOutput::Roots(roots, orphan_ancestors) => (roots, orphan_ancestors),
                FindRootsOutput::NoRoots(orphan_ancestors) => {
                    let blocks: Vec<_> =
                        orphan_ancestors.into_iter().map(|h| self.orphans.swap_remove(&h).expect("orphan ancestor").block).collect();
                    return Some(OrphanOutput::NoRoots(consensus.validate_and_insert_block_batch(blocks)));
                }
            };

        if self.orphans.len() == self.max_orphans {
            let mut eviction_succeeded = false;
            debug!("Orphan blocks pool size exceeded. Trying to evict a random orphan block.");
            // Retry up to a logarithmic number of times
            for i in 0..self.max_orphans_log {
                // Evict a random orphan in order to keep pool size under the limit
                let rand_index = rand::thread_rng().gen_range(0..self.orphans.len());
                if !orphan_ancestors.is_empty() {
                    // IndexMap has no API for getting a removable Entry by index
                    if let Some(rand_hash) = self.orphans.get_index(rand_index).map(|(&h, _)| h) {
                        if orphan_ancestors.contains(&rand_hash) {
                            continue; // Do not evict an ancestor of this new orphan
                        }
                    }
                }
                if let Some((evicted, _)) = self.orphans.swap_remove_index(rand_index) {
                    debug!("Evicted {} from the orphan blocks pool for new block {} (after {} retries)", evicted, orphan_hash, i);
                    eviction_succeeded = true;
                    break;
                }
            }
            if !eviction_succeeded {
                // All retries have found an existing ancestor, so we reject the new block
                debug!(
                    "Tried to evict a random orphan for new orphan {}, but all {} retries found an existing ancestor. Rejecting.",
                    orphan_hash, self.max_orphans_log
                );
                return None;
            }
        }
        for parent in orphan_block.header.direct_parents() {
            if let Some(entry) = self.orphans.get_mut(parent) {
                entry.children.insert(orphan_hash);
            }
        }
        // Insert
        self.orphans.insert(orphan_block.hash(), OrphanBlock::new(orphan_block, self.iterate_child_orphans(orphan_hash).collect()));
        // Return roots
        Some(OrphanOutput::Roots(roots))
    }

    /// Returns whether this block is in the orphan pool.
    pub fn is_known_orphan(&self, hash: Hash) -> bool {
        self.orphans.contains_key(&hash)
    }

    /// Returns the orphan roots of the provided orphan. Orphan roots are ancestors of this orphan which are
    /// not in the orphan pool AND do not exist consensus-wise or are header-only. Given an orphan relayed by
    /// a peer, these blocks should be the next-in-line to be requested from that peer.
    pub async fn get_orphan_roots_if_known(&self, consensus: &ConsensusProxy, orphan: Hash) -> OrphanOutput {
        if let Some(orphan_block) = self.orphans.get(&orphan) {
            match self.get_orphan_roots(consensus, orphan_block.block.header.direct_parents().iter().copied().collect()).await {
                FindRootsOutput::Roots(roots, _) => OrphanOutput::Roots(roots),
                FindRootsOutput::NoRoots(_) => OrphanOutput::NoRoots(Default::default()),
            }
        } else {
            OrphanOutput::Unknown
        }
    }

    /// Internal get roots method. The arg `queue` is the set of blocks to perform BFS from and
    /// search through the orphan pool and consensus until finding any unknown roots or finding
    /// out that no ancestor is missing.
    async fn get_orphan_roots(&self, consensus: &ConsensusProxy, mut queue: VecDeque<Hash>) -> FindRootsOutput {
        let mut roots = Vec::new();
        let mut visited: HashSet<_> = queue.iter().copied().collect();
        let mut orphan_ancestors = HashSet::new();
        while let Some(current) = queue.pop_front() {
            if let Some(block) = self.orphans.get(&current) {
                orphan_ancestors.insert(current);
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

        if roots.is_empty() {
            FindRootsOutput::NoRoots(orphan_ancestors)
        } else {
            FindRootsOutput::Roots(roots, orphan_ancestors)
        }
    }

    pub async fn unorphan_blocks(
        &mut self,
        consensus: &ConsensusProxy,
        root: Hash,
    ) -> (Vec<Block>, Vec<BlockValidationFuture>, Vec<BlockValidationFuture>) {
        let root_entry = self.orphans.swap_remove(&root); // Try removing the root just in case it was previously an orphan
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
                    let orphan_block = entry.swap_remove();
                    let BlockValidationFutures { block_task, virtual_state_task } =
                        consensus.validate_and_insert_block(orphan_block.block.clone());
                    processing.insert(orphan_hash, (orphan_block.block, block_task, virtual_state_task));
                    process_queue.enqueue_chunk(orphan_block.children);
                }
            }
        }
        // We deliberately want all processing tasks to be awaited out of the orphan pool lock
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

    /// Iterate all orphans and remove blocks which are no longer orphans.
    /// This is important for the overall health of the pool and for ensuring that
    /// orphan blocks don't evict due to pool size limit while already processed
    /// blocks remain in it. Should be called following IBD.  
    pub async fn revalidate_orphans(&mut self, consensus: &ConsensusProxy) -> (Vec<Hash>, Vec<BlockValidationFuture>) {
        // First, cleanup blocks already processed by consensus
        let mut i = 0;
        while i < self.orphans.len() {
            if let Some((&h, _)) = self.orphans.get_index(i) {
                if consensus.async_get_block_status(h).await.is_some_and(|s| s.is_invalid() || s.has_block_body()) {
                    // If we swap removed do not advance i so that we revisit the new element moved
                    // to i in the next iteration. Loop will progress because len is shorter now.
                    self.orphans.swap_remove_index(i);
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        // Next, search for root blocks which are processable. A processable block is a block
        // which all of its parents are known to consensus with valid body state
        let mut roots = Vec::new();
        for block in self.orphans.values() {
            let mut processable = true;
            for parent in block.block.header.direct_parents().iter().copied() {
                if self.orphans.contains_key(&parent)
                    || consensus.async_get_block_status(parent).await.is_none_or(|status| status.is_header_only())
                {
                    processable = false;
                    break;
                }
            }
            if processable {
                roots.push(block.block.clone());
            }
        }

        // Now process the roots and unorphan their descendents
        let mut virtual_processing_tasks = Vec::with_capacity(roots.len());
        let mut queued_hashes = Vec::with_capacity(roots.len());
        for root in roots {
            let root_hash = root.hash();
            // Queue the root for processing
            let BlockValidationFutures { block_task: _, virtual_state_task: root_task } = consensus.validate_and_insert_block(root);
            // Queue its descendents which are processable
            let (descendent_blocks, _, descendents_tasks) = self.unorphan_blocks(consensus, root_hash).await;
            // Keep track of all hashes and tasks
            virtual_processing_tasks.extend(once(root_task).chain(descendents_tasks));
            queued_hashes.extend(once(root_hash).chain(descendent_blocks.into_iter().map(|block| block.hash())));
        }

        // We deliberately want the processing tasks to be awaited out of the orphan pool lock
        (queued_hashes, virtual_processing_tasks)
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
    use kaspa_core::assert_match;
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

        let e = Block::from_precomputed_hash(12.into(), vec![10.into()]);
        let f = Block::from_precomputed_hash(13.into(), vec![12.into()]);
        let g = Block::from_precomputed_hash(14.into(), vec![13.into()]);
        let h = Block::from_precomputed_hash(15.into(), vec![14.into()]);
        let k = Block::from_precomputed_hash(16.into(), vec![15.into()]);

        pool.add_orphan(&consensus, c.clone()).await.unwrap();
        pool.add_orphan(&consensus, d.clone()).await.unwrap();

        assert_match!(pool.get_orphan_roots_if_known(&consensus, d.hash()).await, OrphanOutput::Roots(recv_roots) if recv_roots == roots);

        consensus.validate_and_insert_block(a.clone()).virtual_state_task.await.unwrap();
        consensus.validate_and_insert_block(b.clone()).virtual_state_task.await.unwrap();

        // Test unorphaning
        let (blocks, _, virtual_state_tasks) = pool.unorphan_blocks(&consensus, 8.into()).await;
        try_join_all(virtual_state_tasks).await.unwrap();
        assert_eq!(blocks.into_iter().map(|b| b.hash()).collect::<HashSet<_>>(), HashSet::from([10.into(), 11.into()]));
        assert!(pool.orphans.is_empty());

        // Test revalidation
        pool.add_orphan(&consensus, f.clone()).await.unwrap();
        pool.add_orphan(&consensus, g.clone()).await.unwrap();
        pool.add_orphan(&consensus, k.clone()).await.unwrap();
        assert_eq!(pool.orphans.len(), 3);
        consensus.validate_and_insert_block(e.clone()).virtual_state_task.await.unwrap();
        pool.revalidate_orphans(&consensus).await;
        assert_eq!(pool.orphans.len(), 1);
        assert!(pool.orphans.contains_key(&k.hash())); // k's parent, h, was never inserted to the pool
        consensus.validate_and_insert_block(h.clone()).virtual_state_task.await.unwrap();
        pool.revalidate_orphans(&consensus).await;
        assert!(pool.orphans.is_empty());

        drop((a, b, c, d, e, f, g, h, k));
    }
}
