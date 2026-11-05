//! Command-line interface definition (clap).

use crate::commands::add_peer::AddPeer;
use crate::commands::ban::Ban;
use crate::commands::call::Call;
use crate::commands::estimate_network_hashes_per_second::EstimateNetworkHashesPerSecond;
use crate::commands::get_balance_by_address::GetBalanceByAddress;
use crate::commands::get_balances_by_addresses::GetBalancesByAddresses;
use crate::commands::get_block::GetBlock;
use crate::commands::get_block_count::GetBlockCount;
use crate::commands::get_block_dag_info::GetBlockDagInfo;
use crate::commands::get_block_reward_info::GetBlockRewardInfo;
use crate::commands::get_block_template::GetBlockTemplate;
use crate::commands::get_blocks::GetBlocks;
use crate::commands::get_coin_supply::GetCoinSupply;
use crate::commands::get_connected_peer_info::GetConnectedPeerInfo;
use crate::commands::get_connections::GetConnections;
use crate::commands::get_current_block_color::GetCurrentBlockColor;
use crate::commands::get_current_network::GetCurrentNetwork;
use crate::commands::get_daa_score_timestamp_estimate::GetDaaScoreTimestampEstimate;
use crate::commands::get_fee_estimate::GetFeeEstimate;
use crate::commands::get_fee_estimate_experimental::GetFeeEstimateExperimental;
use crate::commands::get_headers::GetHeaders;
use crate::commands::get_info::GetInfo;
use crate::commands::get_mempool_entries::GetMempoolEntries;
use crate::commands::get_mempool_entries_by_addresses::GetMempoolEntriesByAddresses;
use crate::commands::get_mempool_entry::GetMempoolEntry;
use crate::commands::get_metrics::GetMetrics;
use crate::commands::get_peer_addresses::GetPeerAddresses;
use crate::commands::get_seq_commit_lane_proof::GetSeqCommitLaneProof;
use crate::commands::get_server_info::GetServerInfo;
use crate::commands::get_sink::GetSink;
use crate::commands::get_sink_blue_score::GetSinkBlueScore;
use crate::commands::get_subnetwork::GetSubnetwork;
use crate::commands::get_sync_status::GetSyncStatus;
use crate::commands::get_system_info::GetSystemInfo;
use crate::commands::get_utxo_return_address::GetUtxoReturnAddress;
use crate::commands::get_utxos_by_addresses::GetUtxosByAddresses;
use crate::commands::get_virtual_chain_from_block::GetVirtualChainFromBlock;
use crate::commands::get_virtual_chain_from_block_v2::GetVirtualChainFromBlockV2;
use crate::commands::ping::Ping;
use crate::commands::resolve_finality_conflict::ResolveFinalityConflict;
use crate::commands::shutdown::Shutdown;
use crate::commands::submit_block::SubmitBlock;
use crate::commands::submit_transaction::SubmitTransaction;
use crate::commands::submit_transaction_replacement::SubmitTransactionReplacement;
use crate::commands::subscribe::Subscribe;
use crate::commands::unban::Unban;
use crate::output::OutputFormat;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

/// Top-level CLI: global connection/output flags plus a subcommand.
#[derive(Parser, Debug)]
#[command(name = "kaspa-rpc", version, about = "Non-interactive Kaspa RPC client", long_about = None)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Commands,
}

/// Connection, output and config flags shared by all subcommands.
#[derive(Args, Debug, Clone, Default)]
pub struct GlobalArgs {
    /// Endpoint URL. Transport is inferred from the scheme:
    /// `grpc://` => gRPC, `ws://`/`wss://` => wRPC.
    #[arg(long, global = true)]
    pub url: Option<String>,

    /// Network: mainnet | testnet-10 | testnet-11 | devnet | simnet (selects default ports).
    #[arg(long, global = true)]
    pub network: Option<String>,

    /// Explicit transport override (default: infer from URL, else wrpc).
    #[arg(long, global = true, value_enum)]
    pub transport: Option<TransportArg>,

    /// wRPC encoding (wRPC only; default borsh).
    #[arg(long, global = true, value_enum)]
    pub encoding: Option<EncodingArg>,

    /// Request/connect timeout in seconds.
    #[arg(long, global = true)]
    pub timeout: Option<u64>,

    /// Output format.
    #[arg(long, global = true, value_enum)]
    pub output: Option<OutputFormat>,

    /// Shorthand for `--output json`.
    #[arg(long = "json", global = true)]
    pub json: bool,

    /// Suppress non-essential logging (stderr).
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Increase log verbosity (stderr); repeatable.
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    /// Config file override.
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Ignore config files.
    #[arg(long, global = true)]
    pub no_config: bool,
}

/// Transport flag values.
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum TransportArg {
    Grpc,
    Wrpc,
}

/// Encoding flag values (wRPC).
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum, serde::Serialize, serde::Deserialize)]
#[clap(rename_all = "lower")]
#[serde(rename_all = "lowercase")]
pub enum EncodingArg {
    Borsh,
    Json,
}

