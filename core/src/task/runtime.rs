use futures_util::future::join_all;
use kaspa_core::core::Core;
use kaspa_core::service::Service;
use kaspa_core::task::service::AsyncService;
use kaspa_core::trace;
use std::{
    sync::{Arc, Mutex},
    thread::{self, JoinHandle as ThreadJoinHandle},
};
use tokio::task::{JoinError, JoinHandle as TaskJoinHandle};

const ASYNC_RUNTIME: &str = "asnyc-runtime";

/// AsyncRuntime registers async services and provides
/// a tokio Runtime to run them.
pub struct AsyncRuntime {
    services: Mutex<Vec<Arc<dyn AsyncService>>>,
}

impl Default for AsyncRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncRuntime {
    pub fn new() -> Self {
        trace!("Creating the async-runtime service");
        Self { services: Mutex::new(Vec::new()) }
    }

    pub fn register<T>(&self, service: Arc<T>)
    where
        T: AsyncService,
    {
        self.services.lock().unwrap().push(service);
    }

    pub fn init(self: Arc<AsyncRuntime>) -> Vec<ThreadJoinHandle<()>> {
        trace!("initializing async-runtime service");
        vec![thread::Builder::new().name(ASYNC_RUNTIME.to_string()).spawn(move || self.worker()).unwrap()]
    }

    /// Launch a tokio Runtime and run the top-level async objects
    #[tokio::main(worker_threads = 2)]
    pub async fn worker(self: &Arc<AsyncRuntime>) {
        // Start all async services
        trace!("async-runtime worker starting");
        let futures = self.services.lock().unwrap().iter().map(|x| x.clone().start()).collect::<Vec<TaskJoinHandle<()>>>();
        join_all(futures).await.into_iter().collect::<Result<Vec<()>, JoinError>>().unwrap();

        // Stop all async services
        trace!("async-runtime worker stopping");
        let futures = self.services.lock().unwrap().iter().map(|x| x.clone().stop()).collect::<Vec<TaskJoinHandle<()>>>();
        join_all(futures).await.into_iter().collect::<Result<Vec<()>, JoinError>>().unwrap();

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

    fn start(self: Arc<AsyncRuntime>, _core: Arc<Core>) -> Vec<ThreadJoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<AsyncRuntime>) {
        self.signal_exit()
    }
}
