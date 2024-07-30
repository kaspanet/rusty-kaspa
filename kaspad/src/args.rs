use clap::{arg, Arg, ArgAction, Command};
use kaspa_consensus_core::{
    config::Config,
    network::{NetworkId, NetworkType},
};
use kaspa_core::kaspad_env::version;
use kaspa_notify::address::tracker::Tracker;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_wrpc_server::address::WrpcNetAddress;
use serde::Deserialize;
use serde_with::{serde_as, DisplayFromStr};
use std::{ffi::OsString, fs};
use toml::from_str;

#[cfg(feature = "devnet-prealloc")]
use kaspa_addresses::Address;
#[cfg(feature = "devnet-prealloc")]
use kaspa_consensus_core::tx::{TransactionOutpoint, UtxoEntry};
#[cfg(feature = "devnet-prealloc")]
use kaspa_txscript::pay_to_address_script;
#[cfg(feature = "devnet-prealloc")]
use std::sync::Arc;

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct Args {
    // NOTE: it is best if property names match config file fields
    pub appdir: Option<String>,
    pub logdir: Option<String>,
    #[serde(rename = "nologfiles")]
    pub no_log_files: bool,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten: Option<ContextualNetAddress>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten_borsh: Option<WrpcNetAddress>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub rpclisten_json: Option<WrpcNetAddress>,
    #[serde(rename = "unsaferpc")]
    pub unsafe_rpc: bool,
    pub wrpc_verbose: bool,
    #[serde(rename = "loglevel")]
    pub log_level: String,
    pub async_threads: usize,
    #[serde(rename = "connect")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub connect_peers: Vec<ContextualNetAddress>,
    #[serde(rename = "addpeer")]
    #[serde_as(as = "Vec<DisplayFromStr>")]
    pub add_peers: Vec<ContextualNetAddress>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub listen: Option<ContextualNetAddress>,
    #[serde(rename = "uacomment")]
    pub user_agent_comments: Vec<String>,
    pub utxoindex: bool,
    pub reset_db: bool,
    #[serde(rename = "outpeers")]
    pub outbound_target: usize,
    #[serde(rename = "maxinpeers")]
    pub inbound_limit: usize,
    #[serde(rename = "rpcmaxclients")]
    pub rpc_max_clients: usize,
    pub max_tracked_addresses: usize,
    pub enable_unsynced_mining: bool,
    pub enable_mainnet_mining: bool,
    pub testnet: bool,
    #[serde(rename = "netsuffix")]
    pub testnet_suffix: u32,
    pub devnet: bool,
    pub simnet: bool,
    pub archival: bool,
    pub sanity: bool,
    pub yes: bool,
    #[serde_as(as = "Option<DisplayFromStr>")]
    pub externalip: Option<ContextualNetAddress>,
    pub perf_metrics: bool,
    pub perf_metrics_interval_sec: u64,
    pub block_template_cache_lifetime: Option<u64>,

    #[cfg(feature = "devnet-prealloc")]
    pub num_prealloc_utxos: Option<u64>,
    #[cfg(feature = "devnet-prealloc")]
    pub prealloc_address: Option<String>,
    #[cfg(feature = "devnet-prealloc")]
    pub prealloc_amount: u64,

    pub disable_upnp: bool,
    #[serde(rename = "nodnsseed")]
    pub disable_dns_seeding: bool,
    #[serde(rename = "nogrpc")]
    pub disable_grpc: bool,
    pub ram_scale: f64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            appdir: None,
            no_log_files: false,
            rpclisten_borsh: None,
            rpclisten_json: None,
            unsafe_rpc: false,
            async_threads: num_cpus::get(),
            utxoindex: false,
            reset_db: false,
            outbound_target: 8,
            inbound_limit: 128,
            rpc_max_clients: 128,
            max_tracked_addresses: 0,
            enable_unsynced_mining: false,
            enable_mainnet_mining: true,
            testnet: false,
            testnet_suffix: 10,
            devnet: false,
            simnet: false,
            archival: false,
            sanity: false,
            logdir: None,
            rpclisten: None,
            wrpc_verbose: false,
            log_level: "INFO".into(),
            connect_peers: vec![],
            add_peers: vec![],
            listen: None,
            user_agent_comments: vec![],
            yes: false,
            perf_metrics: false,
            perf_metrics_interval_sec: 10,
            externalip: None,
            block_template_cache_lifetime: None,

            #[cfg(feature = "devnet-prealloc")]
            num_prealloc_utxos: None,
            #[cfg(feature = "devnet-prealloc")]
            prealloc_address: None,
            #[cfg(feature = "devnet-prealloc")]
            prealloc_amount: 1_000_000,

            disable_upnp: false,
            disable_dns_seeding: false,
            disable_grpc: false,
            ram_scale: 1.0,
        }
    }
}

