//! Stratum Bridge Service - Direct integration into kaspad
//! Implements AsyncService to run as part of kaspad's async runtime
//! Supports both single-instance (via --stratum-port) and multi-instance (via --stratum-config) modes

use async_trait::async_trait;
use kaspa_addresses::Address;
use kaspa_consensus_core::block::Block;
use kaspa_core::task::service::{AsyncService, AsyncServiceFuture};
use kaspa_rpc_core::{
    api::rpc::RpcApi, EstimateNetworkHashesPerSecondRequest, GetBalancesByAddressesRequest, GetBlockDagInfoRequest,
    GetBlockTemplateRequest, GetSyncStatusRequest, RpcAddress, SubmitBlockRequest,
};
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_stratum_bridge::{listen_and_serve, prom, BridgeConfig, KaspaApiTrait};
use log::{error, info, warn};
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use yaml_rust::YamlLoader;

/// Adapter that wraps RpcCoreService to implement KaspaApiTrait
/// This allows the bridge to use RpcCoreService directly without creating a gRPC connection
pub struct RpcCoreKaspaApi {
    rpc: Arc<RpcCoreService>,
}

impl RpcCoreKaspaApi {
    pub fn new(rpc: Arc<RpcCoreService>) -> Self {
        Self { rpc }
    }
}

#[async_trait]
impl KaspaApiTrait for RpcCoreKaspaApi {
    async fn get_block_template(
        &self,
        wallet_addr: &str,
        _remote_app: &str,
        _canxium_addr: &str,
    ) -> Result<Block, Box<dyn std::error::Error + Send + Sync>> {
        let address = Address::try_from(wallet_addr)?;
        let response = self.rpc.get_block_template_call(None, GetBlockTemplateRequest::new(address, vec![])).await?;
        let rpc_block = response.block;
        let block = Block::try_from(rpc_block)?;
        Ok(block)
    }

    async fn submit_block(
        &self,
        block: Block,
    ) -> Result<kaspa_rpc_core::SubmitBlockResponse, Box<dyn std::error::Error + Send + Sync>> {
        let rpc_block = (&block).into();
        let resp = self.rpc.submit_block_call(None, SubmitBlockRequest::new(rpc_block, false)).await?;
        Ok(resp)
    }

    async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<(String, u64)>, Box<dyn std::error::Error + Send + Sync>> {
        // Convert String addresses to RpcAddress (which is kaspa_addresses::Address)
        let rpc_addresses: Result<Vec<RpcAddress>, _> =
            addresses.iter().map(|addr_str| Address::try_from(addr_str.as_str())).collect();
        let rpc_addresses = rpc_addresses?;

        let req = GetBalancesByAddressesRequest::new(rpc_addresses);
        let resp = self.rpc.get_balances_by_addresses_call(None, req).await?;
        let balances = resp.entries.into_iter().map(|item| (item.address.to_string(), item.balance.unwrap_or(0))).collect();
        Ok(balances)
    }
}

/// Instance-specific configuration
#[derive(Debug, Clone)]
struct InstanceConfig {
    stratum_port: String,
    min_share_diff: u32,
    prom_port: Option<String>,
    log_to_file: Option<bool>,
    var_diff: Option<bool>,
    shares_per_min: Option<u32>,
    var_diff_stats: Option<bool>,
    pow2_clamp: Option<bool>,
}

/// Global configuration (shared across all instances)
#[derive(Debug, Clone)]
struct GlobalConfig {
    kaspad_address: String,
    block_wait_time: Duration,
    print_stats: bool,
    log_to_file: bool,
    health_check_port: String,
    var_diff: bool,
    shares_per_min: u32,
    var_diff_stats: bool,
    extranonce_size: u8,
    pow2_clamp: bool,
}

/// Bridge configuration (supports multi-instance mode)
#[derive(Debug)]
struct BridgeConfigYaml {
    global: GlobalConfig,
    instances: Vec<InstanceConfig>,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            kaspad_address: "127.0.0.1:16110".to_string(),
            block_wait_time: Duration::from_millis(1000),
            print_stats: true,
            log_to_file: false, // Use kaspad's logging in direct mode
            health_check_port: String::new(),
            var_diff: false,
            shares_per_min: 20,
            var_diff_stats: false,
            extranonce_size: 2,
            pow2_clamp: true,
        }
    }
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            stratum_port: ":5555".to_string(),
            min_share_diff: 8192,
            prom_port: None,
            log_to_file: None,
            var_diff: None,
            shares_per_min: None,
            var_diff_stats: None,
            pow2_clamp: None,
        }
    }
}

