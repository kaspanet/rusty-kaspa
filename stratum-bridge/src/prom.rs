use prometheus::{register_counter_vec, register_gauge, register_gauge_vec, CounterVec, Gauge, GaugeVec};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Worker labels for Prometheus metrics
const WORKER_LABELS: &[&str] = &["worker", "miner", "wallet", "ip"];

/// Invalid share type labels
const INVALID_LABELS: &[&str] = &["worker", "miner", "wallet", "ip", "type"];

/// Block labels
const BLOCK_LABELS: &[&str] = &["worker", "miner", "wallet", "ip", "nonce", "bluescore", "hash"];

/// Error labels
const ERROR_LABELS: &[&str] = &["wallet", "error"];

/// Balance labels
const BALANCE_LABELS: &[&str] = &["wallet"];

/// Share counter - number of valid shares found by worker
static SHARE_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Share difficulty counter - total difficulty of shares found by worker
static SHARE_DIFF_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Invalid share counter - number of invalid/stale/duplicate/weak shares
static INVALID_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Block counter - number of blocks mined
static BLOCK_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Block gauge - unique instances per block mined
static BLOCK_GAUGE: OnceLock<GaugeVec> = OnceLock::new();

/// Disconnect counter - number of disconnects by worker
static DISCONNECT_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Job counter - number of jobs sent to miner
static JOB_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Balance gauge - wallet balance for connected workers
static BALANCE_GAUGE: OnceLock<GaugeVec> = OnceLock::new();

/// Error counter - errors by worker
static ERROR_BY_WALLET: OnceLock<CounterVec> = OnceLock::new();

/// Estimated network hashrate gauge
static ESTIMATED_NETWORK_HASHRATE: OnceLock<Gauge> = OnceLock::new();

/// Network difficulty gauge
static NETWORK_DIFFICULTY: OnceLock<Gauge> = OnceLock::new();

/// Network block count gauge
static NETWORK_BLOCK_COUNT: OnceLock<Gauge> = OnceLock::new();

/// Worker start time gauge (Unix timestamp in seconds)
static WORKER_START_TIME: OnceLock<GaugeVec> = OnceLock::new();

/// Initialize Prometheus metrics
pub fn init_metrics() {
    SHARE_COUNTER.get_or_init(|| {
        register_counter_vec!("ks_valid_share_counter", "Number of shares found by worker over time", WORKER_LABELS).unwrap()
    });

    SHARE_DIFF_COUNTER.get_or_init(|| {
        register_counter_vec!("ks_valid_share_diff_counter", "Total difficulty of shares found by worker over time", WORKER_LABELS)
            .unwrap()
    });

    INVALID_COUNTER.get_or_init(|| {
        register_counter_vec!("ks_invalid_share_counter", "Number of stale shares found by worker over time", INVALID_LABELS).unwrap()
    });

    BLOCK_COUNTER.get_or_init(|| register_counter_vec!("ks_blocks_mined", "Number of blocks mined over time", WORKER_LABELS).unwrap());

    BLOCK_GAUGE.get_or_init(|| {
        register_gauge_vec!("ks_mined_blocks_gauge", "Gauge containing 1 unique instance per block mined", BLOCK_LABELS).unwrap()
    });

    DISCONNECT_COUNTER.get_or_init(|| {
        register_counter_vec!("ks_worker_disconnect_counter", "Number of disconnects by worker", WORKER_LABELS).unwrap()
    });

    JOB_COUNTER.get_or_init(|| {
        register_counter_vec!("ks_worker_job_counter", "Number of jobs sent to the miner by worker over time", WORKER_LABELS).unwrap()
    });

    BALANCE_GAUGE.get_or_init(|| {
        register_gauge_vec!(
            "ks_balance_by_wallet_gauge",
            "Gauge representing the wallet balance for connected workers",
            BALANCE_LABELS
        )
        .unwrap()
    });

    ERROR_BY_WALLET
        .get_or_init(|| register_counter_vec!("ks_worker_errors", "Gauge representing errors by worker", ERROR_LABELS).unwrap());

    ESTIMATED_NETWORK_HASHRATE.get_or_init(|| {
        register_gauge!("ks_estimated_network_hashrate_gauge", "Gauge representing the estimated network hashrate").unwrap()
    });

    NETWORK_DIFFICULTY
        .get_or_init(|| register_gauge!("ks_network_difficulty_gauge", "Gauge representing the network difficulty").unwrap());

    NETWORK_BLOCK_COUNT
        .get_or_init(|| register_gauge!("ks_network_block_count", "Gauge representing the network block count").unwrap());

    WORKER_START_TIME.get_or_init(|| {
        register_gauge_vec!("ks_worker_start_time", "Unix timestamp (seconds) when worker first connected", WORKER_LABELS).unwrap()
    });
}

