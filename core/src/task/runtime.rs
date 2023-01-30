use crate::{signals::Shutdown, task::service::AsyncServiceResult};
use futures_util::future::{join_all, select_all};
use kaspa_core::core::Core;
use kaspa_core::service::Service;
use kaspa_core::task::service::AsyncService;
use kaspa_core::trace;
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle as ThreadJoinHandle},
};
use tokio::task::{JoinError, JoinHandle as TaskJoinHandle};

const ASYNC_RUNTIME: &str = "async-runtime";

/// AsyncRuntime registers async services and provides
/// a tokio Runtime to run them.
pub struct AsyncRuntime {
    threads: usize,
    services: Mutex<Vec<Arc<dyn AsyncService>>>,
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        // TODO
        Self::new(std::cmp::max(num_cpus::get() / 3, 2))
    }
}

impl AsyncRuntime {
    pub fn new(threads: usize) -> Self {
        trace!("Creating the async-runtime service");
        Self { threads, services: Mutex::new(Vec::new()) }
    }

    pub fn register<T>(&self, service: Arc<T>)
    where
        T: AsyncService,
    {
        // self.services.lock().unwrap().push(AsyncServiceContainer::new(service));
        self.services.lock().unwrap().push(service);
    }

    pub fn init(self: Arc<AsyncRuntime>, core: Arc<Core>) -> Vec<ThreadJoinHandle<()>> {
        trace!("initializing async-runtime service");
        vec![thread::Builder::new().name(ASYNC_RUNTIME.to_string()).spawn(move || self.worker(core)).unwrap()]
    }

    /// Launch a tokio Runtime and run the top-level async objects

    pub fn worker(self: &Arc<AsyncRuntime>, core: Arc<Core>) {
        return tokio::runtime::Builder::new_multi_thread()
            .worker_threads(self.threads)
            .enable_all()
            .build()
            .expect("Failed building the Runtime")
            .block_on(async { self.worker_impl(core).await });
    }

    // #[tokio::main(worker_threads = 2)]
    // TODO: increase the number of threads if needed
    // TODO: build the runtime explicitly and dedicate a number of threads based on the host specs
    pub async fn worker_impl(self: &Arc<AsyncRuntime>, core: Arc<Core>) {
        // Start all async services
        // All services futures are spawned as tokio tasks to enable parallelism
        trace!("async-runtime worker starting");
        let futures = self
            .services
            .lock()
            .unwrap()
            .iter()
            .map(|x| tokio::spawn(x.clone().start()))
            .collect::<Vec<TaskJoinHandle<AsyncServiceResult<()>>>>();

        // wait for at least one service to return
        let (result, _idx, remaining_futures) = select_all(futures).await;
        // if at least one service yields an error, initiate global shutdown
        // this will cause signal_exit() to be executed externally (by Core invoking `stop()`)
        match result {
            Ok(Err(_)) | Err(_) => {
                trace!("shutting down core due to async-runtime error");
                core.shutdown()
            }
            _ => {}
        }

        // wait for remaining services to finish
        join_all(remaining_futures).await.into_iter().collect::<Result<Vec<AsyncServiceResult<()>>, JoinError>>().unwrap();

        // Stop all async services
        // All services futures are spawned as tokio tasks to enable parallelism
        trace!("async-runtime worker stopping");
        let futures = self
            .services
            .lock()
            .unwrap()
            .iter()
            .map(|x| tokio::spawn(x.clone().stop()))
            .collect::<Vec<TaskJoinHandle<AsyncServiceResult<()>>>>();
        join_all(futures).await.into_iter().collect::<Result<Vec<AsyncServiceResult<()>>, JoinError>>().unwrap();

        trace!("async-runtime worker exiting");
    }

    pub fn signal_exit(self: Arc<AsyncRuntime>) {
        trace!("Sending an exit signal to all async-runtime services");
        for service in self.services.lock().unwrap().iter() {
            service.clone().signal_exit();
        }
    }
}

impl Service for AsyncRuntime {
    fn ident(self: Arc<AsyncRuntime>) -> &'static str {
        ASYNC_RUNTIME
    }

    fn start(self: Arc<AsyncRuntime>, core: Arc<Core>) -> Vec<ThreadJoinHandle<()>> {
        self.init(core)
    }

    fn stop(self: Arc<AsyncRuntime>) {
        self.signal_exit()
    }
}
