use std::{fs, path::PathBuf, process::exit, sync::Arc, time::Duration};

use async_channel::unbounded;
use kaspa_connectionmanager::EVENT_LOOP_TIMER;
use kaspa_consensus_core::{
    config::ConfigBuilder,
    constants::TRANSIENT_BYTE_TO_MASS_FACTOR,
    errors::config::{ConfigError, ConfigResult},
    mining_rules::MiningRules,
};
use kaspa_consensus_notify::{root::ConsensusNotificationRoot, service::NotifyService};
use kaspa_core::{core::Core, debug, info};
use kaspa_core::{kaspad_env::version, task::tick::TickService};
use kaspa_database::{
    prelude::{CachePolicy, DbWriter, DirectDbWriter, RocksDbPreset},
    registry::DatabaseStorePrefixes,
};
use kaspa_grpc_server::service::GrpcService;
use kaspa_notify::{address::tracker::Tracker, subscription::context::SubscriptionContext};
use kaspa_p2p_lib::Hub;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use kaspa_perigeemanager::{PerigeeConfig, PerigeeManager};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_txscript::caches::TxScriptCacheCounters;
use kaspa_utils::git;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_utils::sysinfo::SystemInfo;
use kaspa_utils_tower::counters::TowerConnectionCounters;

use kaspa_addressmanager::AddressManager;
use kaspa_consensus::{
    consensus::factory::MultiConsensusManagementStore, model::stores::headers::DbHeadersStore, pipeline::monitor::ConsensusMonitor,
};
use kaspa_consensus::{
    consensus::factory::{Factory as ConsensusFactory, LATEST_DB_VERSION},
    params::{OverrideParams, Params},
    pipeline::ProcessingCounters,
};
use kaspa_consensusmanager::ConsensusManager;
use kaspa_core::task::runtime::AsyncRuntime;
use kaspa_index_processor::service::IndexService;
use kaspa_mining::{
    MiningCounters,
    manager::{MiningManager, MiningManagerProxy},
    monitor::MiningMonitor,
};
use kaspa_p2p_flows::{flow_context::FlowContext, service::P2pService};

use kaspa_perf_monitor::{builder::Builder as PerfMonitorBuilder, counters::CountersSnapshot};
use kaspa_utxoindex::{UtxoIndex, api::UtxoIndexProxy};
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, WebSocketCounters as WrpcServerCounters, WrpcEncoding, WrpcService};

/// Desired soft FD limit that needs to be configured
/// for the kaspad process.
pub const DESIRED_DAEMON_SOFT_FD_LIMIT: u64 = 8 * 1024;
/// Minimum acceptable soft FD limit for the kaspad
/// process. (Rusty Kaspa will operate with the minimal
/// acceptable limit of `4096`, but a setting below
/// this value may impact the database performance).
pub const MINIMUM_DAEMON_SOFT_FD_LIMIT: u64 = 4 * 1024;

/// If set, the retention period days must be at least this value
/// (otherwise it is meaningless since pruning periods are typically at least 2 days long)
const MINIMUM_RETENTION_PERIOD_DAYS: f64 = 2.0;
const ONE_GIGABYTE: f64 = 1_000_000_000.0;

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
    if args.ram_scale < 0.1 {
        return Err(ConfigError::RamScaleTooLow);
    }
    if args.ram_scale > 10.0 {
        return Err(ConfigError::RamScaleTooHigh);
    }
    if args.max_tracked_addresses > Tracker::MAX_ADDRESS_UPPER_BOUND {
        return Err(ConfigError::MaxTrackedAddressesTooHigh(Tracker::MAX_ADDRESS_UPPER_BOUND));
    }
    Ok(())
}

