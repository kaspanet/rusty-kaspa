use clap::Parser;
use futures_util::future::try_join_all;
use kaspa_stratum_bridge::log_colors::LogColors;
use kaspa_stratum_bridge::{listen_and_serve, prom, BridgeConfig as StratumBridgeConfig, KaspaApi};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

use kaspad_lib::args as kaspad_args;

mod app_config;
mod health_check;
mod inprocess_node;
mod tracing_setup;

use app_config::BridgeConfig;
use inprocess_node::InProcessNode;

#[derive(Debug, Parser)]
#[command(author, version, about)]
struct Cli {
    #[arg(long, default_value = "config.yaml")]
    config: PathBuf,

    #[arg(long, value_enum)]
    node_mode: Option<NodeMode>,

    #[arg(long)]
    node_args: Option<String>,

    #[arg(long, action = clap::ArgAction::Append)]
    node_arg: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum NodeMode {
    External,
    Inprocess,
}

fn split_shell_words(input: &str) -> Result<Vec<String>, anyhow::Error> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let chars = input.chars().peekable();
    let mut quote: Option<char> = None;

    for ch in chars {
        match quote {
            Some(q) if ch == q => {
                quote = None;
            }
            Some(_) => {
                cur.push(ch);
            }
            None if ch == '"' || ch == '\'' => {
                quote = Some(ch);
            }
            None if ch.is_whitespace() => {
                if !cur.is_empty() {
                    out.push(cur.clone());
                    cur.clear();
                }
            }
            None => {
                cur.push(ch);
            }
        }
    }

    if quote.is_some() {
        return Err(anyhow::anyhow!("unterminated quote in --node-args"));
    }

    if !cur.is_empty() {
        out.push(cur);
    }

    Ok(out)
}

/// Log the bridge configuration at startup
fn log_bridge_configuration(config: &app_config::BridgeConfig) {
    let instance_count = config.instances.len();
    tracing::info!("----------------------------------");
    tracing::info!("initializing bridge ({} instance{})", instance_count, if instance_count > 1 { "s" } else { "" });
    tracing::info!("\tkaspad:          {} (shared)", config.global.kaspad_address);
    tracing::info!("\tblock wait:      {:?}", config.global.block_wait_time);
    tracing::info!("\tprint stats:     {}", config.global.print_stats);
    tracing::info!("\tvar diff:        {}", config.global.var_diff);
    tracing::info!("\tshares per min:  {}", config.global.shares_per_min);
    tracing::info!("\tvar diff stats:  {}", config.global.var_diff_stats);
    tracing::info!("\tpow2 clamp:      {}", config.global.pow2_clamp);
    tracing::info!("\textranonce:      auto-detected per client");
    tracing::info!("\thealth check:    {}", config.global.health_check_port);

    for (idx, instance) in config.instances.iter().enumerate() {
        tracing::info!("\t--- Instance {} ---", idx + 1);
        tracing::info!("\t  stratum:       {}", instance.stratum_port);
        tracing::info!("\t  min diff:      {}", instance.min_share_diff);
        if let Some(ref prom_port) = instance.prom_port {
            tracing::info!("\t  prom:          {}", prom_port);
        }
        if let Some(log_to_file) = instance.log_to_file {
            tracing::info!("\t  log to file:   {}", log_to_file);
        }
    }
    tracing::info!("----------------------------------");
}