/// Worker context for metrics
pub struct WorkerContext {
    pub worker_name: String,
    pub miner: String,
    pub wallet: String,
    pub ip: String,
}

impl WorkerContext {
    pub fn labels(&self) -> Vec<&str> {
        vec![&self.worker_name, &self.miner, &self.wallet, &self.ip]
    }
}

/// Record a valid share found
pub fn record_share_found(worker: &WorkerContext, share_diff: f64) {
    if let Some(counter) = SHARE_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }
    if let Some(counter) = SHARE_DIFF_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(share_diff);
    }
}

/// Record a stale share
pub fn record_stale_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("stale");
        counter.with_label_values(&labels).inc();
    }
}

/// Record a duplicate share
pub fn record_dupe_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("duplicate");
        counter.with_label_values(&labels).inc();
    }
}

/// Record an invalid share
pub fn record_invalid_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("invalid");
        counter.with_label_values(&labels).inc();
    }
}

/// Record a weak share
pub fn record_weak_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("weak");
        counter.with_label_values(&labels).inc();
    }
}

/// Record a block found
pub fn record_block_found(worker: &WorkerContext, nonce: u64, bluescore: u64, hash: String) {
    if let Some(counter) = BLOCK_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }
    if let Some(gauge) = BLOCK_GAUGE.get() {
        let mut labels = worker.labels();
        let nonce_str = nonce.to_string();
        let bluescore_str = bluescore.to_string();
        labels.push(&nonce_str);
        labels.push(&bluescore_str);
        labels.push(&hash);
        gauge.with_label_values(&labels).set(1.0);
    }
}

/// Record a disconnect
pub fn record_disconnect(worker: &WorkerContext) {
    if let Some(counter) = DISCONNECT_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }
}

/// Record a new job sent
pub fn record_new_job(worker: &WorkerContext) {
    if let Some(counter) = JOB_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }
}

/// Record network stats
pub fn record_network_stats(hashrate: u64, block_count: u64, difficulty: f64) {
    if let Some(gauge) = ESTIMATED_NETWORK_HASHRATE.get() {
        gauge.set(hashrate as f64);
    }
    if let Some(gauge) = NETWORK_DIFFICULTY.get() {
        gauge.set(difficulty);
    }
    if let Some(gauge) = NETWORK_BLOCK_COUNT.get() {
        gauge.set(block_count as f64);
    }
}

/// Record a worker error
pub fn record_worker_error(wallet: &str, error: &str) {
    if let Some(counter) = ERROR_BY_WALLET.get() {
        counter.with_label_values(&[wallet, error]).inc();
    }
}

/// Record wallet balances
pub fn record_balances(balances: &[(String, u64)]) {
    if let Some(gauge) = BALANCE_GAUGE.get() {
        for (address, balance) in balances {
            // Convert from sompi to KAS (divide by 100000000)
            let balance_kas = *balance as f64 / 100_000_000.0;
            gauge.with_label_values(&[address]).set(balance_kas);
        }
    }
}

