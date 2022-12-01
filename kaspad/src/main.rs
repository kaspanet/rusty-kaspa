extern crate consensus;
extern crate core;
extern crate hashes;

use clap::Parser;
use consensus::model::stores::DB;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use thiserror::__private::PathAsDisplay;

use consensus::consensus::Consensus;
use consensus::params::MAINNET_PARAMS;
use consensus_core::blockhash;
use hashes::Hash;
use kaspa_core::core::Core;
use kaspa_core::*;
use rpc_core::server::collector::ConsensusNotificationChannel;
use rpc_core::server::service::RpcCoreService;
use rpc_grpc::server::GrpcServer;

use crate::async_runtime::AsyncRuntime;
use crate::emulator::ConsensusMonitor;

mod async_runtime;
mod emulator;

const DEFAULT_DATA_DIR: &str = "datadir";

/// Kaspa Network Simulator
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to store data
    #[arg(short = 'b', long = "appdir")]
    app_dir: Option<String>,

    /// Interface/port to listen for RPC connections (default port: 16110, testnet: 16210)
    #[arg(long = "rpclisten")]
    rpc_listen: Option<String>,
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
    // TODO: Refactor all this quick-and-dirty code
    let args = Args::parse();
    let app_dir = args
        .app_dir
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    let app_dir = if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) };
    let db_dir = app_dir.join(DEFAULT_DATA_DIR);
    assert!(!db_dir.to_str().unwrap().is_empty());
    println!("Application directory: {}", app_dir.as_display());
    println!("Data directory: {}", db_dir.as_display());
    fs::create_dir_all(db_dir.as_path()).unwrap();
    let grpc_server_addr = args.rpc_listen.unwrap_or_else(|| "[::1]:16110".to_string()).parse().unwrap();

    let genesis: Hash = blockhash::new_unique();
    let bps = 8.0;
    let delay = 2.0;

    trace!("Kaspad starting... (round-based simulation with BPS={} and D={})", bps, delay);
    trace!("\n\n ------ NOTE: this code is just a placeholder for the actual kaspad code, for an actual simulation run the simpa binary ------\n\n");

    // rayon::ThreadPoolBuilder::new()
    //     .num_threads(8)
    //     .build_global()
    //     .unwrap();

    println!("Using rayon thread pool with {} threads", rayon::current_num_threads());

    let core = Arc::new(Core::new());

    // ---

    let mut params = MAINNET_PARAMS.clone_with_skip_pow();
    params.genesis_hash = genesis;
    params.genesis_timestamp = 0;

    let db = Arc::new(DB::open_default(db_dir.to_str().unwrap()).unwrap());

    // Make sure to create the DB first, so it cleans up last
    let consensus = Arc::new(Consensus::new(db, &params));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));

    let notification_channel = ConsensusNotificationChannel::default();
    let rpc_core_service = Arc::new(RpcCoreService::new(consensus.clone(), notification_channel.receiver()));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_service));
    let async_runtime = Arc::new(AsyncRuntime::new(grpc_server));

    // Bind the keyboard signal to the emitter. The emitter will then shutdown core
    Arc::new(signals::Signals::new(&core)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(monitor);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
