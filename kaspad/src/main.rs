extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use kaspa_addressmanager::AddressManager;
use kaspa_consensus::consensus::factory::Factory as ConsensusFactory;
use kaspa_consensus::pipeline::monitor::ConsensusMonitor;
use kaspa_consensus::pipeline::ProcessingCounters;
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::errors::config::{ConfigError, ConfigResult};
use kaspa_consensus_core::networktype::NetworkType;
use kaspa_consensus_notify::root::ConsensusNotificationRoot;
use kaspa_consensus_notify::service::NotifyService;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::kaspad_env::version;
use kaspa_core::{core::Core, signals::Signals, task::runtime::AsyncRuntime};
use kaspa_index_processor::service::IndexService;
use kaspa_mining::manager::MiningManager;
use kaspa_p2p_flows::flow_context::FlowContext;
use kaspa_rpc_service::RpcCoreServer;
use kaspa_utils::networking::ContextualNetAddress;

use std::fs;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;

// ~~~
// TODO - discuss handling
use args::{Args, Defaults};
// use clap::Parser;
// ~~~

use kaspa_consensus::config::ConfigBuilder;
use kaspa_utxoindex::UtxoIndex;

use async_channel::unbounded;
use kaspa_core::{info, trace};
use kaspa_grpc_server::GrpcServer;
use kaspa_p2p_flows::service::P2pService;
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, WrpcEncoding, WrpcService};

mod args;

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus";
const UTXOINDEX_DB: &str = "utxoindex";
const META_DB: &str = "meta";
const DEFAULT_LOG_DIR: &str = "logs";

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
    return get_home_dir().join("rusty-kaspa");
    #[cfg(not(target_os = "windows"))]
    return get_home_dir().join(".rusty-kaspa");
}

fn validate_config_and_args(_config: &Arc<Config>, args: &Args) -> ConfigResult<()> {
    if !args.connect_peers.is_empty() && !args.add_peers.is_empty() {
        return Err(ConfigError::MixedConnectAndAddPeers);
    }
    if args.logdir.is_some() && args.no_log_files {
        return Err(ConfigError::MixedLogDirAndNoLogFiles);
    }
    Ok(())
}

fn get_user_approval_or_exit(message: &str, approve: bool) {
    if approve {
        return;
    }
    println!("{}", message);
    let mut input = String::new();
    match std::io::stdin().read_line(&mut input) {
        Ok(_) => {
            let lower = input.to_lowercase();
            let answer = lower.as_str().strip_suffix("\r\n").or(lower.as_str().strip_suffix('\n')).unwrap_or(lower.as_str());
            if answer == "y" || answer == "yes" {
                // return
            } else {
                println!("Operation was rejected ({}), exiting..", answer);
                exit(1);
            }
        }
        Err(error) => {
            println!("Error reading from console: {error}, exiting..");
            exit(1);
        }
    }
}