/// Initialize worker counters (set to 0 to create the metric)
pub fn init_worker_counters(worker: &WorkerContext) {
    if let Some(counter) = SHARE_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    if let Some(counter) = SHARE_DIFF_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    if let Some(counter) = INVALID_COUNTER.get() {
        for error_type in &["stale", "duplicate", "invalid", "weak"] {
            let mut labels = worker.labels();
            labels.push(error_type);
            counter.with_label_values(&labels).inc_by(0.0);
        }
    }
    if let Some(counter) = BLOCK_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    if let Some(counter) = DISCONNECT_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    if let Some(counter) = JOB_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    // Set worker start time (Unix timestamp in seconds)
    if let Some(gauge) = WORKER_START_TIME.get() {
        let start_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as f64;
        gauge.with_label_values(&worker.labels()).set(start_time);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct StatsResponse {
    totalBlocks: u64,
    totalShares: u64,
    networkHashrate: u64,
    activeWorkers: usize,
    blocks: Vec<BlockInfo>,
    workers: Vec<WorkerInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BlockInfo {
    worker: String,
    wallet: String,
    hash: String,
    nonce: String,
    bluescore: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerInfo {
    worker: String,
    wallet: String,
    hashrate: f64,
    shares: u64,
    stale: u64,
    invalid: u64,
    blocks: u64,
}

/// Get stats as JSON
async fn get_stats_json() -> StatsResponse {
    use prometheus::gather;

    let metric_families = gather();
    let mut stats = StatsResponse {
        totalBlocks: 0,
        totalShares: 0,
        networkHashrate: 0,
        activeWorkers: 0,
        blocks: Vec::new(),
        workers: Vec::new(),
    };

    let mut worker_stats: HashMap<String, WorkerInfo> = HashMap::new();
    let mut worker_hash_values: HashMap<String, f64> = HashMap::new(); // Store hash values for hashrate calculation
    let mut worker_start_times: HashMap<String, f64> = HashMap::new(); // Store start times for hashrate calculation
    let mut block_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for family in metric_families {
        let name = family.get_name();

        // Parse block gauge
        if name == "ks_mined_blocks_gauge" {
            for metric in family.get_metric() {
                if metric.get_gauge().get_value() > 0.0 {
                    let labels = metric.get_label();
                    let mut worker = String::new();
                    let mut wallet = String::new();
                    let mut hash = String::new();
                    let mut nonce = String::new();
                    let mut bluescore = String::new();

                    for label in labels {
                        match label.get_name() {
                            "worker" => worker = label.get_value().to_string(),
                            "wallet" => wallet = label.get_value().to_string(),
                            "hash" => hash = label.get_value().to_string(),
                            "nonce" => nonce = label.get_value().to_string(),
                            "bluescore" => bluescore = label.get_value().to_string(),
                            _ => {}
                        }
                    }

                    if !hash.is_empty() && !block_set.contains(&hash) {
                        block_set.insert(hash.clone());
                        stats.blocks.push(BlockInfo { worker: worker.clone(), wallet: wallet.clone(), hash, nonce, bluescore });
                        stats.totalBlocks += 1;
                    }
                }
            }
        }

        // Parse block counter
        if name == "ks_blocks_mined" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}", worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    worker_stats
                        .entry(key.clone())
                        .or_insert_with(|| WorkerInfo {
                            worker: worker_key,
                            wallet,
                            hashrate: 0.0,
                            shares: 0,
                            stale: 0,
                            invalid: 0,
                            blocks: 0,
                        })
                        .blocks = count;
                }
            }
        }

        // Parse share diff counter (for hashrate calculation)
        if name == "ks_valid_share_diff_counter" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}", worker_key, wallet);
                    let total_hash_value = metric.get_counter().get_value();
                    // Store hash value for hashrate calculation
                    worker_hash_values.insert(key.clone(), total_hash_value);
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });
                }
            }
        }

        // Parse share counter
        if name == "ks_valid_share_counter" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}", worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    worker_stats
                        .entry(key.clone())
                        .or_insert_with(|| WorkerInfo {
                            worker: worker_key,
                            wallet,
                            hashrate: 0.0,
                            shares: 0,
                            stale: 0,
                            invalid: 0,
                            blocks: 0,
                        })
                        .shares = count;
                    stats.totalShares += count;
                }
            }
        }

        // Parse invalid share counter
        if name == "ks_invalid_share_counter" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut worker_key = String::new();
                let mut wallet = String::new();
                let mut share_type = String::new();

                for label in labels {
                    match label.get_name() {
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        "type" => share_type = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}", worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let worker = worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });

                    if share_type == "stale" {
                        worker.stale = count;
                    } else if share_type == "invalid" {
                        worker.invalid = count;
                    }
                }
            }
        }

        // Parse network hashrate
        if name == "ks_estimated_network_hashrate" {
            if let Some(metric) = family.get_metric().first() {
                stats.networkHashrate = metric.get_gauge().get_value() as u64;
            }
        }

        // Parse worker start time
        if name == "ks_worker_start_time" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}", worker_key, wallet);
                    let start_time_secs = metric.get_gauge().get_value();
                    worker_start_times.insert(key.clone(), start_time_secs);
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });
                }
            }
        }
    }

    // Calculate hashrate for workers using share_diff_counter and start_time
    let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as f64;

    let mut total_worker_hashrate_ghs = 0.0;

    // Calculate hashrate for each worker
    for (key, worker) in worker_stats.iter_mut() {
        if let (Some(&total_hash_value), Some(&start_time_secs)) = (worker_hash_values.get(key), worker_start_times.get(key)) {
            let elapsed = current_time - start_time_secs;
            // Calculate hashrate: total_hash_value / elapsed_time (in GH/s)
            // Matches console stats: hashrate = shares_diff / elapsed
            // Formula: hashrate = total_hash_value / elapsed (already in GH/s units)
            if elapsed > 0.0 && total_hash_value > 0.0 {
                worker.hashrate = total_hash_value / elapsed;
                total_worker_hashrate_ghs += worker.hashrate;
            }
        }
    }

    // If network hashrate is 0 or unavailable, use total worker hashrate as fallback
    // Convert from GH/s to H/s for network hashrate display
    if stats.networkHashrate == 0 && total_worker_hashrate_ghs > 0.0 {
        stats.networkHashrate = (total_worker_hashrate_ghs * 1e9) as u64;
    }

    stats.workers = worker_stats.into_values().collect();
    stats.activeWorkers = stats.workers.len();

    // Sort blocks by bluescore (newest first)
    stats.blocks.sort_by(|a, b| {
        let a_score: u64 = a.bluescore.parse().unwrap_or(0);
        let b_score: u64 = b.bluescore.parse().unwrap_or(0);
        b_score.cmp(&a_score)
    });

    // Sort workers by blocks (most blocks first)
    stats.workers.sort_by(|a, b| b.blocks.cmp(&a.blocks));

    stats
}