impl BridgeConfigYaml {
    fn from_yaml(content: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let docs = YamlLoader::load_from_str(content)?;
        let doc = docs.first().ok_or("empty YAML document")?;

        let mut global = GlobalConfig::default();

        if let Some(addr) = doc["kaspad_address"].as_str() {
            global.kaspad_address = addr.to_string();
        }
        if let Some(ms) = doc["block_wait_time"].as_i64() {
            global.block_wait_time = Duration::from_millis(ms as u64);
        }
        if let Some(val) = doc["print_stats"].as_bool() {
            global.print_stats = val;
        }
        if let Some(val) = doc["log_to_file"].as_bool() {
            global.log_to_file = val;
        }
        if let Some(port) = doc["health_check_port"].as_str() {
            global.health_check_port = port.to_string();
        }
        if let Some(val) = doc["var_diff"].as_bool() {
            global.var_diff = val;
        }
        if let Some(val) = doc["shares_per_min"].as_i64() {
            global.shares_per_min = val as u32;
        }
        if let Some(val) = doc["var_diff_stats"].as_bool() {
            global.var_diff_stats = val;
        }
        if let Some(val) = doc["extranonce_size"].as_i64() {
            global.extranonce_size = val as u8;
        }
        if let Some(val) = doc["pow2_clamp"].as_bool() {
            global.pow2_clamp = val;
        }

        let mut instances = Vec::new();
        if let Some(insts) = doc["instances"].as_vec() {
            // Multi-instance mode
            for (idx, inst) in insts.iter().enumerate() {
                let mut i = InstanceConfig::default();
                if let Some(port) = inst["stratum_port"].as_str() {
                    i.stratum_port = if port.starts_with(':') { port.to_string() } else { format!(":{}", port) };
                } else {
                    return Err(format!("Instance {} missing required 'stratum_port'", idx).into());
                }
                if let Some(diff) = inst["min_share_diff"].as_i64() {
                    i.min_share_diff = diff as u32;
                } else {
                    return Err(format!("Instance {} missing required 'min_share_diff'", idx).into());
                }
                if let Some(port) = inst["prom_port"].as_str() {
                    i.prom_port = Some(if port.starts_with(':') { port.to_string() } else { format!(":{}", port) });
                }
                if let Some(val) = inst["log_to_file"].as_bool() {
                    i.log_to_file = Some(val);
                }
                if let Some(val) = inst["var_diff"].as_bool() {
                    i.var_diff = Some(val);
                }
                if let Some(val) = inst["shares_per_min"].as_i64() {
                    i.shares_per_min = Some(val as u32);
                }
                if let Some(val) = inst["var_diff_stats"].as_bool() {
                    i.var_diff_stats = Some(val);
                }
                if let Some(val) = inst["pow2_clamp"].as_bool() {
                    i.pow2_clamp = Some(val);
                }
                instances.push(i);
            }
        } else {
            // Single instance mode - use defaults
            instances.push(InstanceConfig::default());
        }

        if instances.is_empty() {
            return Err("instances array cannot be empty".into());
        }

        // Validate unique ports
        let mut ports = std::collections::HashSet::new();
        for instance in &instances {
            if !ports.insert(&instance.stratum_port) {
                return Err(format!("Duplicate stratum_port: {}", instance.stratum_port).into());
            }
        }

        Ok(Self { global, instances })
    }
}

/// Stratum Bridge Service
/// Runs the Stratum bridge directly inside kaspad using RpcCoreService
/// Supports both single-instance (via --stratum-port) and multi-instance (via --stratum-config) modes
pub struct StratumBridgeService {
    rpc_core: Arc<RpcCoreService>,
    stratum_port: Option<String>,
    stratum_config: Option<String>,
    shutdown: Arc<kaspa_utils::triggers::SingleTrigger>,
}

impl StratumBridgeService {
    pub const IDENT: &'static str = "stratum-bridge";

    pub fn new(rpc_core: Arc<RpcCoreService>, stratum_port: Option<String>, stratum_config: Option<String>) -> Self {
        Self { rpc_core, stratum_port, stratum_config, shutdown: Arc::new(kaspa_utils::triggers::SingleTrigger::default()) }
    }
}