impl Args {
    pub fn apply_to_config(&self, config: &mut Config) {
        config.utxoindex = self.utxoindex;
        config.disable_upnp = self.disable_upnp;
        config.unsafe_rpc = self.unsafe_rpc;
        config.enable_unsynced_mining = self.enable_unsynced_mining;
        config.enable_mainnet_mining = self.enable_mainnet_mining;
        config.is_archival = self.archival;
        // TODO: change to `config.enable_sanity_checks = self.sanity` when we reach stable versions
        config.enable_sanity_checks = true;
        config.user_agent_comments.clone_from(&self.user_agent_comments);
        config.block_template_cache_lifetime = self.block_template_cache_lifetime;
        config.p2p_listen_address = self.listen.unwrap_or(ContextualNetAddress::unspecified());
        config.externalip = self.externalip.map(|v| v.normalize(config.default_p2p_port()));
        config.ram_scale = self.ram_scale;

        #[cfg(feature = "devnet-prealloc")]
        if let Some(num_prealloc_utxos) = self.num_prealloc_utxos {
            config.initial_utxo_set = Arc::new(self.generate_prealloc_utxos(num_prealloc_utxos));
        }
    }

    #[cfg(feature = "devnet-prealloc")]
    pub fn generate_prealloc_utxos(&self, num_prealloc_utxos: u64) -> kaspa_consensus_core::utxo::utxo_collection::UtxoCollection {
        let addr = Address::try_from(&self.prealloc_address.as_ref().unwrap()[..]).unwrap();
        let spk = pay_to_address_script(&addr);
        (1..=num_prealloc_utxos)
            .map(|i| {
                (
                    TransactionOutpoint { transaction_id: i.into(), index: 0 },
                    UtxoEntry { amount: self.prealloc_amount, script_public_key: spk.clone(), block_daa_score: 0, is_coinbase: false },
                )
            })
            .collect()
    }

    pub fn network(&self) -> NetworkId {
        match (self.testnet, self.devnet, self.simnet) {
            (false, false, false) => NetworkId::new(NetworkType::Mainnet),
            (true, false, false) => NetworkId::with_suffix(NetworkType::Testnet, self.testnet_suffix),
            (false, true, false) => NetworkId::new(NetworkType::Devnet),
            (false, false, true) => NetworkId::new(NetworkType::Simnet),
            _ => panic!("only a single net should be activated"),
        }
    }
}