/// Get current config as JSON
async fn get_config_json() -> String {
    use std::fs;
    use yaml_rust::YamlLoader;

    let config_path = "config.yaml";
    if let Ok(content) = fs::read_to_string(config_path) {
        if let Ok(docs) = YamlLoader::load_from_str(&content) {
            if let Some(doc) = docs.first() {
                let mut config = serde_json::Map::new();

                if let Some(port) = doc["stratum_port"].as_str() {
                    config.insert("stratum_port".to_string(), serde_json::Value::String(port.to_string()));
                }
                if let Some(addr) = doc["kaspad_address"].as_str() {
                    config.insert("kaspad_address".to_string(), serde_json::Value::String(addr.to_string()));
                }
                if let Some(port) = doc["prom_port"].as_str() {
                    config.insert("prom_port".to_string(), serde_json::Value::String(port.to_string()));
                }
                if let Some(stats) = doc["print_stats"].as_bool() {
                    config.insert("print_stats".to_string(), serde_json::Value::Bool(stats));
                }
                if let Some(log) = doc["log_to_file"].as_bool() {
                    config.insert("log_to_file".to_string(), serde_json::Value::Bool(log));
                }
                if let Some(port) = doc["health_check_port"].as_str() {
                    config.insert("health_check_port".to_string(), serde_json::Value::String(port.to_string()));
                }
                if let Some(diff) = doc["min_share_diff"].as_i64() {
                    config.insert("min_share_diff".to_string(), serde_json::Value::Number(serde_json::Number::from(diff)));
                }
                if let Some(vd) = doc["var_diff"].as_bool() {
                    config.insert("var_diff".to_string(), serde_json::Value::Bool(vd));
                }
                if let Some(spm) = doc["shares_per_min"].as_i64() {
                    config.insert("shares_per_min".to_string(), serde_json::Value::Number(serde_json::Number::from(spm)));
                }
                if let Some(vds) = doc["var_diff_stats"].as_bool() {
                    config.insert("var_diff_stats".to_string(), serde_json::Value::Bool(vds));
                }
                if let Some(bwt) = doc["block_wait_time"].as_i64() {
                    config.insert("block_wait_time".to_string(), serde_json::Value::Number(serde_json::Number::from(bwt)));
                }
                if let Some(ens) = doc["extranonce_size"].as_i64() {
                    config.insert("extranonce_size".to_string(), serde_json::Value::Number(serde_json::Number::from(ens)));
                }
                if let Some(clamp) = doc["pow2_clamp"].as_bool() {
                    config.insert("pow2_clamp".to_string(), serde_json::Value::Bool(clamp));
                }

                return serde_json::to_string(&config).unwrap_or_else(|_| "{}".to_string());
            }
        }
    }
    "{}".to_string()
}

