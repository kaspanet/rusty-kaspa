extern crate consensus;
extern crate core;
extern crate hashes;

use clap::Parser;
use consensus::model::stores::DB;
use kaspa_core::{core::Core, signals::Signals, task::runtime::AsyncRuntime};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::__private::PathAsDisplay;

use crate::monitor::ConsensusMonitor;
use consensus::consensus::Consensus;
use consensus::params::DEVNET_PARAMS;
use kaspa_core::{info, trace};
use kaspa_wrpc_server::server::{Options as WrpcServerOptions, WrpcEncoding, WrpcServer};
use rpc_core::server::collector::ConsensusNotificationChannel;
use rpc_core::server::RpcCoreServer;
use rpc_grpc::server::GrpcServer;

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

    /// Interface/port to listen for gRPC connections (default port: 16110, testnet: 16210)
    #[arg(long = "rpclisten")]
    grpc_listen: Option<String>,

    /// Interface/port to listen for wRPC Borsh connections (default port: 8080)
    #[arg(long = "rpclisten-borsh")]
    wrpc_listen_borsh: Option<String>,

    /// Interface/port to listen for wRPC JSON connections (default port: 9090)
    #[arg(long = "rpclisten-json")]
    wrpc_listen_json: Option<String>,

    /// Enable verbose logging of wRPC data exchange
    #[arg(long = "wrpc-verbose")]
    wrpc_verbose: bool,

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

    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // TODO: Refactor all this quick-and-dirty code
    let app_dir = args
        .app_dir
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    let app_dir = if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) };
    let db_dir = app_dir.join(DEFAULT_DATA_DIR);
    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.as_display());
    info!("Data directory: {}", db_dir.as_display());
    fs::create_dir_all(db_dir.as_path()).unwrap();
    let grpc_server_addr = args.grpc_listen.unwrap_or_else(|| "127.0.0.1:16610".to_string()).parse().unwrap();

    let core = Arc::new(Core::new());

    // ---

    let params = DEVNET_PARAMS;
    let db = Arc::new(DB::open_default(db_dir.to_str().unwrap()).unwrap());
    let consensus = Arc::new(Consensus::new(db, &params));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));

    let notification_channel = ConsensusNotificationChannel::default();
    let rpc_core_server = Arc::new(RpcCoreServer::new(consensus.clone(), notification_channel.receiver()));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_server.service()));

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new());
    async_runtime.register(rpc_core_server.clone());
    async_runtime.register(grpc_server);

    // Register wRPC servers based on command line arguments
    [(args.wrpc_listen_borsh, WrpcEncoding::Borsh), (args.wrpc_listen_json, WrpcEncoding::SerdeJson)]
        .iter()
        .filter_map(|(listen_address, encoding)| {
            listen_address.as_ref().map(|listen_address| {
                Arc::new(WrpcServer::new(
                    rpc_core_server.service(),
                    encoding,
                    WrpcServerOptions {
                        listen_address: listen_address.to_string(),
                        verbose: args.wrpc_verbose,
                        ..WrpcServerOptions::default()
                    },
                ))
            })
        })
        .for_each(|server|async_runtime.register(server));

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(monitor);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
