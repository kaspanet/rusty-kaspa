//! Node management: starts `aerod` (unless one is already running), keeps a gRPC connection to it, and publishes a
//! snapshot of its state for the UI. All RPC work happens on a background thread; the UI only reads the snapshot.

use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

use aero_addresses::Address;
use aero_consensus_core::{
    config::params::Params,
    network::NetworkType,
    tx::{Transaction, TransactionOutpoint, UtxoEntry},
};
use aero_grpc_client::GrpcClient;
use aero_notify::subscription::context::SubscriptionContext;
use aero_rpc_core::{RpcTransaction, api::rpc::RpcApi, notify::mode::NotificationMode};
use tokio::sync::mpsc;

/// How many confirmations (in DAA score) an ordinary output needs before it counts as available.
pub const CONFIRMATIONS: u64 = 10;

#[derive(Clone, Debug)]
pub struct NodeConfig {
    pub network: NetworkType,
    /// Start `aerod` ourselves. Set to false to attach to a node that is already running.
    pub spawn: bool,
    pub aerod_path: PathBuf,
    pub rpc_address: String,
    /// Folder for the wallet file and (in `node/`) the node's chain data.
    pub data_dir: PathBuf,
    /// `--p2p-port`: the node's P2P listen port, when running several instances on one machine.
    pub p2p_port: Option<u16>,
    /// `--addpeer`: a peer to connect to at startup.
    pub add_peer: Option<String>,
}

#[derive(Clone, Debug)]
pub struct PeerRow {
    pub id: String,
    pub address: String,
    pub outbound: bool,
    pub ping_ms: u64,
    pub user_agent: String,
    pub protocol_version: u32,
    pub connected_secs: u64,
    pub ibd_peer: bool,
}

/// One unspent output belonging to the wallet.
#[derive(Clone, Debug)]
pub struct WalletUtxo {
    pub outpoint: TransactionOutpoint,
    pub entry: UtxoEntry,
    pub address: String,
    pub amount: u64,
    pub daa_score: u64,
    pub is_coinbase: bool,
}

#[derive(Clone, Debug, Default)]
pub struct NodeState {
    /// True once the gRPC connection to the node is established and answering.
    pub connected: bool,
    /// A human-readable status line (starting, connecting, error text...).
    pub status: String,
    pub synced: bool,
    pub utxo_indexed: bool,
    pub server_version: String,
    pub network: String,
    pub block_count: u64,
    pub header_count: u64,
    pub difficulty: f64,
    pub virtual_daa_score: u64,
    /// Median time of the latest blocks, in milliseconds since the Unix epoch.
    pub past_median_time: u64,
    pub mempool_size: u64,
    pub peers: Vec<PeerRow>,
    pub utxos: Vec<WalletUtxo>,
    /// Console output lines, oldest first.
    pub console: Vec<String>,
    /// Outcome of the last send: `Ok(txid)` or `Err(reason)`.
    pub send_result: Option<Result<String, String>>,
}

pub enum Cmd {
    /// A console command line.
    Console(String),
    /// Submit a signed transaction.
    Submit(Box<Transaction>),
}

pub struct Node {
    pub config: NodeConfig,
    pub state: Arc<Mutex<NodeState>>,
    watch: Arc<Mutex<Vec<Address>>>,
    cmd_tx: mpsc::UnboundedSender<Cmd>,
    child: Option<Child>,
}

impl Node {
    pub fn start(config: NodeConfig) -> Self {
        let state = Arc::new(Mutex::new(NodeState { status: "Starting node...".into(), ..Default::default() }));
        let watch: Arc<Mutex<Vec<Address>>> = Arc::default();
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();

        let child = if config.spawn { spawn_aerod(&config, &state) } else { None };

        {
            let (config, state, watch) = (config.clone(), Arc::clone(&state), Arc::clone(&watch));
            std::thread::Builder::new()
                .name("node-rpc".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_multi_thread()
                        .worker_threads(2)
                        .enable_all()
                        .build()
                        .expect("failed to start the async runtime");
                    runtime.block_on(run(config, state, watch, cmd_rx));
                })
                .expect("failed to start the RPC thread");
        }

