use clap::{Arg, Command};
use kaspa_core::kaspad_env::version;
use std::net::SocketAddr;

pub struct Args {
    pub password: String,
    pub name: Option<String>,
    pub rpc_server: Option<String>,
    pub network_id: Option<String>,
    pub listen_address: SocketAddr,
    pub ecdsa: bool,
}

impl Args {
    pub fn parse() -> Self {
        let matches = cli().get_matches();

        Args {
            password: matches.get_one::<String>("password").cloned().expect("Password argument is missing."),
            name: matches.get_one::<String>("name").cloned(),
            rpc_server: matches.get_one::<String>("rpc-server").cloned(),
            network_id: matches.get_one::<String>("network-id").cloned(),
            listen_address: matches
                .get_one::<SocketAddr>("listen-address")
                .cloned()
                .unwrap_or_else(|| "127.0.0.1:8082".parse().unwrap()),
            ecdsa: matches.get_one::<bool>("ecdsa").cloned().unwrap_or(false),
        }
    }
}

pub fn cli() -> Command {
    Command::new("kaspawalletd")
        .about(format!("{} (kaspawalletd) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("password").long("password").short('p').value_name("password").help("Path of password file"))
        .arg(
            Arg::new("name")
                .long("name")
                .short('n')
                .value_name("name")
                .value_parser(clap::value_parser!(String))
                .help("Name of wallet"),
        )
        .arg(
            Arg::new("rpc-server")
                .long("rpc-server")
                .short('s')
                .value_name("rpc-server")
                .value_parser(clap::value_parser!(String))
                .help("Private RPC server URL"),
        )
        .arg(
            Arg::new("network-id")
                .long("network-id")
                .value_name("network-id")
                .value_parser(clap::value_parser!(String))
                .help("Network id to be connected via PNN."),
        )
        .arg(
            Arg::new("listen-address")
                .long("listen-address")
                .short('l')
                .value_name("listen-address")
                .value_parser(clap::value_parser!(String))
                .help("gRPC listening address with port."),
        )
        .arg(
            Arg::new("ecdsa")
                .long("ecdsa")
                .value_name("ecdsa")
                .value_parser(clap::value_parser!(bool))
                .help("Use ecdsa for transactions broadcast"),
        )
}
