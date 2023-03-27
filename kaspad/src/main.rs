extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use kaspa_addressmanager::AddressManager;
use kaspa_consensus_core::api::DynConsensus;
use kaspa_consensus_core::networktype::NetworkType;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensus_notify::service::NotifyService;

use kaspa_core::{core::Core, signals::Signals, task::runtime::AsyncRuntime};
use kaspa_index_processor::service::IndexService;
use kaspa_mining::manager::MiningManager;
use kaspa_rpc_service::RpcCoreServer;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

// ~~~
// TODO - discuss handling
use args::{Args, Defaults};
// use clap::Parser;
// any specific reason this was used?  changed as_display() to display() below
// use thiserror::__private::PathAsDisplay;
// ~~~
//use clap::Parser;

use crate::monitor::ConsensusMonitor;
use kaspa_consensus::config::ConfigBuilder;
use kaspa_consensus::consensus::Consensus;
use kaspa_consensus::params::{DEVNET_PARAMS, MAINNET_PARAMS};
use kaspa_utxoindex::{api::DynUtxoIndexApi, UtxoIndex};

use async_channel::unbounded;
use kaspa_core::{info, trace};
use kaspa_grpc_server::GrpcServer;
use kaspa_p2p_flows::service::P2pService;
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, WrpcEncoding, WrpcService};

mod args;
mod monitor;

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus";
const UTXOINDEX_DB: &str = "utxoindex";
const AMGR_DB: &str = "addressmanager";
// TODO: add a Config
// TODO: apply Args to Config
// TODO: log to file
// TODO: refactor the shutdown sequence into a predefined controlled sequence