        Self { config, state, watch, cmd_tx, child }
    }

    pub fn snapshot(&self) -> NodeState {
        self.state.lock().unwrap().clone()
    }

    /// Tells the node thread which wallet addresses to fetch outputs for.
    pub fn set_watch(&self, addresses: Vec<Address>) {
        *self.watch.lock().unwrap() = addresses;
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.cmd_tx.send(cmd);
    }

    pub fn clear_send_result(&self) {
        self.state.lock().unwrap().send_result = None;
    }

    /// Whether this app started (and therefore owns) the node process.
    pub fn owns_process(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn spawn_aerod(config: &NodeConfig, state: &Arc<Mutex<NodeState>>) -> Option<Child> {
    let mut command = Command::new(&config.aerod_path);
    command.arg("--utxoindex").arg(format!("--rpclisten={}", config.rpc_address));
    command.arg("--appdir").arg(config.data_dir.join("node"));
    if let Some(port) = config.p2p_port {
        command.arg(format!("--listen=0.0.0.0:{port}"));
    }
    if let Some(peer) = &config.add_peer {
        command.arg(format!("--addpeer={peer}"));
    }
    match config.network {
        NetworkType::Mainnet => {}
        NetworkType::Testnet => {
            command.arg("--testnet");
        }
        // Private test networks have no other miners, so let the node accept locally mined blocks while "unsynced".
        NetworkType::Devnet => {
            command.arg("--devnet").arg("--enable-unsynced-mining");
        }
        NetworkType::Simnet => {
            command.arg("--simnet").arg("--enable-unsynced-mining");
        }
    }
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    match command.spawn() {
        Ok(child) => Some(child),
        Err(e) => {
            state.lock().unwrap().status = format!("Could not start {}: {e}", config.aerod_path.display());
            None
        }
    }
}

async fn run(
    config: NodeConfig,
    state: Arc<Mutex<NodeState>>,
    watch: Arc<Mutex<Vec<Address>>>,
    mut cmd_rx: mpsc::UnboundedReceiver<Cmd>,
) {
    let subscription_context = SubscriptionContext::new();
    let mut client: Option<GrpcClient> = None;
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = tick.tick() => {
                if client.is_none() {
                    match connect(&config.rpc_address, &subscription_context).await {
                        Some(c) => client = Some(c),
                        None => {
                            let mut s = state.lock().unwrap();
                            s.connected = false;
                            if s.status.is_empty() || !s.status.starts_with("Could not start") {
                                s.status = "Waiting for the node to start...".into();
                            }
                            continue;
                        }
                    }
                }
                if let Some(c) = client.as_ref()
                    && let Err(e) = refresh(c, &config, &state, &watch).await
                {
                    let mut s = state.lock().unwrap();
                    s.connected = false;
                    s.status = format!("Lost connection to the node: {e}");
                    client = None;
                }
            }
            Some(cmd) = cmd_rx.recv() => {
                match client.as_ref() {
                    Some(c) => handle_command(c, cmd, &state).await,
                    None => {
                        let mut s = state.lock().unwrap();
                        match cmd {
                            Cmd::Console(line) => s.console.push(format!("> {line}\nError: not connected to the node")),
                            Cmd::Submit(_) => s.send_result = Some(Err("Not connected to the node".into())),
                        }
                    }
                }
            }
        }
    }
}

async fn connect(rpc_address: &str, ctx: &SubscriptionContext) -> Option<GrpcClient> {
    GrpcClient::connect_with_args(
        NotificationMode::Direct,
        format!("grpc://{rpc_address}"),
        Some(ctx.clone()),
        true,
        None,
        false,
        Some(500_000),
        Default::default(),
    )
    .await
    .ok()
}

