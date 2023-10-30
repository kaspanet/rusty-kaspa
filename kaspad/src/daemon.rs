use std::{fs, path::PathBuf, process::exit, sync::Arc, time::Duration};

use async_channel::unbounded;
use kaspa_consensus_core::{
    config::ConfigBuilder,
    errors::config::{ConfigError, ConfigResult},
};
use kaspa_consensus_notify::{root::ConsensusNotificationRoot, service::NotifyService};
use kaspa_core::{core::Core, info, trace};
use kaspa_core::{kaspad_env::version, task::tick::TickService};
use kaspa_grpc_server::service::GrpcService;
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_utils::networking::ContextualNetAddress;

use kaspa_addressmanager::AddressManager;
use kaspa_consensus::{consensus::factory::Factory as ConsensusFactory, pipeline::ProcessingCounters};
use kaspa_consensus::{consensus::factory::MultiConsensusManagementStore, pipeline::monitor::ConsensusMonitor};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::runtime::AsyncRuntime;
use kaspa_index_processor::service::IndexService;
use kaspa_mining::{
    manager::{MiningManager, MiningManagerProxy},
    monitor::MiningMonitor,
    MiningCounters,
};
use kaspa_p2p_flows::{flow_context::FlowContext, service::P2pService};

use kaspa_perf_monitor::builder::Builder as PerfMonitorBuilder;
use kaspa_utils::fd_budget::acquire_guard;
use kaspa_utils::tcp_limiter::Limit;
use kaspa_utxoindex::{api::UtxoIndexProxy, UtxoIndex};
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, ServerCounters as WrpcServerCounters, WrpcEncoding, WrpcService};

use crate::args::Args;

const DEFAULT_DATA_DIR: &str = "datadir";
const CONSENSUS_DB: &str = "consensus";
const UTXOINDEX_DB: &str = "utxoindex";
const META_DB: &str = "meta";
const META_DB_FILE_LIMIT: i32 = 5;
const DEFAULT_LOG_DIR: &str = "logs";

fn get_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return dirs::data_local_dir().unwrap();
    #[cfg(not(target_os = "windows"))]
    return dirs::home_dir().unwrap();
}

/// Get the default application directory.
pub fn get_app_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    return get_home_dir().join("rusty-kaspa");
    #[cfg(not(target_os = "windows"))]
    return get_home_dir().join(".rusty-kaspa");
}