pub fn cli() -> Command {
    let defaults: Args = Default::default();

    #[allow(clippy::let_and_return)]
    let cmd = Command::new("kaspad")
        .about(format!("{} (rusty-kaspa) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(arg!(-C --configfile <CONFIG_FILE> "Path of config file."))
        .arg(arg!(-b --appdir <DATA_DIR> "Directory to store data."))
        .arg(arg!(--logdir <LOG_DIR> "Directory to log output."))
        .arg(arg!(--nologfiles "Disable logging to files."))
        .arg(
            Arg::new("async_threads")
                .short('t')
                .long("async-threads")
                .env("KASPAD_ASYNC_THREADS")
                .value_name("async_threads")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help(format!("Specify number of async threads (default: {}).", defaults.async_threads)),
        )
        .arg(
            Arg::new("log_level")
                .short('d')
                .long("loglevel")
                .env("KASPAD_LOG_LEVEL")
                .value_name("LEVEL")
                .default_value("info")
                .require_equals(true)
                .help("Logging level for all subsystems {off, error, warn, info, debug, trace}\n-- You may also specify <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for individual subsystems.".to_string()),
        )
        .arg(
            Arg::new("rpclisten")
                .long("rpclisten")
                .env("KASPAD_RPCLISTEN")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Interface:port to listen for gRPC connections (default port: 16110, testnet: 16210)."),
        )
        .arg(
            Arg::new("rpclisten-borsh")
                .long("rpclisten-borsh")
                .env("KASPAD_RPCLISTEN_BORSH")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("default") // TODO: Find a way to use defaults.rpclisten_borsh
                .value_parser(clap::value_parser!(WrpcNetAddress))
                .help("Interface:port to listen for wRPC Borsh connections (default port: 17110, testnet: 17210)."),

        )
        .arg(
            Arg::new("rpclisten-json")
                .long("rpclisten-json")
                .env("KASPAD_RPCLISTEN_JSON")
                .value_name("IP[:PORT]")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("default") // TODO: Find a way to use defaults.rpclisten_json
                .value_parser(clap::value_parser!(WrpcNetAddress))
                .help("Interface:port to listen for wRPC JSON connections (default port: 18110, testnet: 18210)."),
        )
        .arg(arg!(--unsaferpc "Enable RPC commands which affect the state of the node"))
        .arg(
            Arg::new("connect-peers")
                .long("connect")
                .env("KASPAD_CONNECTPEERS")
                .value_name("IP[:PORT]")
                .action(ArgAction::Append)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Connect only to the specified peers at startup."),
        )
        .arg(
            Arg::new("add-peers")
                .long("addpeer")
                .env("KASPAD_ADDPEERS")
                .value_name("IP[:PORT]")
                .action(ArgAction::Append)
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add peers to connect with at startup."),
        )
        .arg(
            Arg::new("listen")
                .long("listen")
                .env("KASPAD_LISTEN")
                .value_name("IP[:PORT]")
                .require_equals(true)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add an interface:port to listen for connections (default all interfaces port: 16111, testnet: 16211)."),
        )
        .arg(
            Arg::new("outpeers")
                .long("outpeers")
                .env("KASPAD_OUTPEERS")
                .value_name("outpeers")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Target number of outbound peers (default: 8)."),
        )
        .arg(
            Arg::new("maxinpeers")
                .long("maxinpeers") 
                .env("KASPAD_MAXINPEERS")
                .value_name("maxinpeers")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Max number of inbound peers (default: 128)."),
        )
        .arg(
            Arg::new("rpcmaxclients")
                .long("rpcmaxclients")
                .env("KASPAD_RPCMAXCLIENTS")
                .value_name("rpcmaxclients")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help("Max number of RPC clients for standard connections (default: 128)."),
        )
        .arg(arg!(--"reset-db" "Reset database before starting node. It's needed when switching between subnetworks."))
        .arg(arg!(--"enable-unsynced-mining" "Allow the node to accept blocks from RPC while not synced (this flag is mainly used for testing)"))
        .arg(
            Arg::new("enable-mainnet-mining")
                .long("enable-mainnet-mining")
                .env("KASPAD_ENABLE_MAINNET_MINING")
                .action(ArgAction::SetTrue)
                .hide(true)
                .help("Allow mainnet mining (currently enabled by default while the flag is kept for backwards compatibility)"),
        )
        .arg(arg!(--utxoindex "Enable the UTXO index"))
        .arg(
            Arg::new("max-tracked-addresses")
                .long("max-tracked-addresses")
                .env("KASPAD_MAX_TRACKED_ADDRESSES")
                .require_equals(true)
                .value_parser(clap::value_parser!(usize))
                .help(format!("Max (preallocated) number of addresses being tracked for UTXO changed events (default: {}, maximum: {}). 
Setting to 0 prevents the preallocation and sets the maximum to {}, leading to 0 memory footprint as long as unused but to sub-optimal footprint if used.", 
0, Tracker::MAX_ADDRESS_UPPER_BOUND, Tracker::DEFAULT_MAX_ADDRESSES)),
        )
        .arg(arg!(--testnet "Use the test network"))
        .arg(
            Arg::new("netsuffix")
                .long("netsuffix")
                .env("KASPAD_NETSUFFIX")
                .value_name("netsuffix")
                .require_equals(true)
                .value_parser(clap::value_parser!(u32))
                .help("Testnet network suffix number"),
        )
        .arg(arg!(--devnet "Use the development test network"))
        .arg(arg!(--simnet "Use the simulation test network"))
        .arg(arg!(--archival "Run as an archival node: avoids deleting old block data when moving the pruning point (Warning: heavy disk usage)"))
        .arg(arg!(--sanity "Enable various sanity checks which might be compute-intensive (mostly performed during pruning)"))
        .arg(arg!(--yes "Answer yes to all interactive console questions"))
        .arg(
            Arg::new("user_agent_comments")
                .long("uacomment")
                .env("KASPAD_USER_AGENT_COMMENTS")
                .action(ArgAction::Append)
                .require_equals(true)
                .help("Comment to add to the user agent -- See BIP 14 for more information."),
        )
        .arg(
            Arg::new("externalip")
                .long("externalip")
                .env("KASPAD_EXTERNALIP")
                .value_name("externalip")
                .require_equals(true)
                .default_missing_value(None)
                .value_parser(clap::value_parser!(ContextualNetAddress))
                .help("Add a socket address(ip:port) to the list of local addresses we claim to listen on to peers"),
        )
        .arg(arg!(--"perf-metrics" "Enable performance metrics: cpu, memory, disk io usage"))
        .arg(
            Arg::new("perf-metrics-interval-sec")
                .long("perf-metrics-interval-sec")
                .env("KASPAD_PERF_METRICS_INTERVAL_SEC")
                .require_equals(true)
                .value_parser(clap::value_parser!(u64))
                .help("Interval in seconds for performance metrics collection."),
        )
        .arg(arg!(--"disable-upnp" "Disable upnp"))
        .arg(arg!(--"nodnsseed" "Disable DNS seeding for peers"))
        .arg(arg!(--"nogrpc" "Disable gRPC server"))
        .arg(
            Arg::new("ram-scale")
                .long("ram-scale")
                .env("KASPAD_RAM_SCALE")
                .require_equals(true)
                .value_parser(clap::value_parser!(f64))
                .help("Apply a scale factor to memory allocation bounds. Nodes with limited RAM (~4-8GB) should set this to ~0.3-0.5 respectively. Nodes with
a large RAM (~64GB) can set this value to ~3.0-4.0 and gain superior performance especially for syncing peers faster"),
        )
        ;

    #[cfg(feature = "devnet-prealloc")]
    let cmd = cmd
        .arg(Arg::new("num-prealloc-utxos").long("num-prealloc-utxos").require_equals(true).value_parser(clap::value_parser!(u64)))
        .arg(Arg::new("prealloc-address").long("prealloc-address").require_equals(true).value_parser(clap::value_parser!(String)))
        .arg(Arg::new("prealloc-amount").long("prealloc-amount").require_equals(true).value_parser(clap::value_parser!(u64)));

    cmd
}

pub fn parse_args() -> Args {
    match Args::parse(std::env::args_os()) {
        Ok(args) => args,
        Err(err) => {
            println!("{err}");
            std::process::exit(1);
        }
    }
}

impl Args {
    pub fn parse<I, T>(itr: I) -> Result<Args, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let m: clap::ArgMatches = cli().try_get_matches_from(itr)?;
        let mut defaults: Args = Default::default();

        if let Some(config_file) = m.get_one::<String>("configfile") {
            let config_str = fs::read_to_string(config_file)?;
            defaults = from_str(&config_str).map_err(|toml_error| {
                clap::Error::raw(
                    clap::error::ErrorKind::ValueValidation,
                    format!("failed parsing config file, reason: {}", toml_error.message()),
                )
            })?;
        }

        let args = Args {
            appdir: m.get_one::<String>("appdir").cloned().or(defaults.appdir),
            logdir: m.get_one::<String>("logdir").cloned().or(defaults.logdir),
            no_log_files: arg_match_unwrap_or::<bool>(&m, "nologfiles", defaults.no_log_files),
            rpclisten: m.get_one::<ContextualNetAddress>("rpclisten").cloned().or(defaults.rpclisten),
            rpclisten_borsh: m.get_one::<WrpcNetAddress>("rpclisten-borsh").cloned().or(defaults.rpclisten_borsh),
            rpclisten_json: m.get_one::<WrpcNetAddress>("rpclisten-json").cloned().or(defaults.rpclisten_json),
            unsafe_rpc: arg_match_unwrap_or::<bool>(&m, "unsaferpc", defaults.unsafe_rpc),
            wrpc_verbose: false,
            log_level: arg_match_unwrap_or::<String>(&m, "log_level", defaults.log_level),
            async_threads: arg_match_unwrap_or::<usize>(&m, "async_threads", defaults.async_threads),
            connect_peers: arg_match_many_unwrap_or::<ContextualNetAddress>(&m, "connect-peers", defaults.connect_peers),
            add_peers: arg_match_many_unwrap_or::<ContextualNetAddress>(&m, "add-peers", defaults.add_peers),
            listen: m.get_one::<ContextualNetAddress>("listen").cloned().or(defaults.listen),
            outbound_target: arg_match_unwrap_or::<usize>(&m, "outpeers", defaults.outbound_target),
            inbound_limit: arg_match_unwrap_or::<usize>(&m, "maxinpeers", defaults.inbound_limit),
            rpc_max_clients: arg_match_unwrap_or::<usize>(&m, "rpcmaxclients", defaults.rpc_max_clients),
            max_tracked_addresses: arg_match_unwrap_or::<usize>(&m, "max-tracked-addresses", defaults.max_tracked_addresses),
            reset_db: arg_match_unwrap_or::<bool>(&m, "reset-db", defaults.reset_db),
            enable_unsynced_mining: arg_match_unwrap_or::<bool>(&m, "enable-unsynced-mining", defaults.enable_unsynced_mining),
            enable_mainnet_mining: arg_match_unwrap_or::<bool>(&m, "enable-mainnet-mining", defaults.enable_mainnet_mining),
            utxoindex: arg_match_unwrap_or::<bool>(&m, "utxoindex", defaults.utxoindex),
            testnet: arg_match_unwrap_or::<bool>(&m, "testnet", defaults.testnet),
            testnet_suffix: arg_match_unwrap_or::<u32>(&m, "netsuffix", defaults.testnet_suffix),
            devnet: arg_match_unwrap_or::<bool>(&m, "devnet", defaults.devnet),
            simnet: arg_match_unwrap_or::<bool>(&m, "simnet", defaults.simnet),
            archival: arg_match_unwrap_or::<bool>(&m, "archival", defaults.archival),
            sanity: arg_match_unwrap_or::<bool>(&m, "sanity", defaults.sanity),
            yes: arg_match_unwrap_or::<bool>(&m, "yes", defaults.yes),
            user_agent_comments: arg_match_many_unwrap_or::<String>(&m, "user_agent_comments", defaults.user_agent_comments),
            externalip: m.get_one::<ContextualNetAddress>("externalip").cloned(),
            perf_metrics: arg_match_unwrap_or::<bool>(&m, "perf-metrics", defaults.perf_metrics),
            perf_metrics_interval_sec: arg_match_unwrap_or::<u64>(&m, "perf-metrics-interval-sec", defaults.perf_metrics_interval_sec),
            // Note: currently used programmatically by benchmarks and not exposed to CLI users
            block_template_cache_lifetime: defaults.block_template_cache_lifetime,
            disable_upnp: arg_match_unwrap_or::<bool>(&m, "disable-upnp", defaults.disable_upnp),
            disable_dns_seeding: arg_match_unwrap_or::<bool>(&m, "nodnsseed", defaults.disable_dns_seeding),
            disable_grpc: arg_match_unwrap_or::<bool>(&m, "nogrpc", defaults.disable_grpc),
            ram_scale: arg_match_unwrap_or::<f64>(&m, "ram-scale", defaults.ram_scale),

            #[cfg(feature = "devnet-prealloc")]
            num_prealloc_utxos: m.get_one::<u64>("num-prealloc-utxos").cloned(),
            #[cfg(feature = "devnet-prealloc")]
            prealloc_address: m.get_one::<String>("prealloc-address").cloned(),
            #[cfg(feature = "devnet-prealloc")]
            prealloc_amount: arg_match_unwrap_or::<u64>(&m, "prealloc-amount", defaults.prealloc_amount),
        };

        if arg_match_unwrap_or::<bool>(&m, "enable-mainnet-mining", false) {
            println!("\nNOTE: The flag --enable-mainnet-mining is deprecated and defaults to true also w/o explicit setting\n")
        }

        Ok(args)
    }
}

use clap::parser::ValueSource::DefaultValue;
use std::marker::{Send, Sync};
fn arg_match_unwrap_or<T: Clone + Send + Sync + 'static>(m: &clap::ArgMatches, arg_id: &str, default: T) -> T {
    m.get_one::<T>(arg_id).cloned().filter(|_| m.value_source(arg_id) != Some(DefaultValue)).unwrap_or(default)
}

fn arg_match_many_unwrap_or<T: Clone + Send + Sync + 'static>(m: &clap::ArgMatches, arg_id: &str, default: Vec<T>) -> Vec<T> {
    match m.get_many::<T>(arg_id) {
        Some(val_ref) => val_ref.cloned().collect(),
        None => default,
    }
}

