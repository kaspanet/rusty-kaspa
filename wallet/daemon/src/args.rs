use clap::{Arg, ArgAction, Command, error::ErrorKind};
use kaspa_core::kaspad_env::version;
use std::{ffi::OsString, net::SocketAddr};

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
        Self::try_parse_from(std::env::args_os()).unwrap_or_else(|err| err.exit())
    }

    pub fn try_parse_from<I, T>(itr: I) -> Result<Args, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let matches = cli().try_get_matches_from(itr)?;

        Ok(Args {
            password: matches
                .get_one::<String>("password")
                .cloned()
                .ok_or_else(|| cli().error(ErrorKind::MissingRequiredArgument, "the --password argument is required"))?,
            name: matches.get_one::<String>("name").cloned(),
            rpc_server: matches.get_one::<String>("rpc-server").cloned(),
            network_id: matches.get_one::<String>("network-id").cloned(),
            listen_address: matches
                .get_one::<SocketAddr>("listen-address")
                .copied()
                .ok_or_else(|| cli().error(ErrorKind::MissingRequiredArgument, "the --listen-address argument is required"))?,
            ecdsa: matches.get_flag("ecdsa"),
        })
    }
}

pub fn cli() -> Command {
    Command::new("kaspawalletd")
        .about(format!("{} (kaspawalletd) v{}", env!("CARGO_PKG_DESCRIPTION"), version()))
        .version(env!("CARGO_PKG_VERSION"))
        .arg(Arg::new("password").long("password").short('p').help("Wallet password").required(true))
        .arg(Arg::new("name").long("name").short('n').help("Name of wallet"))
        .arg(Arg::new("rpc-server").long("rpc-server").short('s').help("Private RPC server URL"))
        .arg(
            Arg::new("network-id")
                .long("network-id")
                .required_unless_present("rpc-server")
                .help("Network id to be connected via PNN."),
        )
        .arg(
            Arg::new("listen-address")
                .long("listen-address")
                .short('l')
                .value_parser(clap::value_parser!(SocketAddr))
                .default_value("127.0.0.1:8082")
                .help("gRPC listening address with port."),
        )
        .arg(Arg::new("ecdsa").long("ecdsa").action(ArgAction::SetTrue).help("Use ecdsa for transactions broadcast"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(std::iter::once("kaspa-wallet-daemon").chain(args.iter().copied()))
    }

    #[test]
    fn cli_debug_assert() {
        cli().debug_assert();
    }

    #[test]
    fn defaults() {
        let args = parse(&["-p", "x", "--network-id", "mainnet"]).unwrap();
        assert_eq!(args.listen_address, "127.0.0.1:8082".parse::<SocketAddr>().unwrap());
        assert!(!args.ecdsa);
        assert_eq!(args.password, "x");
        assert_eq!(args.network_id.as_deref(), Some("mainnet"));
        assert!(args.rpc_server.is_none());
        assert!(args.name.is_none());
    }

    #[test]
    fn listen_address_parses() {
        let args = parse(&["-p", "x", "--network-id", "mainnet", "-l", "0.0.0.0:9000"]).unwrap();
        assert_eq!(args.listen_address, "0.0.0.0:9000".parse::<SocketAddr>().unwrap());
    }

    #[test]
    fn listen_address_invalid() {
        assert!(parse(&["-p", "x", "--network-id", "mainnet", "-l", "not-an-address"]).is_err());
    }

    #[test]
    fn ecdsa_flag() {
        let args = parse(&["-p", "x", "--network-id", "testnet-10", "--ecdsa"]).unwrap();
        assert!(args.ecdsa);
    }

    #[test]
    fn password_required() {
        let err = parse(&["--network-id", "mainnet"]).err().unwrap();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn rpc_server_or_network_id_required() {
        let err = parse(&["-p", "x"]).err().unwrap();
        assert_eq!(err.kind(), ErrorKind::MissingRequiredArgument);
        assert!(parse(&["-p", "x", "--network-id", "testnet-10"]).is_ok());
        assert!(parse(&["-p", "x", "--rpc-server", "ws://127.0.0.1:17110"]).is_ok());
    }
}
