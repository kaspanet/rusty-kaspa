extern crate consensus;
extern crate core;
extern crate hashes;

use clap::Parser;
use consensus::config::Config;
use consensus::model::stores::DB;
use consensus_core::events::ConsensusEvent;
use event_processor::notify::Notification;

use kaspa_core::{core::Core, signals::Signals, task::runtime::AsyncRuntime};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::monitor::ConsensusMonitor;
use consensus::consensus::Consensus;
use consensus::params::DEVNET_PARAMS;
use utxoindex::{
    api::{DynUtxoIndexControllerApi, DynUtxoIndexRetrievalApi},
    UtxoIndex,
};

use async_channel::unbounded;
use event_processor::processor::EventProcessor;
use kaspa_core::{info, trace};
use p2p_flows::service::P2pService;
use rpc_core::server::RpcCoreServer;
use rpc_grpc::server::GrpcServer;

mod monitor;

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus-db";
const UTXOINDEX_DB: &str = "utxoindex-db";
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

    /// Activate the utxoindex
    #[arg(long = "utxoindex")]
    utxoindex: Option<String>,

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
    let consensus_db_dir = app_dir.join(CONSENSUS_DB);
    let utxoindex_db_dir = app_dir.join(UTXOINDEX_DB);
    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
    info!("Consensus Data directory {}", consensus_db_dir.display());
    fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
    if args.utxoindex.is_some() {
        info!("Utxoindex Data directory {}", utxoindex_db_dir.display());
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
    }

    let grpc_server_addr = args.rpc_listen.unwrap_or_else(|| "127.0.0.1:16610".to_string()).parse().unwrap();

    let core = Arc::new(Core::new());

    // ---

    let config = Config::new(DEVNET_PARAMS); // TODO: network type

    let (consensus_send, consensus_recv) = unbounded::<ConsensusEvent>();
    let (event_processor_send, event_processor_recv) = unbounded::<Notification>();

    let consensus_db = Arc::new(DB::open_default(consensus_db_dir.to_str().unwrap()).unwrap());
    let consensus = Arc::new(Consensus::new(consensus_db, &config, consensus_send));

    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));

    let (utxoindex_controller, utxoindex_retrieval_api): (DynUtxoIndexControllerApi, DynUtxoIndexRetrievalApi) =
        match args.utxoindex.is_some() {
            true => {
                let utxoindex_db = Arc::new(DB::open_default(utxoindex_db_dir.to_str().unwrap()).unwrap());
                let utxoindex = UtxoIndex::new(consensus.clone(), utxoindex_db);
                (Arc::new(Some(Box::new(utxoindex.clone()))), Arc::new(Some(Box::new(utxoindex))))
            }
            false => (Arc::new(None), Arc::new(None)),
        };

    let event_processor = Arc::new(EventProcessor::new(utxoindex_controller, consensus_recv, event_processor_send));

    let rpc_core_server = Arc::new(RpcCoreServer::new(consensus.clone(), utxoindex_retrieval_api, event_processor_recv));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_server.service()));
    let p2p_service = Arc::new(P2pService::new(consensus.clone()));

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new());
    async_runtime.register(event_processor);
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
