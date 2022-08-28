use crate::errors::BlockProcessResult;
use consensus_core::block::Block;
use hashes::Hash;
use parking_lot::{Condvar, Mutex};
use std::{
    collections::{hash_map::Entry::Vacant, HashMap},
    sync::Arc,
};
use tokio::sync::oneshot;

/// An internal struct used to manage a block processing task
struct BlockTaskInternal {
    // The actual block
    block: Arc<Block>,

    // A list of channel senders for transmitting the processing result of this task to the async callers
    result_transmitters: Vec<oneshot::Sender<BlockProcessResult<()>>>,

    // A list of block hashes depending on the completion of this task
    dependent_tasks: Vec<Hash>,
}

impl BlockTaskInternal {
    fn new(block: Arc<Block>, tx: oneshot::Sender<BlockProcessResult<()>>) -> Self {
        Self { block, result_transmitters: vec![tx], dependent_tasks: Vec::new() }
    }
}

/// A concurrent data structure for managing block processing tasks and their DAG dependencies
pub(crate) struct BlockTaskDependencyManager {
    /// Holds pending block hashes and their corresponding tasks
    pending: Mutex<HashMap<Hash, BlockTaskInternal>>,

    // Used to signal that workers are available/idle
    ready_signal: Condvar,
    idle_signal: Condvar,

    // Threshold to the number of pending items above which we wait for
    // workers to complete some work before queuing further work
    ready_threshold: usize,
}

impl BlockTaskDependencyManager {
    pub fn new(ready_threshold: usize) -> Self {
        Self {
            pending: Mutex::new(HashMap::new()),
            ready_signal: Condvar::new(),
            idle_signal: Condvar::new(),
            ready_threshold,
        }
    }

    /// Registers the `(block, result_transmitter)` pair as a pending task. If the block is already pending
    /// and has a corresponding internal task, the task is updated with the additional
    /// result transmitter and the function returns `false` indicating that the task shall
    /// not be queued for processing yet. Note: this function will block if workers are too busy
    /// with previous pending tasks. The function is expected to be called by a single-thread controlling
    /// the reception of block processing tasks.
    pub fn register(&self, block: Arc<Block>, result_transmitter: oneshot::Sender<BlockProcessResult<()>>) -> bool {
        let mut pending = self.pending.lock();
        match pending.entry(block.header.hash) {
            Vacant(e) => {
                e.insert(BlockTaskInternal::new(block, result_transmitter));
                if pending.len() > self.ready_threshold {
                    // If the number of pending items is already too large,
                    // wait for workers to signal readiness.
                    self.ready_signal.wait(&mut pending);
                }
                true
            }
            e => {
                e.and_modify(|v| v.result_transmitters.push(result_transmitter));
                false
            }
        }
    }

    /// To be called by worker threads wanting to begin a processing task which was
    /// previously registered through `self.register`. If any of the direct parents `parent` of
    /// this hash are in `pending` state, the task is queued as a dependency to the `parent` task
    /// and wil be re-evaluated once that task completes -- in which case the function will return `None`.
    pub fn try_begin(&self, hash: Hash) -> Option<Arc<Block>> {
        // Lock the pending map. The contention around the lock is
        // expected to be negligible in header processing time
        let mut pending = self.pending.lock();
        let block = pending.get(&hash).unwrap().block.clone();
        for parent in block.header.direct_parents().iter() {
            if let Some(task) = pending.get_mut(parent) {
                task.dependent_tasks.push(hash);
                return None; // The block will be reprocessed once the pending parent completes processing
            }
        }
        Some(block)
    }

    /// Report the completion of a processing task. Signals progress to the managing thread.
    /// The function returns the final list of `result_transmitters` and a list of
    /// `dependent_tasks` which should be requeued to workers.
    pub fn end(&self, hash: Hash) -> (Vec<oneshot::Sender<BlockProcessResult<()>>>, Vec<Hash>) {
        // Re-lock for post-processing steps
        let mut pending = self.pending.lock();
        let task = pending
            .remove(&hash)
            .expect("processed block is expected to be in pending map");

        if pending.len() == self.ready_threshold {
            self.ready_signal.notify_one();
        }

        if pending.is_empty() {
            self.idle_signal.notify_one();
        }

        (task.result_transmitters, task.dependent_tasks)
    }

    /// Wait until all pending tasks are completed and workers are idle.
    pub fn wait_for_idle(&self) {
        let mut pending = self.pending.lock();
        if !pending.is_empty() {
            self.idle_signal.wait(&mut pending);
        }
    }
}
