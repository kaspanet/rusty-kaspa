use crate::{errors::BlockProcessResult, model::stores::ghostdag::GhostdagData};
use consensus_core::{block::Block, blockstatus::BlockStatus};
use hashes::Hash;
use parking_lot::{Condvar, Mutex};
use std::{
    collections::{
        hash_map::Entry::{Occupied, Vacant},
        HashMap, VecDeque,
    },
    sync::Arc,
};
use tokio::sync::oneshot;

pub type BlockResultSender = oneshot::Sender<BlockProcessResult<BlockStatus>>;

pub enum BlockProcessingMessage {
    Exit,
    Process(BlockTask, BlockResultSender),
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
    result_transmitter: BlockResultSender,
}

impl BlockTaskInternal {
    fn new(task: BlockTask, result_transmitter: BlockResultSender) -> Self {
        Self { task, result_transmitter }
    }
}

pub(crate) type TaskId = Hash;

struct BlockTaskGroup {
    // Queue of tasks within this group (belong to the same hash; rare event)
    tasks: VecDeque<BlockTaskInternal>,

    // A list of block hashes depending on the completion of this task group
    dependent_tasks: Vec<TaskId>,
}

impl BlockTaskGroup {
    fn new(task: BlockTaskInternal) -> Self {
        Self { tasks: VecDeque::from([task]), dependent_tasks: Vec::new() }
    }
}

/// A concurrent data structure for managing block processing tasks and their DAG dependencies
pub(crate) struct BlockTaskDependencyManager {
    /// Holds pending block hashes and their corresponding tasks
    pending: Mutex<HashMap<Hash, BlockTaskGroup>>,

    // Used to signal that workers are idle
    idle_signal: Condvar,
}

impl BlockTaskDependencyManager {
    pub fn new() -> Self {
        Self { pending: Mutex::new(HashMap::new()), idle_signal: Condvar::new() }
    }

    /// Registers the `(task, result_transmitters)` pair as a pending task. If a task with the same
    /// hash is already pending and has a corresponding internal task group, the task group is updated
    /// with the additional task and the function returns `None` indicating that the task shall
    /// not be queued for processing yet. The function is expected to be called by a single worker
    /// controlling the reception of block processing tasks.
    pub fn register(&self, task: BlockTask, result_transmitter: BlockResultSender) -> Option<TaskId> {
        let mut pending = self.pending.lock();
        let hash = task.block.hash();
        match pending.entry(hash) {
            Vacant(e) => {
                e.insert(BlockTaskGroup::new(BlockTaskInternal::new(task, result_transmitter)));
                Some(hash)
            }
            e => {
                e.and_modify(|v| {
                    v.tasks.push_back(BlockTaskInternal::new(task, result_transmitter));
                });
                None
            }
        }
    }

    /// To be called by worker threads wanting to begin a processing task which was
    /// previously registered through `self.register`. If any of the direct parents `parent` of
    /// this hash are in `pending` state, the task is queued as a dependency to the `parent` task
    /// and wil be re-evaluated once that task completes -- in which case the function will return `None`.
    pub fn try_begin(&self, hash: TaskId) -> Option<BlockTask> {
        // Lock the pending map. The contention around the lock is
        // expected to be negligible in header processing time
        let mut pending = self.pending.lock();
        let group = pending.get(&hash).unwrap();
        let internal_task = group.tasks.front().expect("try_begin expects a task");
        let task = internal_task.task.clone();

        for parent in task.block.header.direct_parents().iter() {
            if let Some(parent_tasks) = pending.get_mut(parent) {
                parent_tasks.dependent_tasks.push(hash);
                return None; // The block will be reprocessed once the pending parent completes processing
            }
        }
        Some(task)
    }

    /// Report the completion of a processing task. Signals idleness if pending task list is emptied.
    /// The function passes the `block` and the `result_transmitter` to the
    /// provided `callback` function (note that `callback` is called under the internal lock),
    /// and returns a list of `dependent_tasks` which should be requeued to workers.
    pub fn end<F>(&self, hash: TaskId, callback: F) -> Vec<TaskId>
    where
        F: Fn(BlockTask, BlockResultSender),
    {
        // Re-lock for post-processing steps
        let mut pending = self.pending.lock();

        let Occupied(mut entry) = pending.entry(hash) else { panic!("processed block is expected to have an entry") };
        let internal_task = entry.get_mut().tasks.pop_front().expect("same task from try_begin is expected");
        let next_tasks = if entry.get().tasks.is_empty() { entry.remove().dependent_tasks } else { vec![hash] };

        // Callback within the lock
        callback(internal_task.task, internal_task.result_transmitter);

        if pending.is_empty() {
            self.idle_signal.notify_one();
        }

        next_tasks
    }

    /// Wait until all pending tasks are completed and workers are idle.
    pub fn wait_for_idle(&self) {
        let mut pending = self.pending.lock();
        if !pending.is_empty() {
            self.idle_signal.wait(&mut pending);
        }
    }
}