/*

  -V, --version                             Display version information and exit
  -C, --configfile=                         Path to configuration file (default: /Users/aspect/Library/Application
                                            Support/Kaspad/kaspad.conf)
  -b, --appdir=                             Directory to store data (default: /Users/aspect/Library/Application
                                            Support/Kaspad)
      --logdir=                             Directory to log output.
  -a, --addpeer=                            Add a peer to connect with at startup
      --connect=                            Connect only to the specified peers at startup
      --nolisten                            Disable listening for incoming connections -- NOTE: Listening is
                                            automatically disabled if the --connect or --proxy options are used
                                            without also specifying listen interfaces via --listen
      --listen=                             Add an interface/port to listen for connections (default all interfaces
                                            port: 16111, testnet: 16211)
      --outpeers=                           Target number of outbound peers (default: 8)
      --maxinpeers=                         Max number of inbound peers (default: 117)
      --enablebanning                       Enable banning of misbehaving peers
      --banduration=                        How long to ban misbehaving peers. Valid time units are {s, m, h}. Minimum
                                            1 second (default: 24h0m0s)
      --banthreshold=                       Maximum allowed ban score before disconnecting and banning misbehaving
                                            peers. (default: 100)
      --whitelist=                          Add an IP network or IP that will not be banned. (eg. 192.168.1.0/24 or
                                            ::1)
      --rpclisten=                          Add an interface/port to listen for RPC connections (default port: 16110,
                                            testnet: 16210)
      --rpccert=                            File containing the certificate file (default:
                                            /Users/aspect/Library/Application Support/Kaspad/rpc.cert)
      --rpckey=                             File containing the certificate key (default:
                                            /Users/aspect/Library/Application Support/Kaspad/rpc.key)
      --rpcmaxclients=                      Max number of RPC clients for standard connections (default: 128)
      --rpcmaxwebsockets=                   Max number of RPC websocket connections (default: 25)
      --rpcmaxconcurrentreqs=               Max number of concurrent RPC requests that may be processed concurrently
                                            (default: 20)
      --norpc                               Disable built-in RPC server
      --saferpc                             Disable RPC commands which affect the state of the node
      --nodnsseed                           Disable DNS seeding for peers
      --dnsseed=                            Override DNS seeds with specified hostname (Only 1 hostname allowed)
      --grpcseed=                           Hostname of gRPC server for seeding peers
      --externalip=                         Add an ip to the list of local addresses we claim to listen on to peers
      --proxy=                              Connect via SOCKS5 proxy (eg. 127.0.0.1:9050)
      --proxyuser=                          Username for proxy server
      --proxypass=                          Password for proxy server
      --dbtype=                             Database backend to use for the Block DAG
      --profile=                            Enable HTTP profiling on given port -- NOTE port must be between 1024 and
                                            65536
  -d, --loglevel=                           Logging level for all subsystems {trace, debug, info, warn, error,
                                            critical} -- You may also specify
                                            <subsystem>=<level>,<subsystem2>=<level>,... to set the log level for
                                            individual subsystems -- Use show to list available subsystems (default:
                                            info)
      --upnp                                Use UPnP to map our listening port outside of NAT
      --minrelaytxfee=                      The minimum transaction fee in KAS/kB to be considered a non-zero fee.
                                            (default: 1e-05)
      --maxorphantx=                        Max number of orphan transactions to keep in memory (default: 100)
      --blockmaxmass=                       Maximum transaction mass to be used when creating a block (default:
                                            10000000)
      --uacomment=                          Comment to add to the user agent -- See BIP 14 for more information.
      --nopeerbloomfilters                  Disable bloom filtering support
      --sigcachemaxsize=                    The maximum number of entries in the signature verification cache
                                            (default: 100000)
      --blocksonly                          Do not accept transactions from remote peers.
      --relaynonstd                         Relay non-standard transactions regardless of the default settings for the
                                            active network.
      --rejectnonstd                        Reject non-standard transactions regardless of the default settings for
                                            the active network.
      --reset-db                            Reset database before starting node. It's needed when switching between
                                            subnetworks.
      --maxutxocachesize=                   Max size of loaded UTXO into ram from the disk in bytes (default:
                                            5000000000)
      --utxoindex                           Enable the UTXO index
      --archival                            Run as an archival node: don't delete old block data when moving the
                                            pruning point (Warning: heavy disk usage)'
      --protocol-version=                   Use non default p2p protocol version (default: 5)
      --enable-unsynced-mining              Allow the node to accept blocks from RPC while not synced
                                            (required when initiating a new network from genesis)
      --testnet                             Use the test network
      --simnet                              Use the simulation test network
      --devnet                              Use the development test network
      --override-dag-params-file=           Overrides DAG params (allowed only on devnet)
  -s, --service=                            Service command {install, remove, start, stop}
      --nogrpc                              Don't initialize the gRPC server
*/
