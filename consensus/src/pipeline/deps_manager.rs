use crate::{errors::BlockProcessResult, model::stores::ghostdag::GhostdagData};
use consensus_core::{block::Block, blockstatus::BlockStatus, BlockHashMap, HashMapCustomHasher};
use hashes::Hash;
use parking_lot::{Condvar, Mutex};
use std::{collections::hash_map::Entry::Vacant, sync::Arc};
use tokio::sync::oneshot;

pub type BlockResultSender = oneshot::Sender<BlockProcessResult<BlockStatus>>;

pub enum BlockProcessingMessage {
    Exit,
    Process(BlockTask, Vec<BlockResultSender>),
}

#[derive(Clone)]
pub struct BlockTask {
    /// The block to process, possibly header-only
    pub block: Block,

    /// Possibly attached trusted ghostdag data - will be set only for
    /// trusted blocks arriving as part of the pruning proof
    pub trusted_ghostdag_data: Option<Arc<GhostdagData>>,

    /// A flag indicating whether to trigger virtual UTXO processing
    pub update_virtual: bool,
}

/// An internal struct used to manage a block processing task
struct BlockTaskInternal {
    // The externally accepted block task
    task: BlockTask,

    // A list of channel senders for transmitting the processing result of this task to the async callers
    result_transmitters: Vec<BlockResultSender>,

    // A list of block hashes depending on the completion of this task
    dependent_tasks: Vec<Hash>,
}

impl BlockTaskInternal {
    fn new(task: BlockTask, result_transmitters: Vec<BlockResultSender>) -> Self {
        Self { task, result_transmitters, dependent_tasks: Vec::new() }
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

    /// Registers the `(task, result_transmitters)` pair as a pending task. If the task is already pending
    /// and has a corresponding internal task, the task is updated with the additional
    /// result transmitters and the function returns `false` indicating that the task shall
    /// not be queued for processing. The function is expected to be called by a worker
    /// controlling the reception of block processing tasks.
    pub fn register(&self, task: BlockTask, mut result_transmitters: Vec<BlockResultSender>) -> bool {
        let mut pending = self.pending.lock();
        match pending.entry(task.block.header.hash) {
            Vacant(e) => {
                e.insert(BlockTaskInternal::new(task, result_transmitters));
                true
            }
            e => {
                e.and_modify(|v| {
                    v.result_transmitters.append(&mut result_transmitters);
                    if v.task.block.is_header_only() && !task.block.is_header_only() {
                        // The block now includes transactions, so we update the internal task data
                        v.task = task;
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
    pub fn try_begin(&self, hash: Hash) -> Option<BlockTask> {
        // Lock the pending map. The contention around the lock is
        // expected to be negligible in header processing time
        let mut pending = self.pending.lock();
        let task = pending.get(&hash).unwrap().task.clone();
        for parent in task.block.header.direct_parents().iter() {
            if let Some(parent_task) = pending.get_mut(parent) {
                parent_task.dependent_tasks.push(hash);
                return None; // The block will be reprocessed once the pending parent completes processing
            }
        }
        Some(task)
    }

    /// Report the completion of a processing task. Signals idleness if pending task list is emptied.
    /// The function passes the `block` and the final list of `result_transmitters` to the
    /// provided `callback` function (note that `callback` is called under the internal lock),
    /// and returns a list of `dependent_tasks` which should be requeued to workers.
    pub fn end<F>(&self, hash: Hash, callback: F) -> Vec<Hash>
    where
        F: Fn(BlockTask, Vec<BlockResultSender>),
    {
        // Re-lock for post-processing steps
        let mut pending = self.pending.lock();
        let internal_task = pending.remove(&hash).expect("processed block is expected to be in pending map");

        // Callback within the lock
        callback(internal_task.task, internal_task.result_transmitters);

        if pending.is_empty() {
            self.idle_signal.notify_one();
        }

        internal_task.dependent_tasks
    }

    /// Wait until all pending tasks are completed and workers are idle.
    pub fn wait_for_idle(&self) {
        let mut pending = self.pending.lock();
        if !pending.is_empty() {
            self.idle_signal.wait(&mut pending);
        }
    }
}