fn request_database_deletion_approval(approve: bool) -> bool {
    let msg = "Node database is from a different Kaspad *DB* version and needs to be fully deleted, do you confirm the delete? (y/n)";
    get_user_approval_or_exit(msg, approve);
    info!("Deleting databases from previous Kaspad version");
    true // if consensus not exited, always return true
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
    if app_dir.is_empty() { get_app_dir() } else { PathBuf::from(app_dir) }
}

/// Get the log directory from the supplied [`Args`].
pub fn get_log_dir(args: &Args) -> Option<String> {
    let network = args.network();
    let app_dir = get_app_dir_from_args(args);

    // Logs directory is usually under the application directory, unless otherwise specified
    let log_dir = args.logdir.clone().unwrap_or_default().replace('~', get_home_dir().as_path().to_str().unwrap());
    let log_dir = if log_dir.is_empty() { app_dir.join(network.to_prefixed()).join(DEFAULT_LOG_DIR) } else { PathBuf::from(log_dir) };

    if args.no_log_files { None } else { log_dir.to_str().map(String::from) }
}

impl Runtime {
    pub fn from_args(args: &Args) -> Self {
        let log_dir = get_log_dir(args);

        // Initialize the logger
        cfg_if::cfg_if! {
            if #[cfg(feature = "semaphore-trace")] {
                kaspa_core::log::init_logger(log_dir.as_deref(), &format!("{},{}=debug", args.log_level, kaspa_utils::sync::semaphore_module_path()));
            } else {
                kaspa_core::log::init_logger(log_dir.as_deref(), &args.log_level);
            }
        };

