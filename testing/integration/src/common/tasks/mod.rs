use self::stop::StopTask;
use async_trait::async_trait;
use futures_util::future::join_all;
use itertools::Itertools;
use kaspa_utils::triggers::SingleTrigger;
use std::sync::Arc;
use tokio::task::JoinHandle;

pub mod block;
pub mod daemon;
pub mod memory_monitor;
pub mod notify;
pub mod stat_recorder;
pub mod stop;
pub mod subscription;
pub mod tick;
pub mod tx;
#[async_trait]
pub trait Task: Sync + Send {
    fn start(&self, stop_signal: SingleTrigger) -> Vec<JoinHandle<()>>;
}

pub type DynTask = Arc<dyn Task>;

#[derive(Default)]
pub struct TasksRunner {
    main: Option<DynTask>,
    tasks: Vec<DynTask>,
    main_handles: Option<Vec<JoinHandle<()>>>,
    handles: Option<Vec<JoinHandle<()>>>,
    main_stop_signal: SingleTrigger,
    stop_signal: SingleTrigger,
}

impl TasksRunner {
    pub fn new(main: Option<DynTask>) -> Self {
        Self {
            main,
            tasks: vec![],
            main_handles: None,
            handles: None,
            main_stop_signal: SingleTrigger::new(),
            stop_signal: SingleTrigger::new(),
        }
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
        if let Some(ref main) = self.main {
            self.main_handles = Some(main.start(self.main_stop_signal.clone()));
            self.tasks.push(StopTask::build(self.main_stop_signal.clone()));
        }
        let handles = self.tasks.iter().cloned().flat_map(|x| x.start(self.stop_signal.clone())).collect_vec();
        self.handles = Some(handles);
    }

    pub fn stop(&self) {
        self.stop_signal.trigger.trigger()
    }

    pub async fn join(&mut self) {
        if let Some(handles) = self.handles.take() {
            join_all(handles).await;
        }

        // Send a stop signal to the main task and wait for it to exit
        self.main_stop_signal.trigger.trigger();
        if let Some(main_handles) = self.main_handles.take() {
            join_all(main_handles).await;
        }
    }
}
