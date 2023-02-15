extern crate consensus;
extern crate core;
extern crate hashes;

use clap::Parser;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::monitor::ConsensusMonitor;
use consensus::config::Config;
use consensus::consensus::Consensus;
use consensus::model::stores::DB;
use consensus::params::DEVNET_PARAMS;
use kaspa_core::{core::Core, signals::Signals, task::runtime::AsyncRuntime};
use kaspa_core::{info, trace};
use kaspa_grpc_server::GrpcServer;
use kaspa_rpc_core::server::collector::ConsensusNotificationChannel;
use kaspa_rpc_core::server::RpcCoreServer;
use p2p_flows::service::P2pService;

mod monitor;

const DEFAULT_DATA_DIR: &str = "datadir";

// TODO: add a Config
// TODO: apply Args to Config
// TODO: log to file

/// Kaspa Node launch arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to store data
    #[arg(short = 'b', long = "appdir")]
    app_dir: Option<String>,

    /// Interface/port to listen for RPC connections (default port: 16110, testnet: 16210)
    #[arg(long = "rpclisten")]
    rpc_listen: Option<String>,

    /// Logging level for all subsystems {off, error, warn, info, debug, trace}
    ///  -- You may also specify <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for individual subsystems
    #[arg(short = 'd', long = "loglevel", default_value = "info")]
    log_level: String,
}

fn get_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return dirs::data_local_dir().unwrap();
    #[cfg(not(target_os = "windows"))]
    return dirs::home_dir().unwrap();
}

fn get_app_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return get_home_dir().join("kaspa-rust");
    #[cfg(not(target_os = "windows"))]
    return get_home_dir().join(".kaspa-rust");
}

pub fn main() {
    // Get CLI arguments
    let args = Args::parse();

    // Initialize the logger
    kaspa_core::log::init_logger(&args.log_level);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // Configure the panic behavior
    kaspa_core::panic::configure_panic();

    // TODO: Refactor all this quick-and-dirty code
    let app_dir = args
        .app_dir
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    let app_dir = if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) };
    let db_dir = app_dir.join(DEFAULT_DATA_DIR);
    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
    fs::create_dir_all(db_dir.as_path()).unwrap();
    let grpc_server_addr = args.rpc_listen.unwrap_or_else(|| "127.0.0.1:16610".to_string()).parse().unwrap();

    let core = Arc::new(Core::new());

    // ---

    let config = Config::new(DEVNET_PARAMS); // TODO: network type
    let db = Arc::new(DB::open_default(db_dir.to_str().unwrap()).unwrap());
    let consensus = Arc::new(Consensus::new(db, &config));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));

    let notification_channel = ConsensusNotificationChannel::default();
    let rpc_core_server = Arc::new(RpcCoreServer::new(consensus.clone(), notification_channel.receiver()));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_server.service()));
    let p2p_service = Arc::new(P2pService::new(consensus.clone()));

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new());
    async_runtime.register(rpc_core_server);
    async_runtime.register(grpc_server);
    async_runtime.register(p2p_service);

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(monitor);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