async fn kaspa_api_with_retry(kaspad_address: String, block_wait_time: Duration) -> Result<Arc<KaspaApi>, anyhow::Error> {
    let mut last_err: Option<anyhow::Error> = None;
    for _ in 0..60 {
        match KaspaApi::new(kaspad_address.clone(), block_wait_time).await {
            Ok(api) => return Ok(api),
            Err(e) => {
                last_err = Some(anyhow::anyhow!("{}", e));
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    Err(last_err.unwrap_or_else(|| anyhow::anyhow!("failed to connect to kaspad")))
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let cli = Cli::parse();

    let mut node_args: Vec<String> = Vec::new();
    if let Some(node_args_str) = cli.node_args.as_deref() {
        node_args.extend(split_shell_words(node_args_str)?);
    }
    node_args.extend(cli.node_arg.iter().cloned());

    let inferred_mode = if !node_args.is_empty() { NodeMode::Inprocess } else { NodeMode::External };
    let node_mode = cli.node_mode.unwrap_or(inferred_mode);

    // Load config first to check if file logging is enabled
    let config_path = cli.config.as_path();
    let fallback_path = std::path::Path::new("bridge").join(config_path);
    // Build candidate paths for config file search:
    // 1. Direct path as specified
    // 2. Fallback path under ./bridge/
    // 3-5. Paths relative to executable directory (for different deployment scenarios)
    let exe_base = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf()));
    let exe_root = exe_base.as_ref().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|p| p.to_path_buf());

    let mut candidates: Vec<std::path::PathBuf> = vec![config_path.to_path_buf(), fallback_path.clone()];

    if config_path.is_relative() {
        if let Some(exe_base) = exe_base.as_ref() {
            candidates.push(exe_base.join(config_path));
        }
        if let Some(exe_root) = exe_root.as_ref() {
            candidates.push(exe_root.join(config_path));
            candidates.push(exe_root.join("bridge").join(config_path));
        }
    }

    let mut loaded_from: Option<std::path::PathBuf> = None;
    let mut config: Option<BridgeConfig> = None;
    for path in candidates.iter() {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            config = Some(BridgeConfig::from_yaml(&content)?);
            loaded_from = Some(path.clone());
            break;
        }
    }

    let config = config.unwrap_or_default();

    // Initialize color support detection
    LogColors::init();

    // Initialize tracing with WARN level by default (less verbose)
    // Can be overridden with RUST_LOG environment variable (e.g., RUST_LOG=info,debug)
    // To see more details, set RUST_LOG=info or RUST_LOG=debug
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        // Default: warn level, but allow info from the bridge. In inprocess mode we also
        // enable info-level logs from the embedded node (which uses the `log` crate).
        if node_mode == NodeMode::Inprocess {
            EnvFilter::new("warn,kaspa_stratum_bridge=info,kaspa=info,kaspad=info,kaspad_lib=info,log=info")
        } else {
            EnvFilter::new("warn,kaspa_stratum_bridge=info")
        }
    });

    // Note: The file_guard must be kept alive for the lifetime of the program
    // to ensure logs are flushed to the file
    let _file_guard = tracing_setup::init_tracing(&config, filter, node_mode == NodeMode::Inprocess);

    // Start in-process node after tracing is initialized so bridge logs (including the stats table)
    // are not filtered out by a tracing subscriber installed by kaspad.
    let mut inprocess_node: Option<InProcessNode> = None;
    if node_mode == NodeMode::Inprocess {
        let mut argv: Vec<OsString> = Vec::with_capacity(node_args.len() + 1);
        argv.push(OsString::from("kaspad"));
        argv.extend(node_args.iter().map(OsString::from));
        let args = kaspad_args::Args::parse(argv).map_err(|e| anyhow::anyhow!("{}", e))?;
        inprocess_node = Some(InProcessNode::start_from_args(args)?);
    }

    if loaded_from.is_none() {
        let cwd = std::env::current_dir().ok();
        tracing::warn!("config.yaml not found, using defaults (requested: {:?}, cwd: {:?})", config_path, cwd);
    }

    log_bridge_configuration(&config);

    // Start global health check server if port is specified
    if !config.global.health_check_port.is_empty() {
        let health_port = config.global.health_check_port.clone();
        health_check::spawn_health_check_server(health_port);
    }

    // Create shared kaspa API client (all instances use the same node)
    let kaspa_api = if inprocess_node.is_some() {
        kaspa_api_with_retry(config.global.kaspad_address.clone(), config.global.block_wait_time)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Kaspa API client: {}", e))?
    } else {
        KaspaApi::new(config.global.kaspad_address.clone(), config.global.block_wait_time)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to create Kaspa API client: {}", e))?
    };

    let mut instance_handles = Vec::new();
    for (idx, instance_config) in config.instances.iter().enumerate() {
        let instance_num = idx + 1;
        let instance = instance_config.clone();
        let global = config.global.clone();
        let kaspa_api_clone = Arc::clone(&kaspa_api);

        let is_first_instance = idx == 0;

        let instance_id_str = LogColors::format_instance_id(instance_num);

        if let Some(ref prom_port) = instance.prom_port {
            let prom_port = prom_port.clone();
            let instance_num_prom = instance_num;
            let instance_id_prom = instance_id_str.clone();
            tokio::spawn(async move {
                if let Err(e) = prom::start_prom_server(&prom_port, &instance_id_prom).await {
                    tracing::error!("[Instance {}] Prometheus server error: {}", instance_num_prom, e);
                }
            });
        }

        let handle = tokio::spawn(async move {
            tracing_setup::register_instance(instance_id_str.clone(), instance_num);

            let colored_instance_id = LogColors::format_instance_id(instance_num);
            tracing::info!("{} Starting on stratum port {}", colored_instance_id, instance.stratum_port);

            let bridge_config = StratumBridgeConfig {
                instance_id: instance_id_str.clone(),
                stratum_port: instance.stratum_port.clone(),
                kaspad_address: global.kaspad_address.clone(),
                prom_port: String::new(),
                print_stats: global.print_stats,
                log_to_file: instance.log_to_file.unwrap_or(global.log_to_file),
                health_check_port: String::new(),
                block_wait_time: global.block_wait_time,
                min_share_diff: instance.min_share_diff,
                var_diff: instance.var_diff.unwrap_or(global.var_diff),
                shares_per_min: instance.shares_per_min.unwrap_or(global.shares_per_min),
                var_diff_stats: instance.var_diff_stats.unwrap_or(global.var_diff_stats),
                extranonce_size: global.extranonce_size,
                pow2_clamp: instance.pow2_clamp.unwrap_or(global.pow2_clamp),
            };

            listen_and_serve(bridge_config, Arc::clone(&kaspa_api_clone), if is_first_instance { Some(kaspa_api_clone) } else { None })
                .await
                .map_err(|e| format!("[Instance {}] Bridge server error: {}", instance_num, e))
        });
        instance_handles.push(handle);
    }

    tracing::info!("All {} instance(s) started, waiting for completion...", config.instances.len());

    let bridge_fut = async {
        let result = try_join_all(instance_handles).await;
        match result {
            Ok(_) => {
                tracing::info!("All instances completed successfully");
                Ok(())
            }
            Err(e) => {
                tracing::error!("One or more instances failed: {:?}", e);
                Err(anyhow::anyhow!("Instance error: {:?}", e))
            }
        }
    };

    tokio::select! {
        res = bridge_fut => {
            if let Some(node) = inprocess_node {
                inprocess_node::shutdown_inprocess(node).await;
            }
            res
        }
        _ = tokio::signal::ctrl_c() => {
            if let Some(node) = inprocess_node {
                inprocess_node::shutdown_inprocess(node).await;
            }
            Ok(())
        }
    }
}
