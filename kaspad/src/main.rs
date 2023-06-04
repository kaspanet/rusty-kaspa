extern crate kaspa_consensus;
extern crate kaspa_core;
extern crate kaspa_hashes;

use daemon::{create_daemon, Args};
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

use kaspa_consensus::config::ConfigBuilder;
use kaspa_utxoindex::UtxoIndex;

use async_channel::unbounded;
use kaspa_core::{info, trace};
use kaspa_grpc_server::GrpcServer;
use kaspa_p2p_flows::service::P2pService;
use kaspa_wrpc_server::service::{Options as WrpcServerOptions, WrpcEncoding, WrpcService};

use crate::args::parse_args;

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

pub fn main() {
    let args = parse_args();
    create_daemon(args).run();
    trace!("Kaspad is finished...");
}
