#[allow(unused)]
use clap::{arg, command, Arg, Command};

// const DEFAULT_DATA_DIR: &str = "datadir";
const DEFAULT_LISTEN_GRPC: &str = "127.0.0.1:16110";
const DEFAULT_LISTEN_WRPC_BORSH: &str = "127.0.0.1:17110";
const DEFAULT_LISTEN_WRPC_JSON: &str = "127.0.0.1:18110";

#[derive(Debug)]
pub struct Args {
    // NOTE: it is best if property names match config file fields
    pub appdir: Option<String>,
    pub rpclisten: Option<String>,
    pub rpclisten_borsh: Option<String>,
    pub rpclisten_json: Option<String>,
    pub wrpc_verbose: bool,
    pub log_level: String,
}

pub fn cli() -> Command {
    Command::new("kaspad")
        .about(format!("{} (rusty-kaspa) v{}", env!("CARGO_PKG_DESCRIPTION"), env!("CARGO_PKG_VERSION")))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(arg!(-b --appdir <DATA_DIR> "Directory to store data."))
        .arg(
            Arg::new("log_level")
                .short('d')
                .long("loglevel")
                .value_name("log_level")
                .default_value("info")
                .num_args(0..=1)
                .require_equals(true)
                .help("Specify log level."),
        )
        .arg(
            Arg::new("rpclisten")
                .long("rpclisten")
                .value_name("rpclisten")
                .default_value(DEFAULT_LISTEN_GRPC)
                .num_args(0..=1)
                .require_equals(true)
                .help("Interface:port to listen for gRPC connections (default port: 16110, testnet: 16210)."),
        )
        .arg(
            Arg::new("rpclisten-borsh")
                .long("rpclisten-borsh")
                .value_name("rpclisten-borsh")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value(DEFAULT_LISTEN_WRPC_BORSH)
                .help(format!(
                    "Interface:port to listen for wRPC Borsh connections (interop only; default: `{DEFAULT_LISTEN_WRPC_BORSH}`)."
                )),
        )
        .arg(
            Arg::new("rpclisten-json")
                .long("rpclisten-json")
                .value_name("rpclisten-json")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value(DEFAULT_LISTEN_WRPC_JSON)
                .help(format!("Interface:port to listen for wRPC JSON connections (default: {DEFAULT_LISTEN_WRPC_JSON}).")),
        )
}

impl Args {
    pub fn parse() -> Args {
        let m = cli().get_matches();
        Args {
            appdir: m.get_one::<String>("appdir").cloned(),
            rpclisten: m.get_one::<String>("rpclisten").cloned(),
            rpclisten_borsh: m.get_one::<String>("rpclisten-borsh").cloned(),
            rpclisten_json: m.get_one::<String>("rpclisten-json").cloned(),
            wrpc_verbose: false,
            log_level: m.get_one::<String>("log_level").cloned().unwrap(),
        }
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
      --testnet                             Use the test network
      --simnet                              Use the simulation test network
      --devnet                              Use the development test network
      --override-dag-params-file=           Overrides DAG params (allowed only on devnet)
  -s, --service=                            Service command {install, remove, start, stop}


*/