        // Configure the panic behavior
        // As we log the panic, we want to set it up after the logger
        kaspa_core::panic::configure_panic();

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

/// Configure RocksDB parameters from CLI arguments.
///
/// Returns: (preset, cache_budget, wal_directory)
fn configure_rocksdb(args: &Args) -> (RocksDbPreset, Option<usize>, Option<PathBuf>) {
    // Parse preset
    let preset = if let Some(preset_str) = &args.rocksdb_preset {
        match preset_str.parse::<RocksDbPreset>() {
            Ok(p) => {
                info!("Using RocksDB preset: {} - {}", p, p.description());
                info!("  Use case: {}", p.use_case());
                info!("  Memory requirements: {}", p.memory_requirements());
                p
            }
            Err(err) => {
                println!("Invalid RocksDB preset: {}", err);
                exit(1);
            }
        }
    } else {
        RocksDbPreset::Default
    };

    // Calculate cache budget for HDD preset
    let cache_budget = if matches!(preset, RocksDbPreset::Hdd) {
        if let Some(cache_mb) = args.rocksdb_cache_size {
            let cache_bytes = cache_mb * 1024 * 1024;
            info!("Custom RocksDB cache size: {} MB", cache_mb);
            Some(cache_bytes)
        } else {
            let base_cache = 256 * 1024 * 1024;
            let scaled_cache = (base_cache as f64 * args.ram_scale) as usize;
            let min_cache = 64 * 1024 * 1024;
            let final_cache = scaled_cache.max(min_cache);
            info!("RocksDB cache size: {} MB (scaled by ram-scale)", final_cache / 1024 / 1024);
            Some(final_cache)
        }
    } else {
        None
    };

    // Setup WAL directory if specified
    let wal_dir = args.rocksdb_wal_dir.as_ref().map(|custom_wal_dir| {
        let wal_path = PathBuf::from(custom_wal_dir);
        info!("Custom WAL directory: {}", wal_path.display());
        wal_path
    });

    (preset, cache_budget, wal_dir)
}

/// Create Perigee configuration from CLI arguments.
/// Panics if the supplied arguments are considered invalid.
fn create_perigee_config(
    outbound_target: usize,
    perigee_target: usize,
    round_duration: usize,
    leverage_target: usize,
    exploration_target: usize,
    persistence: bool,
    statistics: bool,
    network_bps: u64,
) -> PerigeeConfig {
    assert!(
        perigee_target <= outbound_target,
        "Perigee target of {} cannot exceed total outbound target of {}",
        perigee_target,
        outbound_target
    );

    // We only perform within at [`EVENT_LOOP_TIMER`] granularity,
    let round_granularity = EVENT_LOOP_TIMER.as_secs() as usize;
    let min_duration = round_granularity;
    let max_duration = 300;

    assert!(min_duration - round_granularity == 0, "Min perigee round duration be at least the event loop timer granularity");
    assert!(
        max_duration % round_granularity == 0,
        "Max perigee round duration must be a multiple of the event loop timer granularity"
    );

    // clamp to valid ranges (>300 seconds is not allowed, as to limit excessive data accumulation), (<30 seconds is under the bounds set by the [`EVENT_LOOP_TIMER`] interval)
    let round_duration = round_duration.clamp(min_duration, max_duration);
    // We only perform within at [`EVENT_LOOP_TIMER`] granularity, so we round the duration to the nearest multiple of it
    let round_duration = (round_duration as f64 / round_granularity as f64) as usize * round_granularity;

    let leverage_target = if leverage_target == 0 {
        // Apply default to 50% of total target
        perigee_target / 2 // integer division rounds down by default
    } else {
        leverage_target
    };
    let exploration_target = if exploration_target == 0 {
        // Apply default to 25% of total target
        perigee_target / 4 // integer division rounds down by default
    } else {
        exploration_target
    };

    // assert valid targets
    assert!(
        (leverage_target + exploration_target) <= perigee_target,
        "{}",
        format!(
            "Leverage target of {0} Plus the Exploration target of {1} cannot exceed the Total perigee target of {2}",
            leverage_target, exploration_target, perigee_target
        )
    );

    PerigeeConfig::new(
        perigee_target,
        leverage_target,
        exploration_target,
        round_duration,
        EVENT_LOOP_TIMER,
        statistics,
        persistence,
        network_bps,
    )
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
    let utxo_files_limit = if args.utxoindex {
        let utxo_files_limit = fd_remaining / 10;
        fd_remaining -= utxo_files_limit;
        utxo_files_limit
    } else {
        0
    };

    // Configure RocksDB parameters
    let (rocksdb_preset, cache_budget, wal_dir) = configure_rocksdb(args);

    // Make sure args forms a valid set of properties
    if let Err(err) = validate_args(args) {
        println!("{}", err);
        exit(1);
    }

    let params = {
        let params: Params = network.into();
        match &args.override_params_file {
            Some(path) => {
                if network.is_mainnet() {
                    println!("Overriding params on mainnet is not allowed.");
                    exit(1);
                }

                let file_content = fs::read_to_string(path).unwrap_or_else(|err| {
                    println!("Failed to read override params file '{}': {}", path, err);
                    exit(1);
                });
                let override_params: OverrideParams = serde_json::from_str(&file_content).unwrap_or_else(|err| {
                    println!("Failed to parse override params file '{}': {}", path, err);
                    exit(1);
                });
                params.override_params(override_params)
            }
            None => params,
        }
    };

    let config = Arc::new(
        ConfigBuilder::new(params).adjust_perf_params_to_consensus_params().apply_args(|config| args.apply_to_config(config)).build(),
    );

    let app_dir = get_app_dir_from_args(args);
    let db_dir = app_dir.join(network.to_prefixed()).join(DEFAULT_DATA_DIR);

    // Print package name and version
    info!("{} v{}", env!("CARGO_PKG_NAME"), git::with_short_hash(version()));

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

    let mut is_db_reset_needed = args.reset_db;

    // Reset Condition: User explicitly requested a reset
    if is_db_reset_needed && db_dir.exists() {
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

    if !args.archival
        && let Some(retention_period_days) = args.retention_period_days
    {
        // Look only at post-fork values (which are the worst-case)
        let finality_depth = config.finality_depth();
        let target_time_per_block = config.target_time_per_block(); // in ms

        let retention_period_milliseconds = (retention_period_days * 24.0 * 60.0 * 60.0 * 1000.0).ceil() as u64;
        if MINIMUM_RETENTION_PERIOD_DAYS <= retention_period_days {
            let total_blocks = retention_period_milliseconds / target_time_per_block;
            // This worst case usage only considers block space. It does not account for usage of
            // other stores (reachability, block status, mempool, etc.)
            let worst_case_usage =
                ((total_blocks + finality_depth) * (config.max_block_mass / TRANSIENT_BYTE_TO_MASS_FACTOR)) as f64 / ONE_GIGABYTE;

            info!(
                "Retention period is set to {} days. Disk usage may be up to {:.2} GB for block space required for this period.",
                retention_period_days, worst_case_usage
            );
        } else {
            panic!("Retention period ({}) must be at least {} days", retention_period_days, MINIMUM_RETENTION_PERIOD_DAYS);
        }
    }

    // DB used for addresses store and for multi-consensus management
    let mut meta_db = kaspa_database::prelude::ConnBuilder::default()
        .with_db_path(meta_db_dir.clone())
        .with_files_limit(META_DB_FILE_LIMIT)
        .with_preset(rocksdb_preset)
        .with_wal_dir(wal_dir.clone())
        .with_cache_budget(cache_budget)
        .build()
        .unwrap();

    // Reset Condition: Need to reset DB if we can't find genesis in current DB
    if !is_db_reset_needed && (args.testnet || args.devnet || args.simnet) {
        // Non-mainnet can be restarted, and when it does we need to reset the DB.
        // This will check if the current Genesis can be found the active consensus
        // DB (if one exists), and if not then ask to reset the DB.
        let active_consensus_dir_name = MultiConsensusManagementStore::new(meta_db.clone()).active_consensus_dir_name().unwrap();

        match active_consensus_dir_name {
            Some(dir_name) => {
                let consensus_db = kaspa_database::prelude::ConnBuilder::default()
                    .with_db_path(consensus_db_dir.clone().join(dir_name))
                    .with_files_limit(1)
                    .with_preset(rocksdb_preset)
                    .with_wal_dir(wal_dir.clone())
                    .with_cache_budget(cache_budget)
                    .build()
                    .unwrap();

                let headers_store = DbHeadersStore::new(consensus_db, CachePolicy::Empty, CachePolicy::Empty);

                if headers_store.has(config.genesis.hash).unwrap() {
                    debug!("Genesis is found in active consensus DB. No action needed.");
                } else {
                    let msg = "Genesis not found in active consensus DB. This happens when Testnets are restarted and your database needs to be fully deleted. Do you confirm the delete? (y/n)";
                    get_user_approval_or_exit(msg, args.yes);

                    is_db_reset_needed = true;
                }
            }
            None => {
                debug!("Consensus not initialized yet. Skipping genesis check.");
            }
        }
    }

    // Reset Condition: Need to reset if we're upgrading from kaspad DB version
    // TEMP: upgrade from Alpha version or any version before this one
    'db_upgrade: while !is_db_reset_needed
        && (meta_db.get_pinned(b"multi-consensus-metadata-key").is_ok_and(|r| r.is_some())
            || MultiConsensusManagementStore::new(meta_db.clone()).should_upgrade().unwrap())
    {
        let mut mcms = MultiConsensusManagementStore::new(meta_db.clone());
        let version = mcms.version().unwrap();

        if version <= 3 {
            is_db_reset_needed = request_database_deletion_approval(args.yes);
            continue 'db_upgrade;
        }

        let msg = "NOTE: Node database is from an older version. Proceeding with the upgrade is instant and safe.
However, downgrading to an older node version later will require deleting the database.
Do you confirm? (y/n)";
        get_user_approval_or_exit(msg, args.yes);
        if version <= 4 {
            mcms.set_version(5).unwrap();
        }
        if version <= 5 {
            let active_consensus_dir_name = mcms.active_consensus_dir_name().unwrap();

            match active_consensus_dir_name {
                Some(current_consensus_db) => {
                    // Apply soft upgrade logic: delete relation data from higher levels
                    // and then update DB version to 6

                    let consensus_db = kaspa_database::prelude::ConnBuilder::default()
                        .with_db_path(consensus_db_dir.clone().join(current_consensus_db))
                        .with_files_limit(10)
                        .with_preset(rocksdb_preset)
                        .with_wal_dir(wal_dir.clone())
                        .with_cache_budget(cache_budget)
                        .build()
                        .unwrap();

                    let start_level: u8 = 1;
                    let start_level_bytes = start_level.to_le_bytes();

                    let mut writer = DirectDbWriter::new(&consensus_db);

                    let end_level: u8 = config.max_block_level + 1;
                    let end_level_bytes = end_level.to_le_bytes();

                    let start_parents_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsParents.into_iter().chain(start_level_bytes).collect();
                    let end_parents_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsParents.into_iter().chain(end_level_bytes).collect();

                    let start_children_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsChildren.into_iter().chain(start_level_bytes).collect();
                    let end_children_prefix_vec: Vec<_> =
                        DatabaseStorePrefixes::RelationsChildren.into_iter().chain(end_level_bytes).collect();

                    // Apply delete of range from level 1 to max (+1) for RelationsParents and RelationsChildren:
                    writer.delete_range(start_parents_prefix_vec.clone(), end_parents_prefix_vec.clone()).unwrap();
                    writer.delete_range(start_children_prefix_vec.clone(), end_children_prefix_vec.clone()).unwrap();

                    //  update the version to one higher:
                    mcms.set_version(6).unwrap();
                    info!("Deprecated stores have been removed from database, storage will be gradually cleared in due time.");
                    info!("Database is now in version 6");
                }
                None => {
                    is_db_reset_needed = request_database_deletion_approval(args.yes);
                    continue 'db_upgrade;
                }
            }
        }
        // if we reached here, db should be upgraded fully and we should exit the loop next
        assert_eq!(mcms.version().unwrap(), LATEST_DB_VERSION);
    }

    // Will be true if any of the other condition above except args.reset_db
    // has set is_db_reset_needed to true
    if is_db_reset_needed && !args.reset_db {
        // Drop so that deletion works
        drop(meta_db);

        // Delete
        fs::remove_dir_all(db_dir.clone()).unwrap();

        // Recreate the empty folders
        fs::create_dir_all(consensus_db_dir.as_path()).unwrap();
        fs::create_dir_all(meta_db_dir.as_path()).unwrap();

        if args.utxoindex {
            fs::create_dir_all(utxoindex_db_dir.as_path()).unwrap();
        }

        // Reopen the DB
        meta_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(meta_db_dir)
            .with_files_limit(META_DB_FILE_LIMIT)
            .with_preset(rocksdb_preset)
            .with_wal_dir(wal_dir.clone())
            .with_cache_budget(cache_budget)
            .build()
            .unwrap();
    }

    if !args.archival && MultiConsensusManagementStore::new(meta_db.clone()).is_archival_node().unwrap() {
        get_user_approval_or_exit(
            "--archival is set to false although the node was previously archival. Proceeding may delete archived data. Do you confirm? (y/n)",
            args.yes,
        );
    }

    let connect_peers = args.connect_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect::<Vec<_>>();
    let add_peers = args.add_peers.iter().map(|x| x.normalize(config.default_p2p_port())).collect();
    let p2p_server_addr = args.listen.unwrap_or(ContextualNetAddress::unspecified()).normalize(config.default_p2p_port());
    // connect_peers means no DNS seeding and no outbound/inbound peers
    let outbound_target = if connect_peers.is_empty() { args.outbound_target } else { 0 };
    let mut random_graph_target = outbound_target;

    // Handle the Perigee configuration
    let perigee_config = if args.blk_perigee_peers == 0 {
        debug!("Perigee disabled: perigee target is set to 0 (default behavior)");
        None
    } else if !connect_peers.is_empty() {
        // We supply an explicit log here, as the user most have tried to enable perigee, and probably wants to know why, and that, it is disabled
        info!("Perigee disabled: outbound target is set to 0 because `--connect-peers` argument was supplied");
        None
    } else {
        let perigee_config = create_perigee_config(
            outbound_target,
            args.blk_perigee_peers,
            args.blk_perigee_duration,
            args.blk_perigee_leverage,
            args.blk_perigee_exploration,
            args.blk_perigee_persist,
            args.blk_perigee_stats,
            config.bps(),
        );
        // Reduce random graph outbound target by perigee outbound target
        random_graph_target -= perigee_config.perigee_outbound_target;
        info!("Perigee enabled - Perigee Configuration: {}", perigee_config);
        Some(perigee_config)
    };

    let inbound_limit = if connect_peers.is_empty() { args.inbound_limit } else { 0 };
    let dns_seeders = if connect_peers.is_empty() && !args.disable_dns_seeding { config.dns_seeders } else { &[] };

    let grpc_server_addr = args.rpclisten.unwrap_or(ContextualNetAddress::loopback()).normalize(config.default_rpc_port());

    let core = Arc::new(Core::new());

    // ---

    let tick_service = Arc::new(TickService::new());
    let (notification_send, notification_recv) = unbounded();
    let max_tracked_addresses = if args.utxoindex && args.max_tracked_addresses > 0 { Some(args.max_tracked_addresses) } else { None };
    let subscription_context = SubscriptionContext::with_options(max_tracked_addresses);
    let notification_root = Arc::new(ConsensusNotificationRoot::with_context(notification_send, subscription_context.clone()));
    let processing_counters = Arc::new(ProcessingCounters::default());
    let mining_counters = Arc::new(MiningCounters::default());
    let wrpc_borsh_counters = Arc::new(WrpcServerCounters::default());
    let wrpc_json_counters = Arc::new(WrpcServerCounters::default());
    let tx_script_cache_counters = Arc::new(TxScriptCacheCounters::default());
    let p2p_tower_counters = Arc::new(TowerConnectionCounters::default());
    let grpc_tower_counters = Arc::new(TowerConnectionCounters::default());

    // Use `num_cpus` background threads for the consensus database as recommended by rocksdb
    let mining_rules = Arc::new(MiningRules::default());
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
        mining_rules.clone(),
        rocksdb_preset,
        wal_dir.clone(),
        cache_budget,
    ));
    let consensus_manager = Arc::new(ConsensusManager::new(consensus_factory));
    let consensus_monitor = Arc::new(ConsensusMonitor::new(processing_counters.clone(), tick_service.clone()));

