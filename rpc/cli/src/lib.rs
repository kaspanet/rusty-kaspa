//! `kaspa-rpc-cli`: a non-interactive command-line client for the Kaspa node
//! RPC API, usable both as the `kaspa-rpc` binary and as an embeddable library.
//!
//! Engine flow (see [`run`]): resolve config -> connect -> dispatch command ->
//! render output to stdout. Errors are returned as [`CliError`] and mapped to
//! `sysexits`-style process exit codes by the binary.

pub mod args;
pub mod cli;
pub mod commands;
pub mod config;
pub mod error;
pub mod output;
pub mod transport;

pub use cli::{Cli, Commands};
pub use commands::RpcCommand;
pub use config::Config;
pub use error::{CliError, Result};
pub use output::{OutputFormat, render};
pub use transport::{ConnectOptions, Transport, connect};

use crate::cli::{ConfigAction, EncodingArg, GlobalArgs, TransportArg};
use clap::CommandFactory;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_wrpc_client::WrpcEncoding;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

/// Default request/connect timeout when none is configured.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Program name registered with the OS and passed to the completion generator.
/// Must match the `[[bin]]` name in `Cargo.toml` and `#[command(name)]` on
/// [`Cli`]. The hyphen is the reason [`generate_completion`] needs a fixup.
const BIN_NAME: &str = "kaspa-rpc";

/// Run the CLI to completion, returning the process exit code.
pub async fn run(cli: Cli) -> Result<ExitCode> {
    // Connection-free commands are handled before any config/connect work.
    match &cli.command {
        Commands::Completion { shell } => {
            generate_completion(*shell, &mut std::io::stdout())?;
            return Ok(ExitCode::SUCCESS);
        }
        Commands::Config(cfg_cmd) => {
            handle_config(&cli.global, &cfg_cmd.action)?;
            return Ok(ExitCode::SUCCESS);
        }
        _ => {}
    }

    let config = Config::load(cli.global.config.as_deref(), cli.global.no_config)?;
    let opts = build_connect_options(&cli.global, &config)?;
    let format = resolve_output_format(&cli.global, &config);

    let client = connect(&opts).await?;

    // `subscribe` streams notifications to stdout itself and never returns a
    // single value, so it bypasses the request/response render path.
    if let Commands::Subscribe(sub) = &cli.command {
        sub.run(&client, format).await?;
        return Ok(ExitCode::SUCCESS);
    }

    println!("{}", dispatch(&cli.command, &client, format).await?);
    Ok(ExitCode::SUCCESS)
}

/// Generate a shell completion script for the binary and write it to `out`.
///
/// clap_complete 4.x renders a hyphen in the bin name two inconsistent ways
/// inside the generated bash script: the word-walker that tracks the current
/// command uses `-` -> `__`, while the nested option `case` labels use
/// `-` -> `__subcmd__`. For a hyphenated bin name the two never match, so
/// completion past the top level (e.g. `kaspa-rpc subscribe <Tab>`) silently
/// yields nothing. Until the upstream fix lands, rewrite the bash output so the
/// `case` labels use the same form as the walker. Other shells render the bin
/// name consistently and are emitted verbatim.
fn generate_completion(shell: clap_complete::Shell, out: &mut impl std::io::Write) -> Result<()> {
    let mut cmd = Cli::command();
    let mut script = Vec::new();
    clap_complete::generate(shell, &mut cmd, BIN_NAME, &mut script);

    if shell == clap_complete::Shell::Bash && BIN_NAME.contains('-') {
        let script = String::from_utf8(script).expect("clap_complete emits utf-8");
        let mangled = BIN_NAME.replace('-', "__subcmd__");
        let walker = BIN_NAME.replace('-', "__");
        out.write_all(script.replace(&mangled, &walker).as_bytes())?;
    } else {
        out.write_all(&script)?;
    }
    Ok(())
}