/// Subcommand tree. RPC method variants are added per the `get-info` template;
/// `completion` and `config` are connection-free utility commands.
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Get general information about the node.
    GetInfo(GetInfo),
    /// Get server information about the node.
    GetServerInfo(GetServerInfo),
    /// Get whether the node is synced.
    GetSyncStatus(GetSyncStatus),
    /// Get the current network (mainnet, testnet, etc.).
    GetCurrentNetwork(GetCurrentNetwork),
    /// Get system information about the node host.
    GetSystemInfo(GetSystemInfo),
    /// Get active connection counts.
    GetConnections(GetConnections),
    /// Get node metrics.
    GetMetrics(GetMetrics),
    /// Get general information about the block DAG.
    GetBlockDagInfo(GetBlockDagInfo),
    /// Get the current block and header counts.
    GetBlockCount(GetBlockCount),
    /// Get the current circulating and max coin supply.
    GetCoinSupply(GetCoinSupply),
    /// Get the hash of the current sink (selected tip) block.
    GetSink(GetSink),
    /// Get the blue score of the current sink block.
    GetSinkBlueScore(GetSinkBlueScore),
    /// Get current block reward information.
    GetBlockRewardInfo(GetBlockRewardInfo),
    /// Get information about currently connected peers.
    GetConnectedPeerInfo(GetConnectedPeerInfo),
    /// Get known peer addresses.
    GetPeerAddresses(GetPeerAddresses),
    /// Get a fee-rate estimate for transactions.
    GetFeeEstimate(GetFeeEstimate),
    /// Get an experimental fee-rate estimate.
    GetFeeEstimateExperimental(GetFeeEstimateExperimental),
    /// Ping the node to check connectivity.
    Ping(Ping),
    /// Request the node to shut down.
    Shutdown(Shutdown),

    /// Get a block by hash.
    GetBlock(GetBlock),
    /// Get blocks following a low hash.
    GetBlocks(GetBlocks),
    /// Get block headers starting from a hash.
    GetHeaders(GetHeaders),
    /// Get the current color of a block.
    GetCurrentBlockColor(GetCurrentBlockColor),
    /// Get the virtual selected-parent chain from a block.
    GetVirtualChainFromBlock(GetVirtualChainFromBlock),
    /// Get the virtual chain from a block (v2).
    GetVirtualChainFromBlockV2(GetVirtualChainFromBlockV2),
    /// Estimate timestamps for given DAA scores.
    GetDaaScoreTimestampEstimate(GetDaaScoreTimestampEstimate),
    /// Estimate network hashes per second.
    EstimateNetworkHashesPerSecond(EstimateNetworkHashesPerSecond),
    /// Get information about a subnetwork.
    GetSubnetwork(GetSubnetwork),
    /// Get a sequence-commitment lane proof (KIP-21).
    GetSeqCommitLaneProof(GetSeqCommitLaneProof),

    /// Get a single mempool entry by transaction id.
    GetMempoolEntry(GetMempoolEntry),
    /// Get all mempool entries.
    GetMempoolEntries(GetMempoolEntries),
    /// Get mempool entries for given addresses.
    GetMempoolEntriesByAddresses(GetMempoolEntriesByAddresses),
    /// Get UTXOs for given addresses.
    GetUtxosByAddresses(GetUtxosByAddresses),
    /// Get the balance of an address.
    GetBalanceByAddress(GetBalanceByAddress),
    /// Get balances for given addresses.
    GetBalancesByAddresses(GetBalancesByAddresses),
    /// Get the return address for a spent UTXO.
    GetUtxoReturnAddress(GetUtxoReturnAddress),

    /// Get a block template for mining.
    GetBlockTemplate(GetBlockTemplate),
    /// Submit a block to the network.
    SubmitBlock(SubmitBlock),
    /// Submit a transaction to the mempool.
    SubmitTransaction(SubmitTransaction),
    /// Submit a replacement transaction (RBF).
    SubmitTransactionReplacement(SubmitTransactionReplacement),

    /// Add a peer to the address manager.
    AddPeer(AddPeer),
    /// Ban a peer IP address.
    Ban(Ban),
    /// Unban a peer IP address.
    Unban(Unban),
    /// Resolve a finality conflict.
    ResolveFinalityConflict(ResolveFinalityConflict),

    /// Subscribe to a notification scope and stream events until Ctrl-C.
    Subscribe(Subscribe),
    /// Invoke any covered RPC method by name with JSON parameters.
    Call(Call),

    /// Generate a shell completion script to stdout.
    Completion {
        /// Target shell.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Inspect resolved configuration.
    Config(ConfigCmd),
}

/// `config` subcommand group.
#[derive(Args, Debug)]
pub struct ConfigCmd {
    #[command(subcommand)]
    pub action: ConfigAction,
}

/// `config` actions.
#[derive(Subcommand, Debug)]
pub enum ConfigAction {
    /// Print the config file path that would be used.
    Path,
    /// Print the resolved configuration (file + environment).
    Show,
    /// Set a key in the config file, creating it (and parent dirs) if needed.
    Set {
        /// Configuration key to set.
        #[arg(value_enum)]
        key: ConfigKey,
        /// New value, validated against the key's type.
        value: String,
    },
    /// Remove a key from the config file.
    Unset {
        /// Configuration key to remove.
        #[arg(value_enum)]
        key: ConfigKey,
    },
}

/// Keys settable via `config set` / `config unset`. Mirrors the writable fields
/// of [`crate::config::Config`].
#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "lower")]
pub enum ConfigKey {
    Url,
    Network,
    Transport,
    Encoding,
    Output,
    Timeout,
}

impl ConfigKey {
    /// Canonical lower-case key name as written in the config file.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigKey::Url => "url",
            ConfigKey::Network => "network",
            ConfigKey::Transport => "transport",
            ConfigKey::Encoding => "encoding",
            ConfigKey::Output => "output",
            ConfigKey::Timeout => "timeout",
        }
    }
}
