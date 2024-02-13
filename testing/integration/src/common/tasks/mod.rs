use async_trait::async_trait;
use futures_util::future::join_all;
use itertools::Itertools;
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub mod block;
pub mod daemon;
pub mod memory_monitor;
pub mod stat_recorder;
pub mod tick;

#[async_trait]
pub trait Task: Sync + Send {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>>;
}

pub type DynTask = Arc<dyn Task>;

#[derive(Default)]
pub struct TasksRunner {
    tasks: Vec<DynTask>,
    handles: Option<Vec<JoinHandle<()>>>,
    stop_signal: SingleTrigger,
}

impl TasksRunner {
    pub fn new() -> Self {
        Self { tasks: vec![], handles: None, stop_signal: SingleTrigger::new() }
    }

    pub fn task(mut self, task: DynTask) -> Self {
        self.tasks.push(task);
        self
    }

    pub fn optional_task(mut self, task: Option<DynTask>) -> Self {
        if let Some(task) = task {
            self.tasks.push(task);
        }
        self
    }

    pub fn run(&mut self) {
        let handles = self.tasks.iter().cloned().flat_map(|x| x.start(self.stop_signal.clone())).collect_vec();
        self.handles = Some(handles);
    }

    pub async fn join(&mut self) {
        if let Some(handles) = self.handles.take() {
            join_all(handles).await;
        }
    }
}