/// Dispatch a parsed command to its handler, returning the rendered output.
/// Connection-free commands are handled earlier in [`run`] and are unreachable
/// here. Each method variant runs its [`RpcCommand`] and emits the typed
/// response via [`crate::output::emit`].
async fn dispatch(command: &Commands, client: &Arc<dyn RpcApi>, format: OutputFormat) -> Result<String> {
    macro_rules! dispatch_arms {
        ($($v:ident),+ $(,)?) => {
            match command {
                $( Commands::$v(c) => crate::output::emit(&c.run(client).await?, format), )+
                Commands::Subscribe(_) | Commands::Completion { .. } | Commands::Config(_) => {
                    unreachable!("subscribe and connection-free commands are handled before dispatch")
                }
            }
        };
    }

    dispatch_arms!(
        GetInfo,
        GetServerInfo,
        GetSyncStatus,
        GetCurrentNetwork,
        GetSystemInfo,
        GetConnections,
        GetMetrics,
        GetBlockDagInfo,
        GetBlockCount,
        GetCoinSupply,
        GetSink,
        GetSinkBlueScore,
        GetBlockRewardInfo,
        GetConnectedPeerInfo,
        GetPeerAddresses,
        GetFeeEstimate,
        GetFeeEstimateExperimental,
        Ping,
        Shutdown,
        GetBlock,
        GetBlocks,
        GetHeaders,
        GetCurrentBlockColor,
        GetVirtualChainFromBlock,
        GetVirtualChainFromBlockV2,
        GetDaaScoreTimestampEstimate,
        EstimateNetworkHashesPerSecond,
        GetSubnetwork,
        GetSeqCommitLaneProof,
        GetMempoolEntry,
        GetMempoolEntries,
        GetMempoolEntriesByAddresses,
        GetUtxosByAddresses,
        GetBalanceByAddress,
        GetBalancesByAddresses,
        GetUtxoReturnAddress,
        GetBlockTemplate,
        SubmitBlock,
        SubmitTransaction,
        SubmitTransactionReplacement,
        AddPeer,
        Ban,
        Unban,
        ResolveFinalityConflict,
        Call,
    )
}

/// Resolve connection options with precedence: CLI flags > env > config file.
/// (Env is already folded into `config` by [`Config::load`].)
fn build_connect_options(global: &GlobalArgs, config: &Config) -> Result<ConnectOptions> {
    let url = global.url.clone().or_else(|| config.url.clone());

    let network = match &global.network {
        Some(s) => NetworkId::from_str(s).map_err(|e| CliError::Usage(format!("invalid --network '{s}': {e}")))?,
        None => config.network.unwrap_or_else(|| NetworkId::new(NetworkType::Mainnet)),
    };

    let transport = match global.transport {
        Some(TransportArg::Grpc) => Some(Transport::Grpc),
        Some(TransportArg::Wrpc) => Some(Transport::Wrpc),
        None => config.transport,
    };

    let encoding = match global.encoding.or(config.encoding) {
        Some(EncodingArg::Borsh) | None => WrpcEncoding::Borsh,
        Some(EncodingArg::Json) => WrpcEncoding::SerdeJson,
    };

    let timeout_ms =
        global.timeout.or(config.timeout).map(|s| s.saturating_mul(1000)).unwrap_or_else(|| DEFAULT_TIMEOUT.as_millis() as u64);

    Ok(ConnectOptions { url, network, transport, encoding, timeout_ms })
}

/// Resolve the output format with precedence: `--json` / `--output` > config > default text.
fn resolve_output_format(global: &GlobalArgs, config: &Config) -> OutputFormat {
    if global.json {
        return OutputFormat::Json;
    }
    global.output.or(config.output).unwrap_or_default()
}

/// Handle the `config` subcommand (path / show / set / unset).
fn handle_config(global: &GlobalArgs, action: &ConfigAction) -> Result<()> {
    match action {
        ConfigAction::Path => {
            let path = global
                .config
                .clone()
                .or_else(Config::default_path)
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<none>".to_string());
            println!("{path}");
        }
        ConfigAction::Show => {
            let config = Config::load(global.config.as_deref(), global.no_config)?;
            let format = resolve_output_format(global, &config);
            let value = serde_json::to_value(&config)?;
            println!("{}", render(&value, format));
        }
        ConfigAction::Set { key, value } => edit_config(global, key.as_str(), Some(value))?,
        ConfigAction::Unset { key } => edit_config(global, key.as_str(), None)?,
    }
    Ok(())
}

/// Apply a `config set` (`Some`) or `config unset` (`None`) edit to the config
/// file, then write it back. A confirmation is printed to stderr unless quiet.
fn edit_config(global: &GlobalArgs, key: &str, value: Option<&str>) -> Result<()> {
    let path = config_target_path(global)?;
    let mut config = Config::from_file_or_default(&path)?;
    config.set_field(key, value)?;
    config.save(&path)?;
    if !global.quiet {
        let verb = if value.is_some() { "set" } else { "unset" };
        eprintln!("kaspa-rpc: {verb} {key} in {}", path.display());
    }
    Ok(())
}

/// The config file path to edit: the `--config` override, else the default path.
fn config_target_path(global: &GlobalArgs) -> Result<PathBuf> {
    global
        .config
        .clone()
        .or_else(Config::default_path)
        .ok_or_else(|| CliError::Config("no config file path available; pass --config <PATH>".to_string()))
}