async fn refresh(
    client: &GrpcClient,
    config: &NodeConfig,
    state: &Arc<Mutex<NodeState>>,
    watch: &Arc<Mutex<Vec<Address>>>,
) -> Result<(), String> {
    let info = client.get_info().await.map_err(|e| e.to_string())?;
    let dag = client.get_block_dag_info().await.map_err(|e| e.to_string())?;
    let peers = client.get_connected_peer_info().await.map_err(|e| e.to_string())?;

    let addresses = watch.lock().unwrap().clone();
    let utxos = if addresses.is_empty() || !info.is_utxo_indexed {
        Vec::new()
    } else {
        let entries = client.get_utxos_by_addresses(addresses).await.map_err(|e| e.to_string())?;
        entries
            .into_iter()
            .map(|entry| WalletUtxo {
                outpoint: TransactionOutpoint::from(entry.outpoint),
                address: entry.address.map(|a| a.to_string()).unwrap_or_default(),
                amount: entry.utxo_entry.amount,
                daa_score: entry.utxo_entry.block_daa_score,
                is_coinbase: entry.utxo_entry.is_coinbase,
                entry: UtxoEntry::from(entry.utxo_entry),
            })
            .collect()
    };

    let peer_rows = peers
        .peer_info
        .into_iter()
        .map(|p| PeerRow {
            id: p.id.to_string(),
            address: p.address.to_string(),
            outbound: p.is_outbound,
            ping_ms: p.last_ping_duration,
            user_agent: p.user_agent,
            protocol_version: p.advertised_protocol_version,
            connected_secs: p.time_connected / 1000,
            ibd_peer: p.is_ibd_peer,
        })
        .collect();

    let mut s = state.lock().unwrap();
    s.connected = true;
    s.status = "Connected".into();
    s.synced = info.is_synced;
    s.utxo_indexed = info.is_utxo_indexed;
    s.server_version = info.server_version;
    s.network = dag.network.to_string();
    s.block_count = dag.block_count;
    s.header_count = dag.header_count;
    s.difficulty = dag.difficulty;
    s.virtual_daa_score = dag.virtual_daa_score;
    s.past_median_time = dag.past_median_time;
    s.mempool_size = info.mempool_size;
    s.peers = peer_rows;
    s.utxos = utxos;
    let _ = config;
    Ok(())
}

async fn handle_command(client: &GrpcClient, cmd: Cmd, state: &Arc<Mutex<NodeState>>) {
    match cmd {
        Cmd::Console(line) => {
            let output = run_console_command(client, line.trim()).await;
            state.lock().unwrap().console.push(format!("> {line}\n{output}"));
        }
        Cmd::Submit(tx) => {
            let rpc_tx = RpcTransaction::from(&*tx);
            let result = client.submit_transaction(rpc_tx, false).await.map(|id| id.to_string()).map_err(|e| e.to_string());
            state.lock().unwrap().send_result = Some(result);
        }
    }
}

const CONSOLE_HELP: &str = "Available commands:\n  help\n  getinfo\n  getblockchaininfo\n  getblockcount\n  getdifficulty\n  getconnectioncount\n  getpeerinfo\n  getmempoolinfo\n  getnetworkinfo\n  stop (closes the application's node)";

async fn run_console_command(client: &GrpcClient, line: &str) -> String {
    match line {
        "" => String::new(),
        "help" => CONSOLE_HELP.to_string(),
        "getinfo" => match client.get_info().await {
            Ok(r) => format!("{r:#?}"),
            Err(e) => format!("Error: {e}"),
        },
        "getblockchaininfo" | "getblockdaginfo" => match client.get_block_dag_info().await {
            Ok(r) => format!("{r:#?}"),
            Err(e) => format!("Error: {e}"),
        },
        "getblockcount" => match client.get_block_dag_info().await {
            Ok(r) => r.block_count.to_string(),
            Err(e) => format!("Error: {e}"),
        },
        "getdifficulty" => match client.get_block_dag_info().await {
            Ok(r) => r.difficulty.to_string(),
            Err(e) => format!("Error: {e}"),
        },
        "getconnectioncount" => match client.get_connected_peer_info().await {
            Ok(r) => r.peer_info.len().to_string(),
            Err(e) => format!("Error: {e}"),
        },
        "getpeerinfo" => match client.get_connected_peer_info().await {
            Ok(r) => format!("{:#?}", r.peer_info),
            Err(e) => format!("Error: {e}"),
        },
        "getmempoolinfo" => match client.get_info().await {
            Ok(r) => format!("size: {}", r.mempool_size),
            Err(e) => format!("Error: {e}"),
        },
        "getnetworkinfo" => match client.get_server_info().await {
            Ok(r) => format!("{r:#?}"),
            Err(e) => format!("Error: {e}"),
        },
        other => format!("Command not found: {other}\nType \"help\" for the list of commands."),
    }
}

/// Default network parameters, for confirmation depth and block rate.
pub fn params_for(network: NetworkType) -> Params {
    Params::from(network)
}
