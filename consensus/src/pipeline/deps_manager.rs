use crate::{errors::BlockProcessResult, model::stores::ghostdag::GhostdagData};
use consensus_core::{block::Block, blockstatus::BlockStatus, BlockHashMap, HashMapCustomHasher};
use hashes::Hash;
use parking_lot::{Condvar, Mutex};
use std::{collections::hash_map::Entry::Vacant, sync::Arc};
use tokio::sync::oneshot;

pub type BlockResultSender = oneshot::Sender<BlockProcessResult<BlockStatus>>;

pub enum BlockTask {
    Exit,
    Process(MaybeTrustedBlock, Vec<BlockResultSender>),
}

#[derive(Clone)]
pub struct MaybeTrustedBlock {
    pub block: Block,
    pub ghostdag_data: Option<Arc<GhostdagData>>,
}

/// An internal struct used to manage a block processing task
struct BlockTaskInternal {
    // The actual block
    block: MaybeTrustedBlock,

    // A list of channel senders for transmitting the processing result of this task to the async callers
    result_transmitters: Vec<BlockResultSender>,

    // A list of block hashes depending on the completion of this task
    dependent_tasks: Vec<Hash>,
}

impl BlockTaskInternal {
    fn new(block: MaybeTrustedBlock, result_transmitters: Vec<BlockResultSender>) -> Self {
        Self { block, result_transmitters, dependent_tasks: Vec::new() }
    }
}

/// A concurrent data structure for managing block processing tasks and their DAG dependencies
pub(crate) struct BlockTaskDependencyManager {
    /// Holds pending block hashes and their corresponding tasks
    pending: Mutex<BlockHashMap<BlockTaskInternal>>,

    // Used to signal that workers are idle
    idle_signal: Condvar,
}

impl BlockTaskDependencyManager {
    pub fn new() -> Self {
        Self { pending: Mutex::new(BlockHashMap::new()), idle_signal: Condvar::new() }
    }

    /// Registers the `(block, result_transmitters)` pair as a pending task. If the block is already pending
    /// and has a corresponding internal task, the task is updated with the additional
    /// result transmitters and the function returns `false` indicating that the task shall
    /// not be queued for processing. The function is expected to be called by a worker
    /// controlling the reception of block processing tasks.
    pub fn register(&self, block: MaybeTrustedBlock, mut result_transmitters: Vec<BlockResultSender>) -> bool {
        let mut pending = self.pending.lock();
        match pending.entry(block.block.header.hash) {
            Vacant(e) => {
                e.insert(BlockTaskInternal::new(block, result_transmitters));
                true
            }
            e => {
                e.and_modify(|v| {
                    v.result_transmitters.append(&mut result_transmitters);
                    if v.block.block.is_header_only() && !block.block.is_header_only() {
                        // The block now includes transactions, so we update the internal block data
                        v.block = block;
                    }
                });
                false
            }
        }
    }

    /// To be called by worker threads wanting to begin a processing task which was
    /// previously registered through `self.register`. If any of the direct parents `parent` of
    /// this hash are in `pending` state, the task is queued as a dependency to the `parent` task
    /// and wil be re-evaluated once that task completes -- in which case the function will return `None`.
    pub fn try_begin(&self, hash: Hash) -> Option<MaybeTrustedBlock> {
        // Lock the pending map. The contention around the lock is
        // expected to be negligible in header processing time
        let mut pending = self.pending.lock();
        let block = pending.get(&hash).unwrap().block.clone();
        for parent in block.block.header.direct_parents().iter() {
            if let Some(task) = pending.get_mut(parent) {
                task.dependent_tasks.push(hash);
                return None; // The block will be reprocessed once the pending parent completes processing
            }
        }
        Some(block)
    }

    /// Report the completion of a processing task. Signals idleness if pending task list is emptied.
    /// The function passes the `block` and the final list of `result_transmitters` to the
    /// provided `callback` function (note that `callback` is called under the internal lock),
    /// and returns a list of `dependent_tasks` which should be requeued to workers.
    pub fn end<F>(&self, hash: Hash, callback: F) -> Vec<Hash>
    where
        F: Fn(MaybeTrustedBlock, Vec<BlockResultSender>),
    {
        // Re-lock for post-processing steps
        let mut pending = self.pending.lock();
        let task = pending.remove(&hash).expect("processed block is expected to be in pending map");

        // Callback within the lock
        callback(task.block, task.result_transmitters);

        if pending.is_empty() {
            self.idle_signal.notify_one();
        }

        task.dependent_tasks
    }

    /// Wait until all pending tasks are completed and workers are idle.
    pub fn wait_for_idle(&self) {
        let mut pending = self.pending.lock();
        if !pending.is_empty() {
            self.idle_signal.wait(&mut pending);
        }
    }
}