/*
/// Kaspa Node launch arguments
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Directory to store data
    #[arg(short = 'b', long = "appdir")]
    app_dir: Option<String>,

    /// Interface/port to listen for gRPC connections (default port: 16110, testnet: 16210)
    #[arg(long = "rpclisten")]
    rpc_listen: Option<String>,

    /// Activate the utxoindex
    #[arg(long = "utxoindex")]
    utxoindex: bool,

    /// Interface/port to listen for wRPC Borsh connections (default: 127.0.0.1:17110)
    #[clap(long = "rpclisten-borsh", default_missing_value = "abc")]
    // #[arg()]
    wrpc_listen_borsh: Option<String>,

    /// Interface/port to listen for wRPC JSON connections (default: 127.0.0.1:18110)
    #[arg(long = "rpclisten-json")]
    wrpc_listen_json: Option<String>,

    /// Enable verbose logging of wRPC data exchange
    #[arg(long = "wrpc-verbose")]
    wrpc_verbose: bool,

    /// Logging level for all subsystems {off, error, warn, info, debug, trace}
    ///  -- You may also specify <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for individual subsystems
    #[arg(short = 'd', long = "loglevel", default_value = "info")]
    log_level: String,

    #[arg(long = "connect")]
    connect: Option<String>,

    #[arg(long = "listen")]
    listen: Option<String>,

    #[arg(long = "reset-db")]
    reset_db: bool,

    #[arg(long = "outpeers", default_value = "8")]
    target_outbound: usize,

    #[arg(long = "maxinpeers", default_value = "128")]
    inbound_limit: usize,

    #[arg(long = "testnet")]
    testnet: bool,

    #[arg(long = "devnet")]
    devnet: bool,

    #[arg(long = "simnet")]
    simnet: bool,
}
 */

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
    let defaults = Defaults {
        // --async-threads N
        async_threads: num_cpus::get() / 2,
        ..Defaults::default()
    };

    let args = Args::parse(&defaults);

    // Initialize the logger
    kaspa_core::log::init_logger(&args.log_level);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));

    // Configure the panic behavior
    kaspa_core::panic::configure_panic();

    let network_type = match (args.testnet, args.devnet, args.simnet) {
        (false, false, false) => NetworkType::Mainnet,
        (true, false, false) => NetworkType::Testnet,
        (false, true, false) => NetworkType::Devnet,
        (false, false, true) => NetworkType::Simnet,
        _ => panic!("only a single net should be activated"),
    };

    // TODO: Refactor all this quick-and-dirty code
    let app_dir = args
        .appdir
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    let app_dir = if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) };
    let db_dir = app_dir.join(format!("kaspa-{}", network_type)).join(DEFAULT_DATA_DIR); // TODO: append testnet number

    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());

    let consensus_db_dir = db_dir.join(CONSENSUS_DB);
    let utxoindex_db_dir = db_dir.join(UTXOINDEX_DB);
    let amgr_db_dir = db_dir.join(AMGR_DB);

    if args.reset_db {
        // TODO: add prompt that validates the choice (unless you pass -y)
        info!("Deleting databases {:?}, {:?}", consensus_db_dir, utxoindex_db_dir);
        kaspa_database::prelude::delete_db(consensus_db_dir.clone());
        kaspa_database::prelude::delete_db(utxoindex_db_dir.clone());
        kaspa_database::prelude::delete_db(amgr_db_dir.clone());
    }

    info!("Consensus Data directory {}", consensus_db_dir.display());
    fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
    if args.utxoindex {
        info!("Utxoindex Data directory {}", utxoindex_db_dir.display());
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
    }

    let grpc_server_addr = args.rpclisten.unwrap_or_else(|| "127.0.0.1:16610".to_string()).parse().unwrap();

    let core = Arc::new(Core::new());

    // ---

    let config = match network_type {
        // TODO: TEMP, until staging consensus is managed, skip adding genesis on mainnet
        NetworkType::Mainnet => ConfigBuilder::new(MAINNET_PARAMS).skip_adding_genesis().build(),
        NetworkType::Testnet => unimplemented!("testnet params"),
        NetworkType::Devnet => ConfigBuilder::new(DEVNET_PARAMS).build(),
        NetworkType::Simnet => unimplemented!("simnet params"),
    };

    let (notification_send, notification_recv) = unbounded();
    let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_send));

    // Use `num_cpus` background threads for the consensus database as recommended by rocksdb
    let consensus_db = kaspa_database::prelude::open_db(consensus_db_dir, true, num_cpus::get());
    let consensus = Arc::new(Consensus::new(consensus_db, &config, notification_root));
    let monitor = Arc::new(ConsensusMonitor::new(consensus.processing_counters().clone()));

    let notify_service = Arc::new(NotifyService::new(consensus.notification_root(), notification_recv));

    let index_service: Option<Arc<IndexService>> = if args.utxoindex {
        // Use only a single thread for none-consensus databases
        let utxoindex_db = kaspa_database::prelude::open_db(utxoindex_db_dir, true, 1);
        let utxoindex: DynUtxoIndexApi = Some(UtxoIndex::new(consensus.clone(), utxoindex_db).unwrap());
        Some(Arc::new(IndexService::new(&notify_service.notifier(), utxoindex)))
    } else {
        None
    };

    let amgr_db = kaspa_database::prelude::open_db(amgr_db_dir, true, 1);
    let amgr = AddressManager::new(amgr_db);

    let rpc_core_server =
        Arc::new(RpcCoreServer::new(consensus.clone(), notify_service.notifier(), index_service.as_ref().map(|x| x.notifier())));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_server.service()));
    let p2p_service = Arc::new(P2pService::new(
        consensus.clone(),
        amgr,
        &config,
        args.connect,
        args.listen,
        args.outbound_target,
        args.inbound_limit,
    ));

    // TODO: TEMP: temp mining manager initialization just to make sure it complies with consensus
    let _mining_manager =
        MiningManager::new(consensus.clone() as DynConsensus, config.target_time_per_block, false, config.max_block_mass, None);

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new(args.async_threads));
    async_runtime.register(notify_service);
    if let Some(index_service) = index_service {
        async_runtime.register(index_service)
    };
    async_runtime.register(rpc_core_server.clone());
    async_runtime.register(grpc_server);
    async_runtime.register(p2p_service);
    async_runtime.register(monitor);

    let wrpc_service_tasks: usize = 2; // num_cpus::get() / 2;
                                       // Register wRPC servers based on command line arguments
    [(args.rpclisten_borsh, WrpcEncoding::Borsh), (args.rpclisten_json, WrpcEncoding::SerdeJson)]
        .iter()
        .filter_map(|(listen_address, encoding)| {
            listen_address.as_ref().map(|listen_address| {
                Arc::new(WrpcService::new(
                    wrpc_service_tasks,
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
        .for_each(|server| async_runtime.register(server));

    // Bind the keyboard signal to the core
    Arc::new(Signals::new(&core)).init();

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
