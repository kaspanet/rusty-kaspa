use kaspa_core::core::Core;
use kaspa_core::service::Service;
use kaspa_core::trace;
use rpc_grpc::server::GrpcServer;
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};
use tokio::join;

const ASYNC_RUNTIME: &str = "asnyc-runtime";

/// AsyncRuntime contains top-level async objects and provides
/// a tokio Runtime to run them.
pub struct AsyncRuntime {
    grpc_server: Arc<GrpcServer>,
}

impl AsyncRuntime {
    pub fn new(grpc_server: Arc<GrpcServer>) -> Self {
        trace!("Creating the async runtime service");
        Self { grpc_server }
    }

    pub fn init(self: Arc<AsyncRuntime>) -> Vec<JoinHandle<()>> {
        trace!("Initializing the async runtime service");
        vec![thread::Builder::new().name(ASYNC_RUNTIME.to_string()).spawn(move || self.worker()).unwrap()]
    }

    /// Launch a tokio Runtime and run the top-level async objects
    #[tokio::main(worker_threads = 2)]
    pub async fn worker(self: &Arc<AsyncRuntime>) {
        // Start all the top-level objects
        trace!("Starting the async runtime worker");
        let result = join!(self.grpc_server.start());
        match result.0 {
            Ok(_) => {}
            Err(err) => {
                trace!("gRPC server starter task left with error {0}", err);
            }
        }

        // Stop all the top-level objects
        trace!("Stopping the async runtime worker");
        let result = join!(self.grpc_server.stop());
        match result.0 {
            Ok(_) => {}
            Err(err) => {
                trace!("gRPC server closer task left with error {0}", err);
            }
        }
    }

    pub fn signal_exit(self: Arc<AsyncRuntime>) {
        trace!("Sending an exit signal to the async runtime");
        self.grpc_server.signal_exit();
    }
}

impl Service for AsyncRuntime {
    fn ident(self: Arc<AsyncRuntime>) -> &'static str {
        ASYNC_RUNTIME
    }

    fn start(self: Arc<AsyncRuntime>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<AsyncRuntime>) {
        self.signal_exit()
    }
}