/// Update config from JSON
async fn update_config_from_json(json_body: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::fs;

    let config: serde_json::Value = serde_json::from_str(json_body)?;
    let config_path = "config.yaml";

    // Build YAML content directly from JSON values
    let mut out_str = String::new();
    out_str.push_str("# RustBridge Configuration\n");
    out_str.push_str("# This file configures the stratum bridge that connects miners to the Kaspa node\n\n");

    if let Some(port) = config.get("stratum_port").and_then(|v| v.as_str()) {
        out_str.push_str("# Stratum server port (format: \":PORT\" or \"HOST:PORT\")\n");
        out_str.push_str(&format!("stratum_port: {}\n\n", port));
    }
    if let Some(addr) = config.get("kaspad_address").and_then(|v| v.as_str()) {
        out_str.push_str("# Kaspa node gRPC address (format: \"HOST:PORT\" or \"grpc://HOST:PORT\")\n");
        out_str.push_str(&format!("kaspad_address: {}\n\n", addr));
    }
    if let Some(port) = config.get("prom_port").and_then(|v| v.as_str()) {
        out_str.push_str("# Prometheus metrics server port (format: \":PORT\" or \"HOST:PORT\")\n");
        out_str.push_str(&format!("prom_port: {}\n\n", port));
    }
    if let Some(stats) = config.get("print_stats").and_then(|v| v.as_bool()) {
        out_str.push_str("# Print statistics to console\n");
        out_str.push_str(&format!("print_stats: {}\n\n", stats));
    }
    if let Some(log) = config.get("log_to_file").and_then(|v| v.as_bool()) {
        out_str.push_str("# Log to file (if true, logs will be written to a file)\n");
        out_str.push_str(&format!("log_to_file: {}\n\n", log));
    }
    if let Some(port) = config.get("health_check_port").and_then(|v| v.as_str()) {
        out_str.push_str("# Health check server port (optional, leave empty to disable)\n");
        out_str.push_str(&format!("health_check_port: {}\n\n", port));
    }
    if let Some(diff) = config.get("min_share_diff").and_then(|v| v.as_u64()) {
        out_str.push_str("# Minimum share difficulty\n");
        out_str.push_str(&format!("min_share_diff: {}\n\n", diff));
    }
    if let Some(vd) = config.get("var_diff").and_then(|v| v.as_bool()) {
        out_str.push_str("# Enable variable difficulty adjustment\n");
        out_str.push_str(&format!("var_diff: {}\n\n", vd));
    }
    if let Some(spm) = config.get("shares_per_min").and_then(|v| v.as_u64()) {
        out_str.push_str("# Target shares per minute for variable difficulty\n");
        out_str.push_str(&format!("shares_per_min: {}\n\n", spm));
    }
    if let Some(vds) = config.get("var_diff_stats").and_then(|v| v.as_bool()) {
        out_str.push_str("# Enable variable difficulty statistics logging\n");
        out_str.push_str(&format!("var_diff_stats: {}\n\n", vds));
    }
    if let Some(bwt) = config.get("block_wait_time").and_then(|v| v.as_u64()) {
        out_str.push_str("# Block template wait time in seconds\n");
        out_str.push_str(&format!("block_wait_time: {}\n\n", bwt));
    }
    if let Some(ens) = config.get("extranonce_size").and_then(|v| v.as_u64()) {
        out_str.push_str("# Extranonce size in bytes (0-3)\n");
        out_str.push_str("# NOTE: Auto-detected per client based on miner type (Bitmain=0, IceRiver/BzMiner/Goldshell=2)\n");
        out_str.push_str(&format!("extranonce_size: {}\n\n", ens));
    }
    if let Some(clamp) = config.get("pow2_clamp").and_then(|v| v.as_bool()) {
        out_str.push_str("# Enable power-of-2 difficulty clamping\n");
        out_str.push_str(&format!("pow2_clamp: {}\n\n", clamp));
    }

    // Write to file
    fs::write(config_path, out_str)?;

    Ok(())
}

