use crate::errors::BlockProcessResult;
use kaspa_consensus_core::{block::Block, blockstatus::BlockStatus};
use kaspa_hashes::Hash;
use parking_lot::{Condvar, Mutex};
use std::collections::{
    hash_map::Entry::{Occupied, Vacant},
    HashMap, VecDeque,
};
use tokio::sync::oneshot;

pub type BlockResultSender = oneshot::Sender<BlockProcessResult<BlockStatus>>;

pub enum BlockProcessingMessage {
    Exit,
    Process(BlockTask, BlockResultSender, BlockResultSender),
}

impl BlockProcessingMessage {
    pub fn is_processing_message(&self) -> bool {
        matches!(self, BlockProcessingMessage::Process(_, _, _))
    }

    pub fn is_exit_message(&self) -> bool {
        matches!(self, BlockProcessingMessage::Exit)
    }
}

pub enum VirtualStateProcessingMessage {
    Exit,
    Process(BlockTask, BlockResultSender),
}

impl VirtualStateProcessingMessage {
    pub fn is_processing_message(&self) -> bool {
        matches!(self, VirtualStateProcessingMessage::Process(_, _))
    }

    pub fn is_exit_message(&self) -> bool {
        matches!(self, VirtualStateProcessingMessage::Exit)
    }
}

pub enum BlockTask {
    /// Ordinary block processing task, requiring full validation. The block might be header-only
    Ordinary { block: Block },

    /// Trusted block processing task, only requiring partial validation.
    /// Trusted blocks arrive as part of the pruning proof; the block might be header-only.
    Trusted { block: Block },
}

impl BlockTask {
    pub fn block(&self) -> &Block {
        match self {
            BlockTask::Ordinary { block } => block,
            BlockTask::Trusted { block } => block,
        }
    }

    pub fn is_ordinary(&self) -> bool {
        matches!(self, BlockTask::Ordinary { .. })
    }

    pub fn is_trusted(&self) -> bool {
        matches!(self, BlockTask::Trusted { .. })
    }

    pub fn requires_virtual_processing(&self) -> bool {
        // Trusted blocks should not trigger virtual processing
        self.is_ordinary()
    }
}

/// An internal struct used to manage a block processing task
struct BlockTaskInternal {
    // The externally accepted block task
    task: Option<BlockTask>,

    // A list of channel senders for transmitting the processing result of this task to the async callers
    block_result_transmitter: BlockResultSender,
    virtual_state_result_transmitter: BlockResultSender,
}

impl BlockTaskInternal {
    fn new(task: BlockTask, block_result_transmitter: BlockResultSender, virtual_state_result_transmitter: BlockResultSender) -> Self {
        Self { task: Some(task), block_result_transmitter, virtual_state_result_transmitter }
    }
}

pub(crate) type TaskId = Hash;

/// We usually only have a single task per hash. This enum optimizes for this.
enum TaskQueue {
    Empty,
    Single(BlockTaskInternal),
    Many(VecDeque<BlockTaskInternal>),
}

impl TaskQueue {
    fn new(task: BlockTaskInternal) -> Self {
        TaskQueue::Single(task)
    }

    fn push_back(&mut self, task: BlockTaskInternal) {
        match self {
            TaskQueue::Empty => *self = Self::Single(task),
            TaskQueue::Single(_) => {
                let prev = std::mem::replace(self, Self::Many(VecDeque::with_capacity(2)));
                let TaskQueue::Single(t) = prev else { panic!() };
                let TaskQueue::Many(q) = self else { panic!() };
                q.push_back(t);
                q.push_back(task);
            }
            TaskQueue::Many(q) => q.push_back(task),
        }
    }

    fn front(&self) -> Option<&BlockTaskInternal> {
        match self {
            TaskQueue::Empty => None,
            TaskQueue::Single(t) => Some(t),
            TaskQueue::Many(q) => q.front(),
        }
    }

    fn front_mut(&mut self) -> Option<&mut BlockTaskInternal> {
        match self {
            TaskQueue::Empty => None,
            TaskQueue::Single(t) => Some(t),
            TaskQueue::Many(q) => q.front_mut(),
        }
    }