    let perf_monitor_builder = PerfMonitorBuilder::new()
        .with_fetch_interval(Duration::from_secs(args.perf_metrics_interval_sec))
        .with_tick_service(tick_service.clone());
    let perf_monitor = if args.perf_metrics {
        let cb = move |counters: CountersSnapshot| {
            debug!("[{}] {}", kaspa_perf_monitor::SERVICE_NAME, counters.to_process_metrics_display());
            debug!("[{}] {}", kaspa_perf_monitor::SERVICE_NAME, counters.to_io_metrics_display());
            #[cfg(feature = "heap")]
            debug!("[{}] heap stats: {:?}", kaspa_perf_monitor::SERVICE_NAME, dhat::HeapStats::get());
        };
        Arc::new(perf_monitor_builder.with_fetch_cb(cb).build())
    } else {
        Arc::new(perf_monitor_builder.build())
    };

    let system_info = SystemInfo::default();

    let notify_service = Arc::new(NotifyService::new(notification_root.clone(), notification_recv, subscription_context.clone()));
    let index_service: Option<Arc<IndexService>> = if args.utxoindex {
        // Use only a single thread for none-consensus databases
        let utxoindex_db = kaspa_database::prelude::ConnBuilder::default()
            .with_db_path(utxoindex_db_dir)
            .with_files_limit(utxo_files_limit)
            .with_preset(rocksdb_preset)
            .with_wal_dir(wal_dir.clone())
            .with_cache_budget(cache_budget)
            .build()
            .unwrap();
        let utxoindex = UtxoIndexProxy::new(UtxoIndex::new(consensus_manager.clone(), utxoindex_db).unwrap());
        let index_service = Arc::new(IndexService::new(&notify_service.notifier(), subscription_context.clone(), Some(utxoindex)));
        Some(index_service)
    } else {
        None
    };