pub fn main() {
    let args = Args::parse(&Defaults::default());

    // Configure the panic behavior
    kaspa_core::panic::configure_panic();

    let network_type = match (args.testnet, args.devnet, args.simnet) {
        (false, false, false) => NetworkType::Mainnet,
        (true, false, false) => NetworkType::Testnet,
        (false, true, false) => NetworkType::Devnet,
        (false, false, true) => NetworkType::Simnet,
        _ => panic!("only a single net should be activated"),
    };

    let config = Arc::new(ConfigBuilder::new(network_type.into()).apply_args(|config| args.apply_to_config(config)).build());

    // Make sure config and args form a valid set of properties
    if let Err(err) = validate_config_and_args(&config, &args) {
        println!("{}", err);
        exit(1);
    }

    // TODO: Refactor all this quick-and-dirty code
    let app_dir = args
        .appdir
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    let app_dir = if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) };
    let db_dir = app_dir.join(config.network_name()).join(DEFAULT_DATA_DIR);

    // Logs directory is usually under the application directory, unless otherwise specified
    let log_dir = args.logdir.unwrap_or_default().replace('~', get_home_dir().as_path().to_str().unwrap());
    let log_dir = if log_dir.is_empty() { app_dir.join(config.network_name()).join(DEFAULT_LOG_DIR) } else { PathBuf::from(log_dir) };
    let log_dir = if args.no_log_files { None } else { log_dir.to_str() };

    // Initialize the logger
    kaspa_core::log::init_logger(log_dir, &args.log_level);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), version());

    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
    match log_dir {
        Some(s) => {
            info!("Logs directory: {}", s);
        }
        None => {
            info!("Logs to console only");
        }
    }

    let consensus_db_dir = db_dir.join(CONSENSUS_DB);
    let utxoindex_db_dir = db_dir.join(UTXOINDEX_DB);
    let meta_db_dir = db_dir.join(META_DB);

    if args.reset_db && db_dir.exists() {
        let msg = "Reset DB was requested -- this means the current databases will be fully deleted, 
do you confirm? (answer y/n or pass --yes to the Kaspad command line to confirm all interactive questions)";
        get_user_approval_or_exit(msg, args.yes);
        info!("Deleting databases");
        fs::remove_dir_all(db_dir.clone()).unwrap();
    }

    fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
    fs::create_dir_all(meta_db_dir.as_path()).unwrap();
    if args.utxoindex {
        info!("Utxoindex Data directory {}", utxoindex_db_dir.display());
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
    }

    // DB used for addresses store and for multi-consensus management
    let mut meta_db = kaspa_database::prelude::open_db(meta_db_dir.clone(), true, 1);

    // TEMP: upgrade from Alpha version or any version before this one
    if meta_db.get_pinned(b"multi-consensus-metadata-key").is_ok_and(|r| r.is_some()) {
        let msg = "Node database is from an older Kaspad version and needs to be fully deleted, do you confirm the delete? (y/n)";
        get_user_approval_or_exit(msg, args.yes);

        info!("Deleting databases from previous Kaspad version");

        // Drop so that deletion works
        drop(meta_db);

        // Delete
        fs::remove_dir_all(db_dir).unwrap();

        // Recreate the empty folders
        fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
        fs::create_dir_all(meta_db_dir.as_path()).unwrap();
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();

        // Reopen the DB
        meta_db = kaspa_database::prelude::open_db(meta_db_dir, true, 1);
    }

    let connect_peers = args.connect_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect::<Vec<_>>();
    let add_peers = args.add_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect();
    let p2p_server_addr = args.listen.unwrap_or(ContextualNetAddress::unspecified()).normalize(config.default_p2p_port());
    // connect_peers means no DNS seeding and no outbound peers
    let outbound_target = if connect_peers.is_empty() { args.outbound_target } else { 0 };
    let dns_seeders = if connect_peers.is_empty() { config.dns_seeders } else { &[] };

    let grpc_server_addr = args.rpclisten.unwrap_or(ContextualNetAddress::unspecified()).normalize(config.default_rpc_port());

    let core = Arc::new(Core::new());

    // ---

    let (notification_send, notification_recv) = unbounded();
    let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_send));
    let counters = Arc::new(ProcessingCounters::default());

    // Use `num_cpus` background threads for the consensus database as recommended by rocksdb
    let consensus_db_parallelism = num_cpus::get();
    let consensus_factory = Arc::new(ConsensusFactory::new(
        meta_db.clone(),
        &config,
        consensus_db_dir,
        consensus_db_parallelism,
        notification_root.clone(),
        counters.clone(),
    ));
    let consensus_manager = Arc::new(ConsensusManager::new(consensus_factory));
    let monitor = Arc::new(ConsensusMonitor::new(counters));

    let notify_service = Arc::new(NotifyService::new(notification_root.clone(), notification_recv));
    let index_service: Option<Arc<IndexService>> = if args.utxoindex {
        // Use only a single thread for none-consensus databases
        let utxoindex_db = kaspa_database::prelude::open_db(utxoindex_db_dir, true, 1);
        let utxoindex = UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap();
        let index_service = Arc::new(IndexService::new(&notify_service.notifier(), Some(utxoindex)));
        Some(index_service)
    } else {
        None
    };

    let address_manager = AddressManager::new(meta_db);
    let mining_manager = Arc::new(MiningManager::new(config.target_time_per_block, false, config.max_block_mass, None));

    let flow_context = Arc::new(FlowContext::new(
        consensus_manager.clone(),
        address_manager,
        config.clone(),
        mining_manager.clone(),
        notification_root,
    ));
    let p2p_service = Arc::new(P2pService::new(
        flow_context.clone(),
        connect_peers,
        add_peers,
        p2p_server_addr,
        outbound_target,
        args.inbound_limit,
        dns_seeders,
        config.default_p2p_port(),
    ));

    let rpc_core_server = Arc::new(RpcCoreServer::new(
        consensus_manager.clone(),
        notify_service.notifier(),
        index_service.as_ref().map(|x| x.notifier()),
        mining_manager,
        flow_context,
        index_service.as_ref().map(|x| x.utxoindex().unwrap()),
        config,
        core.clone(),
    ));
    let grpc_server = Arc::new(GrpcServer::new(grpc_server_addr, rpc_core_server.service()));

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
                    Some(rpc_core_server.service()),
                    encoding,
                    WrpcServerOptions {
                        listen_address: listen_address.to_string(), // TODO: use a normalized ContextualNetAddress instead of a String
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
    core.bind(consensus_manager);
    core.bind(async_runtime);

    core.run();

    trace!("Kaspad is finished...");
}