pub fn validate_args(args: &Args) -> ConfigResult<()> {
    #[cfg(feature = "devnet-prealloc")]
    {
        if args.num_prealloc_utxos.is_some() && !(args.devnet || args.simnet) {
            return Err(ConfigError::PreallocUtxosOnNonDevnet);
        }

        if args.prealloc_address.is_some() ^ args.num_prealloc_utxos.is_some() {
            return Err(ConfigError::MissingPreallocNumOrAddress);
        }
    }

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

/// Runtime configuration struct for the application.
#[derive(Default)]
pub struct Runtime {
    log_dir: Option<String>,
}

/// Get the application directory from the supplied [`Args`].
/// This function can be used to identify the location of
/// the application folder that contains kaspad logs and the database.
pub fn get_app_dir_from_args(args: &Args) -> PathBuf {
    let app_dir = args
        .appdir
        .clone()
        .unwrap_or_else(|| get_app_dir().as_path().to_str().unwrap().to_string())
        .replace('~', get_home_dir().as_path().to_str().unwrap());
    if app_dir.is_empty() {
        get_app_dir()
    } else {
        PathBuf::from(app_dir)
    }
}

/// Get the log directory from the supplied [`Args`].
pub fn get_log_dir(args: &Args) -> Option<String> {
    let network = args.network();
    let app_dir = get_app_dir_from_args(args);

    // Logs directory is usually under the application directory, unless otherwise specified
    let log_dir = args.logdir.clone().unwrap_or_default().replace('~', get_home_dir().as_path().to_str().unwrap());
    let log_dir = if log_dir.is_empty() { app_dir.join(network.to_prefixed()).join(DEFAULT_LOG_DIR) } else { PathBuf::from(log_dir) };
    let log_dir = if args.no_log_files { None } else { log_dir.to_str().map(String::from) };
    log_dir
}

impl Runtime {
    pub fn from_args(args: &Args) -> Self {
        // Configure the panic behavior
        kaspa_core::panic::configure_panic();

        let log_dir = get_log_dir(args);

        // Initialize the logger
        kaspa_core::log::init_logger(log_dir.as_deref(), &args.log_level);

        Self { log_dir: log_dir.map(|log_dir| log_dir.to_owned()) }
    }
}

/// Create [`Core`] instance with supplied [`Args`].
/// This function will automatically create a [`Runtime`]
/// instance with the supplied [`Args`] and then
/// call [`create_core_with_runtime`].
///
/// Usage semantics:
/// `let (core, rpc_core_service) = create_core(args);`
///
/// The instance of the [`RpcCoreService`] needs to be released
/// (dropped) before the `Core` is shut down.
///
pub fn create_core(args: Args, fd_total_budget: i32) -> (Arc<Core>, Arc<RpcCoreService>) {
    let rt = Runtime::from_args(&args);
    create_core_with_runtime(&rt, &args, fd_total_budget)
}

/// Create [`Core`] instance with supplied [`Args`] and [`Runtime`].
///
/// Usage semantics:
/// ```ignore
/// let Runtime = Runtime::from_args(&args); // or create your own
/// let (core, rpc_core_service) = create_core(&runtime, &args);
/// ```
///
/// The instance of the [`RpcCoreService`] needs to be released
/// (dropped) before the `Core` is shut down.
///
pub fn create_core_with_runtime(runtime: &Runtime, args: &Args, fd_total_budget: i32) -> (Arc<Core>, Arc<RpcCoreService>) {
    let network = args.network();
    let mut fd_remaining = fd_total_budget;
    let tcp_limit = if let Some(tcp_limit) = args.max_tcp_connections {
        tcp_limit as i32
    } else {
        args.rpc_max_clients as i32 - args.inbound_limit as i32 - args.outbound_target as i32
    };
    let Ok(_tcp_limit_guard) = acquire_guard(tcp_limit) else {
        println!("Oops! Looks like we've hit a system limit. You have a couple of options:");
        println!("1. Increase the file limit for this process. If you're on Linux, you can do this with the 'ulimit' command.");
        println!("2. Reduce the TCP connection limit by passing the `max-tcp-conns` argument when starting this program.");
        exit(1);
    };
    fd_remaining -= tcp_limit;
    let utxo_files_limit = if args.utxoindex {
        let utxo_files_limit = fd_remaining * 10 / 100;
        fd_remaining -= utxo_files_limit;
        utxo_files_limit
    } else {
        0
    };
    // Make sure args forms a valid set of properties
    if let Err(err) = validate_args(args) {
        println!("{}", err);
        exit(1);
    }

    let tcp_limit = Some(Arc::new(Limit::new(tcp_limit)));
    let config = Arc::new(
        ConfigBuilder::new(network.into())
            .adjust_perf_params_to_consensus_params()
            .apply_args(|config| args.apply_to_config(config))
            .build(),
    );

    // TODO: Validate `config` forms a valid set of properties

    let app_dir = get_app_dir_from_args(args);
    let db_dir = app_dir.join(network.to_prefixed()).join(DEFAULT_DATA_DIR);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), version());

    assert!(!db_dir.to_str().unwrap().is_empty());
    info!("Application directory: {}", app_dir.display());
    info!("Data directory: {}", db_dir.display());
    match runtime.log_dir.as_ref() {
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
        fs::remove_dir_all(&db_dir).unwrap();
    }

    fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
    fs::create_dir_all(meta_db_dir.as_path()).unwrap();
    if args.utxoindex {
        info!("Utxoindex Data directory {}", utxoindex_db_dir.display());
        fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
    }

    // DB used for addresses store and for multi-consensus management
    let mut meta_db = kaspa_database::prelude::ConnBuilder::default()
        .with_db_path(meta_db_dir.clone())
        .with_files_limit(META_DB_FILE_LIMIT)
        .build()
        .unwrap();

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
        meta_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(meta_db_dir)
            .with_files_limit(META_DB_FILE_LIMIT)
            .build()
            .unwrap();
    }

    if !args.archival && MultiConsensusManagementStore::new(meta_db.clone()).is_archival_node().unwrap() {
        get_user_approval_or_exit("--archival is set to false although the node was previously archival. Proceeding may delete archived data. Do you confirm? (y/n)", args.yes);
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

    let tick_service = Arc::new(TickService::new());
    let (notification_send, notification_recv) = unbounded();
    let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_send));
    let processing_counters = Arc::new(ProcessingCounters::default());
    let mining_counters = Arc::new(MiningCounters::default());
    let wrpc_borsh_counters = Arc::new(WrpcServerCounters::default());
    let wrpc_json_counters = Arc::new(WrpcServerCounters::default());
    let tx_script_cache_counters = Arc::new(TxScriptCacheCounters::default());

    // Use `num_cpus` background threads for the consensus database as recommended by rocksdb
    let consensus_db_parallelism = num_cpus::get();
    let consensus_factory = Arc::new(ConsensusFactory::new(
        meta_db.clone(),
        &config,
        consensus_db_dir,
        consensus_db_parallelism,
        notification_root.clone(),
        processing_counters.clone(),
        tx_script_cache_counters.clone(),
        fd_remaining,
    ));
    let consensus_manager = Arc::new(ConsensusManager::new(consensus_factory));
    let consensus_monitor = Arc::new(ConsensusMonitor::new(processing_counters.clone(), tick_service.clone()));

    let perf_monitor_builder = PerfMonitorBuilder::new()
        .with_fetch_interval(Duration::from_secs(args.perf_metrics_interval_sec))
        .with_tick_service(tick_service.clone());
    let perf_monitor = if args.perf_metrics {
        let cb = move |counters| {
            trace!("[{}] metrics: {:?}", kaspa_perf_monitor::SERVICE_NAME, counters);
            #[cfg(feature = "heap")]
            trace!("[{}] heap stats: {:?}", kaspa_perf_monitor::SERVICE_NAME, dhat::HeapStats::get());
        };
        Arc::new(perf_monitor_builder.with_fetch_cb(cb).build())
    } else {
        Arc::new(perf_monitor_builder.build())
    };

    let notify_service = Arc::new(NotifyService::new(notification_root.clone(), notification_recv));
    let index_service: Option<Arc<IndexService>> = if args.utxoindex {
        // Use only a single thread for none-consensus databases
        let utxoindex_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(utxoindex_db_dir)
            .with_files_limit(utxo_files_limit)
            .build()
            .unwrap();
        let utxoindex = UtxoIndexProxy::new(UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap());
        let index_service = Arc::new(IndexService::new(&notify_service.notifier(), Some(utxoindex)));
        Some(index_service)
    } else {
        None
    };

    let (address_manager, port_mapping_extender_svc) = AddressManager::new(config.clone(), meta_db, tick_service.clone());

    let mining_monitor = Arc::new(MiningMonitor::new(mining_counters.clone(), tx_script_cache_counters.clone(), tick_service.clone()));
    let mining_manager = MiningManagerProxy::new(Arc::new(MiningManager::new_with_spam_blocking_option(
        network.is_mainnet(),
        config.target_time_per_block,
        false,
        config.max_block_mass,
        config.block_template_cache_lifetime,
        mining_counters,
    )));

    let flow_context = Arc::new(FlowContext::new(
        consensus_manager.clone(),
        address_manager,
        config.clone(),
        mining_manager.clone(),
        tick_service.clone(),
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
        tcp_limit.clone(),
    ));

    let rpc_core_service = Arc::new(RpcCoreService::new(
        consensus_manager.clone(),
        notify_service.notifier(),
        index_service.as_ref().map(|x| x.notifier()),
        mining_manager,
        flow_context,
        index_service.as_ref().map(|x| x.utxoindex().unwrap()),
        config,
        core.clone(),
        processing_counters,
        wrpc_borsh_counters.clone(),
        wrpc_json_counters.clone(),
        perf_monitor.clone(),
    ));
    let grpc_service = Arc::new(GrpcService::new(grpc_server_addr, rpc_core_service.clone(), args.rpc_max_clients, tcp_limit.clone()));

    // Create an async runtime and register the top-level async services
    let async_runtime = Arc::new(AsyncRuntime::new(args.async_threads));
    async_runtime.register(tick_service);
    async_runtime.register(notify_service);
    if let Some(index_service) = index_service {
        async_runtime.register(index_service)
    };
    if let Some(port_mapping_extender_svc) = port_mapping_extender_svc {
        async_runtime.register(Arc::new(port_mapping_extender_svc))
    };
    async_runtime.register(rpc_core_service.clone());
    async_runtime.register(grpc_service);
    async_runtime.register(p2p_service);
    async_runtime.register(consensus_monitor);
    async_runtime.register(mining_monitor);
    async_runtime.register(perf_monitor);
    let wrpc_service_tasks: usize = 2; // num_cpus::get() / 2;
                                       // Register wRPC servers based on command line arguments
    [
        (args.rpclisten_borsh.clone(), WrpcEncoding::Borsh, wrpc_borsh_counters),
        (args.rpclisten_json.clone(), WrpcEncoding::SerdeJson, wrpc_json_counters),
    ]
    .into_iter()
    .filter_map(|(listen_address, encoding, wrpc_server_counters)| {
        listen_address.map(|listen_address| {
            Arc::new(WrpcService::new(
                wrpc_service_tasks,
                Some(rpc_core_service.clone()),
                &encoding,
                wrpc_server_counters,
                WrpcServerOptions {
                    listen_address: listen_address.to_address(&network.network_type, &encoding).to_string(), // TODO: use a normalized ContextualNetAddress instead of a String
                    verbose: args.wrpc_verbose,
                    tcp_limit: tcp_limit.clone(),
                    ..WrpcServerOptions::default()
                },
            ))
        })
    })
    .for_each(|server| async_runtime.register(server));

    // Consensus must start first in order to init genesis in stores
    core.bind(consensus_manager);
    core.bind(async_runtime);

    (core, rpc_core_service)
}