    let (address_manager, port_mapping_extender_svc) = AddressManager::new(config.clone(), meta_db, tick_service.clone());
    if args.blk_perigee_reset {
        address_manager.lock().reset_perigee_data();
    }

    let mining_manager = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        mining_counters.clone(),
    )));
    let mining_monitor =
        Arc::new(MiningMonitor::new(mining_manager.clone(), mining_counters, tx_script_cache_counters.clone(), tick_service.clone()));

    let hub = Hub::new();
    let mining_rule_engine = Arc::new(MiningRuleEngine::new(
        consensus_manager.clone(),
        config.clone(),
        processing_counters.clone(),
        tick_service.clone(),
        hub.clone(),
        mining_rules,
    ));

    // Ibd running flag, is created here to be potentially shared with the perigee manager
    let is_ibd_running = Arc::new(std::sync::atomic::AtomicBool::default());
    let perigee_manager = perigee_config.map(|perigee_config| Arc::new(PerigeeManager::new(perigee_config, is_ibd_running.clone())));

    let flow_context = Arc::new(FlowContext::new(
        consensus_manager.clone(),
        address_manager,
        config.clone(),
        mining_manager.clone(),
        tick_service.clone(),
        notification_root,
        hub.clone(),
        mining_rule_engine.clone(),
        is_ibd_running,
        perigee_manager,
    ));

    let p2p_service = Arc::new(P2pService::new(
        flow_context.clone(),
        connect_peers,
        add_peers,
        p2p_server_addr,
        random_graph_target,
        inbound_limit,
        dns_seeders,
        config.default_p2p_port(),
        p2p_tower_counters.clone(),
    ));

    let rpc_core_service = Arc::new(RpcCoreService::new(
        consensus_manager.clone(),
        notify_service.notifier(),
        index_service.as_ref().map(|x| x.notifier()),
        mining_manager,
        flow_context,
        subscription_context,
        index_service.as_ref().map(|x| x.utxoindex().unwrap()),
        config.clone(),
        core.clone(),
        processing_counters,
        wrpc_borsh_counters.clone(),
        wrpc_json_counters.clone(),
        perf_monitor.clone(),
        p2p_tower_counters.clone(),
        grpc_tower_counters.clone(),
        system_info,
        mining_rule_engine.clone(),
    ));
    let grpc_service_broadcasters: usize = 3; // TODO: add a command line argument or derive from other arg/config/host-related fields
    let grpc_service = if !args.disable_grpc {
        Some(Arc::new(GrpcService::new(
            grpc_server_addr,
            config,
            rpc_core_service.clone(),
            args.rpc_max_clients,
            grpc_service_broadcasters,
            grpc_tower_counters,
        )))
    } else {
        None
    };

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
    if let Some(grpc_service) = grpc_service {
        async_runtime.register(grpc_service)
    }
    async_runtime.register(p2p_service);
    async_runtime.register(consensus_monitor);
    async_runtime.register(mining_monitor);
    async_runtime.register(perf_monitor);
    async_runtime.register(mining_rule_engine);

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
