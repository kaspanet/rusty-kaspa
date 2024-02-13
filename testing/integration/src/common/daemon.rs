use kaspa_consensus_core::network::NetworkId;
use kaspa_core::{core::Core, signals::Shutdown};
use kaspa_database::utils::get_kaspa_tempdir;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{address::tracker::DEFAULT_TRACKER_CAPACITY, subscription::context::SubscriptionContext};
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspad_lib::{args::Args, daemon::create_core_with_runtime};
use std::{ops::Deref, sync::Arc, time::Duration};
use tempfile::TempDir;

use kaspa_grpc_client::ClientPool;

pub struct ClientManager {
    // Type and suffix of the daemon network
    pub network: NetworkId,
    pub context: SubscriptionContext,

    // Daemon ports
    pub rpc_port: u16,
    pub p2p_port: u16,
}

impl ClientManager {
    pub fn new(args: &Args) -> Self {
        let network = args.network();
        let context = SubscriptionContext::with_options(Some(DEFAULT_TRACKER_CAPACITY));
        let rpc_port = args.rpclisten.unwrap().normalize(0).port;
        let p2p_port = args.listen.unwrap().normalize(0).port;
        Self { network, context, rpc_port, p2p_port }
    }

    pub async fn new_client(&self) -> GrpcClient {
        GrpcClient::connect(
            NotificationMode::Direct,
            format!("grpc://localhost:{}", self.rpc_port),
            Some(self.context.clone()),
            false,
            None,
            false,
            Some(500_000),
            Default::default(),
        )
        .await
        .unwrap()
    }

    pub async fn new_clients(&self, count: usize) -> Vec<GrpcClient> {
        let mut clients = Vec::with_capacity(count);
        for _ in 0..count {
            clients.push(self.new_client().await);
        }
        clients
    }

    pub async fn new_multi_listener_client(&self) -> GrpcClient {
        GrpcClient::connect(
            NotificationMode::MultiListeners,
            format!("grpc://localhost:{}", self.rpc_port),
            Some(self.context.clone()),
            true,
            None,
            false,
            Some(500_000),
            Default::default(),
        )
        .await
        .unwrap()
    }

    pub async fn new_client_pool<T: Send + 'static>(&self, pool_size: usize, distribution_channel_capacity: usize) -> ClientPool<T>
where {
        let mut clients = Vec::with_capacity(pool_size);
        for _ in 0..pool_size {
            clients.push(Arc::new(self.new_client().await));
        }
        ClientPool::new(clients, distribution_channel_capacity)
    }
}

pub struct Daemon {
    client_manager: Arc<ClientManager>,

    pub core: Arc<Core>,
    workers: Option<Vec<std::thread::JoinHandle<()>>>,

    _appdir_tempdir: TempDir,
}

impl Daemon {
    pub fn fill_args_with_random_ports(args: &mut Args) {
        // This should ask the OS to allocate free port for socket 1 to 4.
        let socket1 = std::net::TcpListener::bind(format!("127.0.0.1:{}", args.rpclisten.map_or(0, |x| x.normalize(0).port))).unwrap();
        let rpc_port = socket1.local_addr().unwrap().port();

        let socket2 = std::net::TcpListener::bind(format!("127.0.0.1:{}", args.listen.map_or(0, |x| x.normalize(0).port))).unwrap();
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
    }

    pub fn new_random(fd_total_budget: i32) -> Daemon {
        // UPnP registration might take some time and is not needed for usual daemon tests
        let args = Args { devnet: true, disable_upnp: true, ..Default::default() };
        Self::new_random_with_args(args, fd_total_budget)
    }

    pub fn new_random_with_args(mut args: Args, fd_total_budget: i32) -> Daemon {
        Self::fill_args_with_random_ports(&mut args);
        Self::new_with_args(args, fd_total_budget)
    }

    pub fn new_with_args(mut args: Args, fd_total_budget: i32) -> Daemon {
        let appdir_tempdir = get_kaspa_tempdir();
        args.appdir = Some(appdir_tempdir.path().to_str().unwrap().to_owned());
        let client_manager = Arc::new(ClientManager::new(&args));
        let (core, _) = create_core_with_runtime(&Default::default(), &args, fd_total_budget);
        Daemon { client_manager, core, workers: None, _appdir_tempdir: appdir_tempdir }
    }

    pub fn client_manager(&self) -> Arc<ClientManager> {
        self.client_manager.clone()
    }

    pub fn run(&mut self) {
        self.workers = Some(self.core.start());
    }

    pub fn join(&mut self) {
        if let Some(workers) = self.workers.take() {
            self.core.join(workers);
        }
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
}

impl Deref for Daemon {
    type Target = ClientManager;

    fn deref(&self) -> &Self::Target {
        &self.client_manager
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.shutdown()
    }
}
