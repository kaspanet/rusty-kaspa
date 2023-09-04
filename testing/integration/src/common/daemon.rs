use kaspa_core::core::Core;
use kaspa_database::utils::get_kaspa_tempdir;
use kaspa_grpc_client::GrpcClient;
use kaspa_rpc_core::notify::mode::NotificationMode;
use kaspad::{args::Args, daemon::create_core_with_runtime};
use std::{sync::Arc, time::Duration};
use tempfile::TempDir;

pub struct Daemon {
    pub core: Arc<Core>,
    pub rpc_port: u16,
    _appdir_tempdir: TempDir,
}

impl Daemon {
    pub fn new_random() -> Daemon {
        let args = Args { devnet: true, ..Default::default() };
        Self::new_random_with_args(args)
    }

    pub fn new_random_with_args(mut args: Args) -> Daemon {
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

        let core = create_core_with_runtime(&Default::default(), &args);
        Daemon { core, rpc_port, _appdir_tempdir: appdir_tempdir }
    }

    pub async fn start(&self) -> (Vec<std::thread::JoinHandle<()>>, GrpcClient) {
        let workers = self.core.start();
        tokio::time::sleep(Duration::from_secs(1)).await;
        let rpc_client = GrpcClient::connect(
            NotificationMode::Direct,
            format!("grpc://localhost:{}", self.rpc_port),
            true,
            None,
            false,
            Some(500_000),
        )
        .await
        .unwrap();
        (workers, rpc_client)
    }

    pub async fn new_client(&self) -> GrpcClient {
        GrpcClient::connect(NotificationMode::Direct, format!("grpc://localhost:{}", self.rpc_port), true, None, false, Some(500_000))
            .await
            .unwrap()
    }
}