impl AsyncService for StratumBridgeService {
    fn ident(self: Arc<Self>) -> &'static str {
        Self::IDENT
    }

    fn start(self: Arc<Self>) -> AsyncServiceFuture {
        let shutdown_signal = self.shutdown.listener.clone();
        let rpc_core = Arc::clone(&self.rpc_core);
        let stratum_port = self.stratum_port.clone();
        let stratum_config = self.stratum_config.clone();

        Box::pin(async move {
            // Initialize tracing subscriber to make bridge logs visible
            // The bridge crate uses tracing macros, so we need to initialize a subscriber
            use std::sync::Once;
            static INIT_TRACING: Once = Once::new();
            INIT_TRACING.call_once(|| {
                use tracing_subscriber::fmt;
                use tracing_subscriber::prelude::*;
                use tracing_subscriber::EnvFilter;

                // Get log level from RUST_LOG or default to info for bridge
                // Allow info level for kaspa_stratum_bridge crate to see mining activity
                let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,kaspa_stratum_bridge=info"));

                // Create a tracing subscriber that outputs to stderr (same as kaspad's logging)
                // Format: timestamp level message (compact, matches kaspad style)
                let _ = tracing_subscriber::registry()
                    .with(filter)
                    .with(
                        fmt::layer()
                            .with_writer(std::io::stderr)
                            .with_ansi(true)
                            .with_target(false) // Hide module path to reduce clutter
                            .with_level(true) // Show log level
                            .compact(), // Compact format (no extra spacing)
                    )
                    .try_init();
            });

            // Wait for node to sync before starting the Stratum server
            info!("[stratum-bridge] Waiting for Kaspa node to sync...");
            loop {
                match rpc_core.get_sync_status_call(None, GetSyncStatusRequest {}).await {
                    Ok(response) => {
                        if response.is_synced {
                            info!("[stratum-bridge] Kaspa node is synced, starting Stratum server");
                            break;
                        }
                    }
                    Err(e) => {
                        warn!("[stratum-bridge] Failed to get sync status: {}, retrying...", e);
                    }
                }
                info!("[stratum-bridge] Kaspa node is not synced, waiting for sync before starting Stratum server...");
                sleep(Duration::from_secs(5)).await;
            }

            let stats_shutdown_signal = shutdown_signal.clone();
            let rpc_core_stats = Arc::clone(&rpc_core);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(30));
                loop {
                    tokio::select! {
                        _ = stats_shutdown_signal.clone() => {
                            break;
                        }
                        _ = interval.tick() => {
                            let dag_response = match rpc_core_stats.get_block_dag_info_call(None, GetBlockDagInfoRequest {}).await {
                                Ok(r) => r,
                                Err(e) => {
                                    warn!("[stratum-bridge] failed to get block DAG info for prom network stats: {}", e);
                                    continue;
                                }
                            };

                            let tip_hash = match dag_response.tip_hashes.first() {
                                Some(hash) => Some(*hash),
                                None => {
                                    warn!("[stratum-bridge] no tip hashes available for prom network stats");
                                    continue;
                                }
                            };

                            let hashrate_response = match rpc_core_stats
                                .estimate_network_hashes_per_second_call(None, EstimateNetworkHashesPerSecondRequest::new(1000, tip_hash))
                                .await
                            {
                                Ok(r) => r,
                                Err(e) => {
                                    warn!("[stratum-bridge] failed to estimate network hashrate for prom network stats: {}", e);
                                    continue;
                                }
                            };

                            prom::record_network_stats(
                                hashrate_response.network_hashes_per_second,
                                dag_response.block_count,
                                dag_response.difficulty,
                            );
                        }
                    }
                }
            });

            // Create RpcCoreKaspaApi adapter
            let api = Arc::new(RpcCoreKaspaApi::new(rpc_core));

            // Determine configuration mode
            let config = if let Some(config_path) = stratum_config {
                // Multi-instance mode: load from config file
                // Try multiple paths: absolute path, current dir, relative to executable
                let resolved_path = if std::path::Path::new(&config_path).is_absolute() {
                    config_path.clone()
                } else if std::path::Path::new(&config_path).exists() {
                    // Try current directory first
                    config_path.clone()
                } else {
                    // Try relative to executable directory
                    if let Ok(exe_path) = std::env::current_exe() {
                        if let Some(exe_dir) = exe_path.parent() {
                            let exe_config_path = exe_dir.join(&config_path);
                            if exe_config_path.exists() {
                                exe_config_path.to_string_lossy().to_string()
                            } else {
                                config_path.clone()
                            }
                        } else {
                            config_path.clone()
                        }
                    } else {
                        config_path.clone()
                    }
                };

                info!("[stratum-bridge] Loading configuration from: {}", resolved_path);
                let content = match fs::read_to_string(&resolved_path) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("[stratum-bridge] Failed to read config file {}: {}", resolved_path, e);
                        error!("[stratum-bridge] Tried path: {}", resolved_path);
                        return Err(kaspa_core::task::service::AsyncServiceError::Service(format!(
                            "Failed to read config file '{}': {}. Make sure the file exists in the current directory or provide an absolute path.",
                            resolved_path, e
                        )));
                    }
                };

                match BridgeConfigYaml::from_yaml(&content) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("[stratum-bridge] Failed to parse config file {}: {}", config_path, e);
                        return Err(kaspa_core::task::service::AsyncServiceError::Service(format!(
                            "Failed to parse config file: {}",
                            e
                        )));
                    }
                }
            } else if let Some(port) = stratum_port {
                // Single-instance mode: use command-line port
                info!("[stratum-bridge] Starting single-instance mode on port: {}", port);
                BridgeConfigYaml {
                    global: GlobalConfig::default(),
                    instances: vec![InstanceConfig {
                        stratum_port: if port.starts_with(':') { port } else { format!(":{}", port) },
                        min_share_diff: 8192,
                        prom_port: None,
                        log_to_file: Some(false), // Use kaspad's logging
                        var_diff: None,
                        shares_per_min: None,
                        var_diff_stats: None,
                        pow2_clamp: None,
                    }],
                }
            } else {
                // Default: single instance on :5555
                info!("[stratum-bridge] Starting single-instance mode on default port: :5555");
                BridgeConfigYaml {
                    global: GlobalConfig::default(),
                    instances: vec![InstanceConfig {
                        stratum_port: ":5555".to_string(),
                        min_share_diff: 8192,
                        prom_port: None,
                        log_to_file: Some(false),
                        var_diff: None,
                        shares_per_min: None,
                        var_diff_stats: None,
                        pow2_clamp: None,
                    }],
                }
            };

            // Spawn a task for each instance
            let mut handles = Vec::new();
            for (idx, instance) in config.instances.iter().enumerate() {
                let instance_num = idx + 1;
                let api_clone = api.clone();
                let instance = instance.clone();
                let global = config.global.clone();

                let handle = tokio::spawn(async move {
                    // Start instance-specific Prometheus server if configured
                    if let Some(ref prom_port) = instance.prom_port {
                        if !prom_port.is_empty() {
                            let prom_port = prom_port.clone();
                            let instance_num_prom = instance_num;
                            tokio::spawn(async move {
                                if let Err(e) = prom::start_prom_server(&prom_port).await {
                                    error!("[Instance {}] Prometheus server error: {}", instance_num_prom, e);
                                }
                            });
                        }
                    }

                    let bridge_config = BridgeConfig {
                        instance_id: format!("[Instance {}]", instance_num),
                        stratum_port: instance.stratum_port.clone(),
                        kaspad_address: global.kaspad_address.clone(), // Not used in direct mode, but required
                        prom_port: instance.prom_port.clone().unwrap_or_default(),
                        print_stats: global.print_stats,
                        log_to_file: instance.log_to_file.unwrap_or(global.log_to_file),
                        health_check_port: global.health_check_port.clone(),
                        block_wait_time: global.block_wait_time,
                        min_share_diff: instance.min_share_diff,
                        var_diff: instance.var_diff.unwrap_or(global.var_diff),
                        shares_per_min: instance.shares_per_min.unwrap_or(global.shares_per_min),
                        var_diff_stats: instance.var_diff_stats.unwrap_or(global.var_diff_stats),
                        extranonce_size: global.extranonce_size,
                        pow2_clamp: instance.pow2_clamp.unwrap_or(global.pow2_clamp),
                    };

                    info!(
                        "[stratum-bridge] Starting instance {} on port {} (min_share_diff: {})",
                        instance_num, bridge_config.stratum_port, bridge_config.min_share_diff
                    );

                    if let Err(e) = listen_and_serve(bridge_config, api_clone, None).await {
                        error!("[stratum-bridge] Instance {} failed: {}", instance_num, e);
                    }
                });

                handles.push(handle);
            }

            // Wait for shutdown signal
            shutdown_signal.await;

            // Abort all bridge tasks on shutdown
            for handle in handles {
                handle.abort();
                let _ = handle.await;
            }

            Ok(())
        })
    }

    fn signal_exit(self: Arc<Self>) {
        self.shutdown.trigger.trigger();
    }

    fn stop(self: Arc<Self>) -> AsyncServiceFuture {
        Box::pin(async move { Ok(()) })
    }
}
