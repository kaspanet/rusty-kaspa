use futures_util::Future;
use kaspa_consensus_core::network::NetworkId;
use kaspa_core::{core::Core, signals::Shutdown};
use kaspa_database::utils::get_kaspa_tempdir;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspad_lib::{args::Args, daemon::create_core_with_runtime};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;

use super::client_pool::ClientPool;

pub struct Daemon {
    // Type and suffix of the daemon network
    pub network: NetworkId,

    // Daemon ports
    pub rpc_port: u16,
    pub p2p_port: u16,

    pub core: Arc<Core>,
    workers: Option<Vec<std::thread::JoinHandle<()>>>,

    _appdir_tempdir: TempDir,
}

impl Daemon {
    pub fn new_random(fd_total_budget: i32) -> Daemon {
        // UPnP registration might take some time and is not needed for usual daemon tests
        let args = Args { devnet: true, disable_upnp: true, max_tcp_connections: Some(10), ..Default::default() };
        Self::new_random_with_args(args, fd_total_budget)
    }

    pub fn new_random_with_args(mut args: Args, fd_total_budget: i32) -> Daemon {
        // This should ask the OS to allocate free port for socket 1 to 4.
        let socket1 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_port = socket1.local_addr().unwrap().port();

        let socket2 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let p2p_port = socket2.local_addr().unwrap().port();

        let socket3 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_json_port = socket3.local_addr().unwrap().port();

        let socket4 = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let rpc_borsh_port = socket4.local_addr().unwrap().port();

        drop(socket1);
        drop(socket2);
        drop(socket3);
        drop(socket4);

        args.rpclisten = Some(format!("0.0.0.0:{rpc_port}").try_into().unwrap());
        args.listen = Some(format!("0.0.0.0:{p2p_port}").try_into().unwrap());
        args.rpclisten_json = Some(format!("0.0.0.0:{rpc_json_port}").parse().unwrap());
        args.rpclisten_borsh = Some(format!("0.0.0.0:{rpc_borsh_port}").parse().unwrap());
        let appdir_tempdir = get_kaspa_tempdir();
        args.appdir = Some(appdir_tempdir.path().to_str().unwrap().to_owned());

        let network = args.network();
        let (core, _) = create_core_with_runtime(&Default::default(), &args, fd_total_budget);
        Daemon { network, rpc_port, p2p_port, core, workers: None, _appdir_tempdir: appdir_tempdir }
    }

    pub async fn start(&mut self) -> GrpcClient {
        self.workers = Some(self.core.start());
        // Wait for the node to initialize before connecting to RPC
        tokio::time::sleep(Duration::from_secs(1)).await;
        self.new_client().await
    }

    pub fn shutdown(&mut self) {
        if let Some(workers) = self.workers.take() {
            self.core.shutdown();
            self.core.join(workers);
        }
    }

    pub async fn new_client(&self) -> GrpcClient {
        GrpcClient::connect(NotificationMode::Direct, format!("grpc://localhost:{}", self.rpc_port), true, None, false, Some(500_000))
            .await
            .unwrap()
    }

    pub async fn new_client_pool<T: Send + 'static, F, R>(
        &self,
        pool_size: usize,
        distribution_channel_capacity: usize,
        client_op: F,
    ) -> ClientPool<T>
    where
        F: Fn(Arc<GrpcClient>, T) -> R + Sync + Send + Copy + 'static,
        R: Future<Output = bool> + Send,
    {
        let mut clients = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            clients.push(Arc::new(self.new_client().await));
        }
        ClientPool::new(clients, distribution_channel_capacity, client_op)
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.shutdown()
    }
}