/// Start Prometheus metrics server
pub async fn start_prom_server(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    init_metrics();

    // Handle ":PORT" format by prepending "0.0.0.0"
    let addr_str = if port.starts_with(':') { format!("0.0.0.0{}", port) } else { port.to_string() };

    let addr: SocketAddr = addr_str.parse()?;
    let listener = TcpListener::bind(addr).await?;

    tracing::debug!("Hosting prom stats on {}/metrics", addr);

    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buffer = [0; 8192];

        if let Ok(n) = stream.read(&mut buffer).await {
            let request = String::from_utf8_lossy(&buffer[..n]);

            if request.starts_with("GET /metrics") {
                use prometheus::Encoder;
                let encoder = prometheus::TextEncoder::new();
                let metric_families = prometheus::gather();
                let mut buffer = Vec::new();
                encoder.encode(&metric_families, &mut buffer)?;

                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
                    buffer.len(),
                    String::from_utf8_lossy(&buffer)
                );

                stream.write_all(response.as_bytes()).await?;
            } else if request.starts_with("GET /api/stats") {
                // Return JSON stats
                let stats = get_stats_json().await;
                let json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                    json.len(),
                    json
                );
                stream.write_all(response.as_bytes()).await?;
            } else if request.starts_with("GET /api/config") {
                // Return current config as JSON
                let config_json = get_config_json().await;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                    config_json.len(),
                    config_json
                );
                stream.write_all(response.as_bytes()).await?;
            } else if request.starts_with("POST /api/config") {
                // Update config from JSON body
                let body_start = request.find("\r\n\r\n").unwrap_or(request.len());
                let body = &request[body_start + 4..];
                let result = update_config_from_json(body).await;
                let json_response = if result.is_ok() {
                    r#"{"success": true, "message": "Config updated successfully. Bridge restart required for changes to take effect."}"#
                } else {
                    r#"{"success": false, "message": "Failed to update config"}"#
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                    json_response.len(),
                    json_response
                );
                stream.write_all(response.as_bytes()).await?;
            } else {
                let response = "HTTP/1.1 404 Not Found\r\n\r\n";
                stream.write_all(response.as_bytes()).await?;
            }
        }
    }
}