    fn pop_front(&mut self) -> Option<BlockTaskInternal> {
        match self {
            TaskQueue::Empty => None,
            TaskQueue::Single(_) => {
                let prev = std::mem::replace(self, Self::Empty);
                let TaskQueue::Single(t) = prev else { panic!() };
                Some(t)
            }
            TaskQueue::Many(q) => q.pop_front(),
        }
    }

    fn is_empty(&self) -> bool {
        match self {
            TaskQueue::Empty => true,
            TaskQueue::Single(_) => false,
            TaskQueue::Many(q) => q.is_empty(),
        }
    }
}

struct BlockTaskGroup {
    // Queue of tasks within this group (where all belong to the same hash)
    tasks: TaskQueue,

    // A list of block hashes depending on the completion of this task group
    dependent_tasks: Vec<TaskId>,
}

impl BlockTaskGroup {
    fn new(task: BlockTaskInternal) -> Self {
        Self { tasks: TaskQueue::new(task), dependent_tasks: Vec::new() }
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

    /// Registers the `(task, result_transmitter)` pair as a pending task. If a task with the same
    /// hash is already pending and has a corresponding internal task group, the task group is updated
    /// with the additional task and the function returns `None` indicating that the task shall
    /// not be queued for processing yet. The function is expected to be called by a single worker
    /// controlling the reception of block processing tasks.
    pub fn register(
        &self,
        task: BlockTask,
        block_result_transmitter: BlockResultSender,
        virtual_state_result_transmitter: BlockResultSender,
    ) -> Option<TaskId> {
        let mut pending = self.pending.lock();
        let hash = task.block().hash();
        match pending.entry(hash) {
            Vacant(e) => {
                e.insert(BlockTaskGroup::new(BlockTaskInternal::new(
                    task,
                    block_result_transmitter,
                    virtual_state_result_transmitter,
                )));
                Some(hash)
            }
            e => {
                e.and_modify(|g| {
                    g.tasks.push_back(BlockTaskInternal::new(task, block_result_transmitter, virtual_state_result_transmitter));
                });
                None
            }
        }
    }

    /// To be called by worker threads wanting to begin a processing task which was
    /// previously registered through `self.register`. If any of the direct parents `parent` of
    /// this task id are in `pending` state, the task is queued as a dependency to the `parent` task
    /// and wil be re-evaluated once that task completes -- in which case the function will return `None`.
    pub fn try_begin(&self, task_id: TaskId) -> Option<BlockTask> {
        // Lock the pending map. The contention around the lock is
        // expected to be negligible in task processing time
        let mut pending = self.pending.lock();
        let group = pending.get(&task_id).expect("try_begin expects a task group");
        let internal_task = group.tasks.front().expect("try_begin expects a task");
        let header = internal_task.task.as_ref().expect("task is expected to not be taken").block().header.clone();
        for parent in header.direct_parents().iter() {
            if let Some(parent_task) = pending.get_mut(parent) {
                parent_task.dependent_tasks.push(task_id);
                return None; // The block will be reprocessed once the pending parent completes processing
            }
        }
        // Re-access and take the inner task (now with mutable access)
        Some(pending.get_mut(&task_id).unwrap().tasks.front_mut().unwrap().task.take().unwrap())
    }

    /// Report the completion of a processing task. Signals idleness if pending task list is emptied.
    /// The function passes the `task` and the `result_transmitter` to the
    /// provided `callback` function (note that `callback` is called under the internal lock),
    /// and returns a list of `dependent_tasks` which should be requeued to workers.
    pub fn end<F>(&self, task: BlockTask, callback: F) -> Vec<TaskId>
    where
        F: Fn(BlockTask, BlockResultSender, BlockResultSender),
    {
        let task_id = task.block().hash();
        // Re-lock for post-processing steps
        let mut pending = self.pending.lock();

        let Occupied(mut entry) = pending.entry(task_id) else { panic!("processed task is expected to have an entry") };
        let internal_task = entry.get_mut().tasks.pop_front().expect("same task from try_begin is expected");

        // If this task group is not empty, we return the same hash in order for the next task in
        // the group to be queued, otherwise we return the dependent tasks
        let next_tasks = if entry.get().tasks.is_empty() { entry.remove().dependent_tasks } else { vec![task_id] };

        // We expect the inner task to be taken by `try_begin`
        assert!(internal_task.task.is_none());

        // Callback within the lock
        callback(task, internal_task.block_result_transmitter, internal_task.virtual_state_result_transmitter);

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
