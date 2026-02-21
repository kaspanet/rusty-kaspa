use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use kaspa_addresses::Prefix;
use kaspa_wrpc_client::prelude::{NetworkId, NetworkType};

use zk_covenant_rollup_tui::app::App;
use zk_covenant_rollup_tui::db::RollupDb;
use zk_covenant_rollup_tui::node::KaspaNode;

#[derive(Parser, Debug)]
#[command(name = "zk-covenant-rollup-tui")]
#[command(about = "Interactive TUI for the ZK Covenant Rollup (testnet-12)")]
struct Args {
    /// wRPC endpoint URL
    #[arg(long, default_value = "ws://127.0.0.1:17210")]
    wrpc_url: String,

    /// Path to the RocksDB database directory
    #[arg(long, default_value = "./rollup-db")]
    db_path: PathBuf,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    let network_id = NetworkId::with_suffix(NetworkType::Testnet, 12);
    let prefix = Prefix::Testnet;

    // Open database
    let db = Arc::new(RollupDb::open(&args.db_path)?);

    // Connect to Kaspa node (pre-TUI, blocking)
    eprintln!("Connecting to {} ...", args.wrpc_url);
    let node = KaspaNode::try_new(&args.wrpc_url, network_id)?;
    node.connect().await?;

    // Get initial chain info
    let dag_info = node.get_block_dag_info().await?;

    // Build app state
    let mut app = App::new(db, node.clone(), prefix);
    app.daa_score = dag_info.virtual_daa_score;
    app.pruning_point = dag_info.pruning_point_hash;
    app.connected = true;
    app.log(format!("Connected to {} (DAA: {})", args.wrpc_url, dag_info.virtual_daa_score));

    // Auto-select first covenant if available
    app.auto_select_first_covenant();

    // Run TUI
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal).await;
    ratatui::restore();

    // Shutdown node
    node.stop().await?;

    result
}
