use prometheus::proto::MetricFamily;
#[cfg(feature = "rkstratum_cpu_miner")]
use prometheus::{Counter, register_counter};
use prometheus::{CounterVec, Gauge, GaugeVec, register_counter_vec, register_gauge, register_gauge_vec};
use serde::{Deserialize, Serialize};
#[cfg(feature = "rkstratum_cpu_miner")]
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::app_config::BridgeConfig;
use crate::net_utils::bind_addr_from_port;
use std::path::PathBuf;

/// Worker labels for Prometheus metrics
const WORKER_LABELS: &[&str] = &["instance", "worker", "miner", "wallet", "ip"];

/// Invalid share type labels
const INVALID_LABELS: &[&str] = &["instance", "worker", "miner", "wallet", "ip", "type"];

/// Block labels
const BLOCK_LABELS: &[&str] = &["instance", "worker", "miner", "wallet", "ip", "nonce", "bluescore", "timestamp", "hash"];

/// Error labels
const ERROR_LABELS: &[&str] = &["instance", "wallet", "error"];

/// Balance labels
const BALANCE_LABELS: &[&str] = &["instance", "wallet"];

/// Share counter - number of valid shares found by worker
static SHARE_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Share difficulty counter - total difficulty of shares found by worker
static SHARE_DIFF_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Invalid share counter - number of invalid/stale/duplicate/weak shares
static INVALID_COUNTER: OnceLock<CounterVec> = OnceLock::new();

/// Block counter - number of blocks mined
static BLOCK_COUNTER: OnceLock<CounterVec> = OnceLock::new();

static BLOCK_ACCEPTED_COUNTER: OnceLock<CounterVec> = OnceLock::new();

static BLOCK_NOT_CONFIRMED_BLUE_COUNTER: OnceLock<CounterVec> = OnceLock::new();

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

/// Worker current difficulty gauge (current mining difficulty assigned to worker)
static WORKER_CURRENT_DIFFICULTY: OnceLock<GaugeVec> = OnceLock::new();

/// Worker last activity time - tracks when each worker last submitted a share
/// Key: "instance:worker:wallet", Value: Instant of last activity
static WORKER_LAST_ACTIVITY: OnceLock<parking_lot::Mutex<HashMap<String, Instant>>> = OnceLock::new();

/// Bridge start time - tracks when the bridge started (for uptime calculation)
static BRIDGE_START_TIME: OnceLock<Instant> = OnceLock::new();

// ---------------------------
// Internal CPU miner metrics (feature-gated)
// ---------------------------
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_HASHES_TRIED_TOTAL: OnceLock<Counter> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_BLOCKS_SUBMITTED_TOTAL: OnceLock<Counter> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_BLOCKS_ACCEPTED_TOTAL: OnceLock<Counter> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_HASHRATE_GHS: OnceLock<Gauge> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_MINING_ADDRESS: OnceLock<String> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
static INTERNAL_CPU_RECENT_BLOCKS: OnceLock<parking_lot::Mutex<VecDeque<InternalCpuBlock>>> = OnceLock::new();
#[cfg(feature = "rkstratum_cpu_miner")]
const INTERNAL_CPU_RECENT_BLOCKS_LIMIT: usize = 256;

/// Initialize Prometheus metrics
pub fn init_metrics() {
    // Record bridge start time for uptime calculation
    BRIDGE_START_TIME.get_or_init(Instant::now);
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

    BLOCK_ACCEPTED_COUNTER.get_or_init(|| {
        register_counter_vec!(
            "ks_blocks_accepted_by_node",
            "Number of blocks accepted by the connected Kaspa node (may later be red)",
            WORKER_LABELS
        )
        .unwrap()
    });

    BLOCK_NOT_CONFIRMED_BLUE_COUNTER.get_or_init(|| {
        register_counter_vec!(
            "ks_blocks_not_confirmed_blue",
            "Number of node-accepted blocks that were not confirmed blue within the confirmation window",
            WORKER_LABELS
        )
        .unwrap()
    });

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

    WORKER_CURRENT_DIFFICULTY.get_or_init(|| {
        register_gauge_vec!("ks_worker_current_difficulty", "Current mining difficulty assigned to worker", WORKER_LABELS).unwrap()
    });

    // Internal CPU miner metrics (no labels; there is only one internal miner per process)
    #[cfg(feature = "rkstratum_cpu_miner")]
    {
        INTERNAL_CPU_HASHES_TRIED_TOTAL.get_or_init(|| {
            register_counter!("ks_internal_cpu_hashes_tried_total", "Total hashes tried by the internal CPU miner since process start")
                .unwrap()
        });
        INTERNAL_CPU_BLOCKS_SUBMITTED_TOTAL.get_or_init(|| {
            register_counter!(
                "ks_internal_cpu_blocks_submitted_total",
                "Total blocks submitted by the internal CPU miner since process start"
            )
            .unwrap()
        });
        INTERNAL_CPU_BLOCKS_ACCEPTED_TOTAL.get_or_init(|| {
            register_counter!(
                "ks_internal_cpu_blocks_accepted_total",
                "Total blocks accepted by the connected Kaspa node from the internal CPU miner since process start"
            )
            .unwrap()
        });
        INTERNAL_CPU_HASHRATE_GHS
            .get_or_init(|| register_gauge!("ks_internal_cpu_hashrate_ghs", "Internal CPU miner hashrate (GH/s)").unwrap());
    }
}

/// Update internal CPU miner metrics from a snapshot.
/// Values should be monotonically increasing counts; this function converts them to Prometheus counters.
#[cfg(feature = "rkstratum_cpu_miner")]
pub fn record_internal_cpu_miner_snapshot(hashes_tried: u64, blocks_submitted: u64, blocks_accepted: u64, hashrate_ghs: f64) {
    // Ensure metrics are registered even if the prom server hasn't started yet.
    init_metrics();

    if let Some(c) = INTERNAL_CPU_HASHES_TRIED_TOTAL.get() {
        let current = c.get() as u64;
        if hashes_tried > current {
            c.inc_by((hashes_tried - current) as f64);
        }
    }
    if let Some(c) = INTERNAL_CPU_BLOCKS_SUBMITTED_TOTAL.get() {
        let current = c.get() as u64;
        if blocks_submitted > current {
            c.inc_by((blocks_submitted - current) as f64);
        }
    }
    if let Some(c) = INTERNAL_CPU_BLOCKS_ACCEPTED_TOTAL.get() {
        let current = c.get() as u64;
        if blocks_accepted > current {
            c.inc_by((blocks_accepted - current) as f64);
        }
    }
    if let Some(g) = INTERNAL_CPU_HASHRATE_GHS.get() {
        let v = if hashrate_ghs.is_finite() && hashrate_ghs >= 0.0 { hashrate_ghs } else { 0.0 };
        g.set(v);
    }
}

/// Store the internal CPU miner reward address for display in `/api/stats`.
/// Best-effort: if called multiple times, only the first value is kept.
#[cfg(feature = "rkstratum_cpu_miner")]
pub fn set_internal_cpu_mining_address(addr: String) {
    let addr = addr.trim().to_string();
    if addr.is_empty() {
        return;
    }
    let _ = INTERNAL_CPU_MINING_ADDRESS.set(addr);
}

#[cfg(feature = "rkstratum_cpu_miner")]
#[derive(Clone, Debug)]
struct InternalCpuBlock {
    timestamp_unix: u64,
    bluescore: u64,
    nonce: u64,
    hash: String,
}

/// Record a recently submitted internal CPU miner block so the dashboard can display it
/// without relying on high-cardinality Prometheus labels.
#[cfg(feature = "rkstratum_cpu_miner")]
pub fn record_internal_cpu_recent_block(hash: String, nonce: u64, bluescore: u64) {
    use std::time::{SystemTime, UNIX_EPOCH};

    if hash.trim().is_empty() {
        return;
    }

    let ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

    let mut q = INTERNAL_CPU_RECENT_BLOCKS
        .get_or_init(|| parking_lot::Mutex::new(VecDeque::with_capacity(INTERNAL_CPU_RECENT_BLOCKS_LIMIT)))
        .lock();

    // De-dupe by hash
    if q.iter().any(|b| b.hash == hash) {
        return;
    }

    q.push_front(InternalCpuBlock { timestamp_unix: ts, bluescore, nonce, hash });
    if q.len() > INTERNAL_CPU_RECENT_BLOCKS_LIMIT {
        q.truncate(INTERNAL_CPU_RECENT_BLOCKS_LIMIT);
    }
}

#[derive(Clone, Debug)]
enum HttpMode {
    Aggregated { web_bind: String },
    Instance { instance_id: String, web_bind: String },
}

fn content_type_for_path(path: &str) -> &'static str {
    let p = path.to_ascii_lowercase();
    if p.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if p.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if p.ends_with(".js") {
        "application/javascript; charset=utf-8"
    } else if p.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn try_read_static_file(url_path: &str) -> Option<(String, Vec<u8>)> {
    // Files are vendored under bridge/static.
    // URL layout expected by the dashboard:
    // - / -> index.html
    // - /raw.html
    // - /static/... -> maps to bridge/static/... (strip leading /static/)
    let rel = match url_path {
        "/" => "index.html".to_string(),
        "/index.html" => "index.html".to_string(),
        "/raw.html" => "raw.html".to_string(),
        p if p.starts_with("/static/") => p.trim_start_matches("/static/").to_string(),
        _ => return None,
    };

    // Prevent path traversal
    if rel.contains("..") || rel.contains('\\') {
        return None;
    }

    // Prefer embedded assets for production/portable binaries.
    // Fall back to reading from disk to keep local development simple.
    static STATIC_DIR: include_dir::Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/static");

    if let Some(f) = STATIC_DIR.get_file(&rel) {
        return Some((rel, f.contents().to_vec()));
    }

    let file_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("static").join(&rel);
    let bytes = std::fs::read(&file_path).ok()?;
    Some((rel, bytes))
}

async fn write_response(
    mut stream: tokio::net::TcpStream,
    response: String,
    body_bytes: Option<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;
    stream.write_all(response.as_bytes()).await?;
    if let Some(body) = body_bytes {
        stream.write_all(&body).await?;
    }
    Ok(())
}

async fn handle_http_request(
    mut stream: tokio::net::TcpStream,
    request: &str,
    mode: &HttpMode,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncWriteExt;

    let path = request.lines().next().and_then(|line| line.split_whitespace().nth(1)).unwrap_or("/");

    if request.starts_with("GET /metrics") {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let metric_families = match mode {
            HttpMode::Aggregated { .. } => prometheus::gather(),
            HttpMode::Instance { instance_id, .. } => filter_metric_families_for_instance(prometheus::gather(), instance_id),
        };
        let mut buf = Vec::new();
        encoder.encode(&metric_families, &mut buf)?;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
            buf.len(),
            String::from_utf8_lossy(&buf)
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    if request.starts_with("GET /api/status") {
        let kaspad_version = crate::kaspaapi::NODE_STATUS.lock().server_version.clone().unwrap_or_else(|| "-".to_string());
        let status_cfg = get_web_status_config();
        let web_bind = match mode {
            HttpMode::Aggregated { web_bind } => web_bind.clone(),
            HttpMode::Instance { web_bind, .. } => web_bind.clone(),
        };

        let status =
            WebStatusResponse { kaspad_address: status_cfg.kaspad_address, kaspad_version, instances: status_cfg.instances, web_bind };
        let json = serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
            json.len(),
            json
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    if request.starts_with("GET /api/stats") {
        let stats = match mode {
            HttpMode::Aggregated { .. } => get_stats_json_all().await,
            HttpMode::Instance { instance_id, .. } => get_stats_json(instance_id).await,
        };
        let json = serde_json::to_string(&stats).unwrap_or_else(|_| "{}".to_string());
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
            json.len(),
            json
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    if matches!(mode, HttpMode::Instance { .. }) && request.starts_with("GET /api/config") {
        let config_json = get_config_json().await;
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
            config_json.len(),
            config_json
        );
        stream.write_all(response.as_bytes()).await?;
        return Ok(());
    }

    if matches!(mode, HttpMode::Instance { .. }) && request.starts_with("POST /api/config") {
        if !config_write_allowed() {
            let json_response =
                r#"{"success": false, "message": "Config write disabled. Set RKSTRATUM_ALLOW_CONFIG_WRITE=1 to enable."}"#;
            let response = format!(
                "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}",
                json_response.len(),
                json_response
            );
            stream.write_all(response.as_bytes()).await?;
            return Ok(());
        }

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
        return Ok(());
    }

    if request.starts_with("GET /") {
        if let Some((rel, bytes)) = try_read_static_file(path) {
            let ct = content_type_for_path(&rel);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n\r\n", ct, bytes.len());
            write_response(stream, response, Some(bytes)).await?;
        } else {
            stream.write_all("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes()).await?;
        }
        return Ok(());
    }

    stream.write_all("HTTP/1.1 404 Not Found\r\n\r\n".as_bytes()).await?;
    Ok(())
}

async fn serve_http_loop(listener: tokio::net::TcpListener, mode: HttpMode) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::io::AsyncReadExt;

    loop {
        let (mut stream, _) = listener.accept().await?;
        let mut buffer = [0; 8192];

        if let Ok(n) = stream.read(&mut buffer).await {
            let request = String::from_utf8_lossy(&buffer[..n]);
            let _ = handle_http_request(stream, &request, &mode).await;
        }
    }
}

pub async fn start_web_server_all(port: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    init_metrics();

    let addr_str = bind_addr_from_port(port);
    let addr: SocketAddr = addr_str.parse()?;
    let listener = TcpListener::bind(addr).await?;
    let web_bind_for_status = addr_str.clone();

    tracing::debug!("Hosting aggregated web stats on {}/", addr);
    serve_http_loop(listener, HttpMode::Aggregated { web_bind: web_bind_for_status }).await
}

/// Worker context for metrics
pub struct WorkerContext {
    pub instance_id: String,
    pub worker_name: String,
    pub miner: String,
    pub wallet: String,
    pub ip: String,
}

impl WorkerContext {
    pub fn labels(&self) -> Vec<&str> {
        vec![&self.instance_id, &self.worker_name, &self.miner, &self.wallet, &self.ip]
    }
}

pub fn record_block_accepted_by_node(worker: &WorkerContext) {
    if let Some(counter) = BLOCK_ACCEPTED_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }
}

pub fn record_block_not_confirmed_blue(worker: &WorkerContext) {
    if let Some(counter) = BLOCK_NOT_CONFIRMED_BLUE_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
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
    // Update last activity time for this worker
    update_worker_activity(worker);
}

/// Record a stale share
pub fn record_stale_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("stale");
        counter.with_label_values(&labels).inc();
    }
    // Update activity time - worker is still connected even if share is stale
    update_worker_activity(worker);
}

/// Record a duplicate share
pub fn record_dupe_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("duplicate");
        counter.with_label_values(&labels).inc();
    }
    // Update activity time - worker is still connected even if share is duplicate
    update_worker_activity(worker);
}

/// Record an invalid share
pub fn record_invalid_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("invalid");
        counter.with_label_values(&labels).inc();
    }
    // Update activity time - worker is still connected even if share is invalid
    update_worker_activity(worker);
}

/// Record a weak share
pub fn record_weak_share(worker: &WorkerContext) {
    if let Some(counter) = INVALID_COUNTER.get() {
        let mut labels = worker.labels();
        labels.push("weak");
        counter.with_label_values(&labels).inc();
    }
    // Update activity time - worker is still connected even if share is weak
    update_worker_activity(worker);
}

/// Helper function to update worker activity time
fn update_worker_activity(worker: &WorkerContext) {
    let key = format!("{}:{}:{}", worker.instance_id, worker.worker_name, worker.wallet);
    let activity_map = WORKER_LAST_ACTIVITY.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
    activity_map.lock().insert(key, Instant::now());
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
        let timestamp_str =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs().to_string();
        labels.push(&nonce_str);
        labels.push(&bluescore_str);
        labels.push(&timestamp_str);
        labels.push(&hash);
        gauge.with_label_values(&labels).set(1.0);
    }
}

/// Record a disconnect
pub fn record_disconnect(worker: &WorkerContext) {
    if let Some(counter) = DISCONNECT_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc();
    }

    // Remove worker from activity tracking immediately on disconnect
    let key = format!("{}:{}:{}", worker.instance_id, worker.worker_name, worker.wallet);
    let activity_map = WORKER_LAST_ACTIVITY.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
    activity_map.lock().remove(&key);
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
    if let Some(gauge) = NETWORK_BLOCK_COUNT.get() {
        gauge.set(block_count as f64);
    }
    if let Some(gauge) = NETWORK_DIFFICULTY.get() {
        gauge.set(difficulty);
    }
}

#[derive(Serialize)]
struct WebStatusResponse {
    kaspad_address: String,
    kaspad_version: String,
    instances: usize,
    web_bind: String,
}

#[derive(Clone, Debug)]
struct WebStatusConfig {
    kaspad_address: String,
    instances: usize,
}

static WEB_STATUS_CONFIG: OnceLock<parking_lot::RwLock<WebStatusConfig>> = OnceLock::new();
static WEB_CONFIG_PATH: OnceLock<PathBuf> = OnceLock::new();
static WEB_CONFIG_WRITE_LOCK: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();

/// Set which config file `/api/config` reads/writes.
/// If not set, it falls back to `config.yaml` in the current working directory.
pub fn set_web_config_path(path: PathBuf) {
    let _ = WEB_CONFIG_PATH.set(path);
}

fn get_web_config_path() -> PathBuf {
    WEB_CONFIG_PATH.get().cloned().unwrap_or_else(|| PathBuf::from("config.yaml"))
}

fn config_write_allowed() -> bool {
    matches!(
        std::env::var("RKSTRATUM_ALLOW_CONFIG_WRITE").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE") | Ok("yes") | Ok("YES") | Ok("on") | Ok("ON")
    )
}

/// Set best-effort status fields used by `/api/status`.
///
/// This avoids re-reading `config.yaml` from within the web/prom servers (which can be wrong when
/// using `--config` / CLI overrides, and also breaks when the working directory differs).
pub fn set_web_status_config(kaspad_address: String, instances: usize) {
    let lock =
        WEB_STATUS_CONFIG.get_or_init(|| parking_lot::RwLock::new(WebStatusConfig { kaspad_address: "-".to_string(), instances: 1 }));
    *lock.write() = WebStatusConfig { kaspad_address, instances: instances.max(1) };
}

fn get_web_status_config() -> WebStatusConfig {
    WEB_STATUS_CONFIG
        .get_or_init(|| parking_lot::RwLock::new(WebStatusConfig { kaspad_address: "-".to_string(), instances: 1 }))
        .read()
        .clone()
}

/// Record a worker error
pub fn record_worker_error(instance_id: &str, wallet: &str, error: &str) {
    if let Some(counter) = ERROR_BY_WALLET.get() {
        counter.with_label_values(&[instance_id, wallet, error]).inc();
    }
}

/// Record wallet balances
pub fn record_balances(instance_id: &str, balances: &[(String, u64)]) {
    if let Some(gauge) = BALANCE_GAUGE.get() {
        for (address, balance) in balances {
            // Convert from sompi to KAS (divide by 100000000)
            let balance_kas = *balance as f64 / 100_000_000.0;
            gauge.with_label_values(&[instance_id, address]).set(balance_kas);
        }
    }
}

fn metric_matches_instance(metric: &prometheus::proto::Metric, instance_id: &str) -> bool {
    metric.get_label().iter().any(|label| label.get_name() == "instance" && label.get_value() == instance_id)
}

fn filter_metric_families_for_instance(metric_families: Vec<MetricFamily>, instance_id: &str) -> Vec<MetricFamily> {
    let mut out = Vec::with_capacity(metric_families.len());

    for family in metric_families {
        let has_instance_label =
            family.get_metric().iter().any(|metric| metric.get_label().iter().any(|label| label.get_name() == "instance"));

        if !has_instance_label {
            out.push(family);
            continue;
        }

        let mut filtered_family = family.clone();
        filtered_family.mut_metric().retain(|metric| metric_matches_instance(metric, instance_id));
        if !filtered_family.get_metric().is_empty() {
            out.push(filtered_family);
        }
    }

    out
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
    if let Some(counter) = BLOCK_ACCEPTED_COUNTER.get() {
        counter.with_label_values(&worker.labels()).inc_by(0.0);
    }
    if let Some(counter) = BLOCK_NOT_CONFIRMED_BLUE_COUNTER.get() {
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
    // Initialize worker difficulty to 0 (will be updated when difficulty is set)
    if let Some(gauge) = WORKER_CURRENT_DIFFICULTY.get() {
        gauge.with_label_values(&worker.labels()).set(0.0);
    }
}

/// Update the current mining difficulty for a worker
pub fn update_worker_difficulty(worker: &WorkerContext, difficulty: f64) {
    if let Some(gauge) = WORKER_CURRENT_DIFFICULTY.get() {
        gauge.with_label_values(&worker.labels()).set(difficulty);
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct InternalCpuStats {
    hashrateGhs: f64,
    hashesTried: u64,
    blocksSubmitted: u64,
    blocksAccepted: u64,
    shares: u64,
    stale: u64,
    invalid: u64,
    wallet: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(non_snake_case)]
struct StatsResponse {
    totalBlocks: u64,
    totalShares: u64,
    networkHashrate: u64,
    networkDifficulty: f64,
    networkBlockCount: u64,
    activeWorkers: usize,
    internalCpu: Option<InternalCpuStats>,
    blocks: Vec<BlockInfo>,
    workers: Vec<WorkerInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bridgeUptime: Option<u64>, // Bridge uptime in seconds
}

#[derive(Debug, Serialize, Deserialize)]
struct BlockInfo {
    instance: String,
    worker: String,
    wallet: String,
    timestamp: String,
    hash: String,
    nonce: String,
    bluescore: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkerInfo {
    instance: String,
    worker: String,
    wallet: String,
    hashrate: f64,
    shares: u64,
    stale: u64,
    invalid: u64,
    blocks: u64,
    #[serde(skip_serializing_if = "Option::is_none", rename = "lastSeen")]
    last_seen: Option<u64>, // Unix timestamp in seconds
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<String>, // "online", "offline", or "idle"
    #[serde(skip_serializing_if = "Option::is_none", rename = "currentDifficulty")]
    current_difficulty: Option<f64>, // Current mining difficulty assigned to this worker
    #[serde(skip_serializing_if = "Option::is_none", rename = "sessionUptime")]
    session_uptime: Option<u64>, // Session uptime in seconds (time since last connection)
}

fn parse_worker_labels(labels: &[prometheus::proto::LabelPair]) -> (String, String, String) {
    let mut instance = String::new();
    let mut worker = String::new();
    let mut wallet = String::new();

    for label in labels {
        match label.get_name() {
            "instance" => instance = label.get_value().to_string(),
            "worker" => worker = label.get_value().to_string(),
            "wallet" => wallet = label.get_value().to_string(),
            _ => {}
        }
    }

    (instance, worker, wallet)
}

fn new_worker_info(instance: String, worker: String, wallet: String) -> WorkerInfo {
    WorkerInfo {
        instance,
        worker,
        wallet,
        hashrate: 0.0,
        shares: 0,
        stale: 0,
        invalid: 0,
        blocks: 0,
        last_seen: None,
        status: None,
        current_difficulty: None,
        session_uptime: None,
    }
}

/// Get stats as JSON (optionally filtered to a single instance id)
async fn get_stats_json_filtered(instance_id: Option<&str>) -> StatsResponse {
    use prometheus::gather;

    // NOTE: Instance filtering removes metrics that don't carry an `instance` label.
    // The dashboard expects global network gauges too, so we always gather unfiltered
    // for those, and then optionally filter for per-worker/per-block metrics.
    let all_families = gather();
    let families_for_workers_and_blocks = match instance_id {
        Some(id) => filter_metric_families_for_instance(all_families.clone(), id),
        None => all_families.clone(),
    };
    let mut stats = StatsResponse {
        totalBlocks: 0,
        totalShares: 0,
        networkHashrate: 0,
        networkDifficulty: 0.0,
        networkBlockCount: 0,
        activeWorkers: 0,
        internalCpu: None,
        blocks: Vec::new(),
        workers: Vec::new(),
        bridgeUptime: None,
    };

    let mut worker_stats: HashMap<String, WorkerInfo> = HashMap::new();
    let mut worker_hash_values: HashMap<String, f64> = HashMap::new(); // Store hash values for hashrate calculation
    let mut worker_start_times: HashMap<String, f64> = HashMap::new(); // Store start times for hashrate calculation
    let mut worker_difficulties: HashMap<String, f64> = HashMap::new(); // Store current difficulty for each worker
    let mut block_set: HashSet<String> = HashSet::new();

    // Parse global network gauges from the unfiltered set.
    // Also pick up internal CPU miner metrics (if present).
    let mut internal_cpu_hashrate_ghs: Option<f64> = None;
    let mut internal_cpu_hashes_tried: Option<u64> = None;
    let mut internal_cpu_blocks_submitted: Option<u64> = None;
    let mut internal_cpu_blocks_accepted: Option<u64> = None;

    for family in all_families.iter() {
        let name = family.get_name();

        if name == "ks_estimated_network_hashrate_gauge" {
            if let Some(metric) = family.get_metric().first() {
                stats.networkHashrate = metric.get_gauge().get_value() as u64;
            }
        }

        if name == "ks_network_difficulty_gauge" {
            if let Some(metric) = family.get_metric().first() {
                stats.networkDifficulty = metric.get_gauge().get_value();
            }
        }

        // Network height / block count gauge. Historical name is "ks_network_block_count".
        // Accept both just in case we rename later.
        if name == "ks_network_block_count" || name == "ks_network_block_count_gauge" {
            if let Some(metric) = family.get_metric().first() {
                stats.networkBlockCount = metric.get_gauge().get_value() as u64;
            }
        }

        // Internal CPU miner metrics (exported when the bridge is built with `rkstratum_cpu_miner`
        // and the internal miner is enabled at runtime).
        if name == "ks_internal_cpu_hashrate_ghs" {
            if let Some(metric) = family.get_metric().first() {
                internal_cpu_hashrate_ghs = Some(metric.get_gauge().get_value());
            }
        }
        if name == "ks_internal_cpu_hashes_tried_total" {
            if let Some(metric) = family.get_metric().first() {
                internal_cpu_hashes_tried = Some(metric.get_counter().get_value().max(0.0) as u64);
            }
        }
        if name == "ks_internal_cpu_blocks_submitted_total" {
            if let Some(metric) = family.get_metric().first() {
                internal_cpu_blocks_submitted = Some(metric.get_counter().get_value().max(0.0) as u64);
            }
        }
        if name == "ks_internal_cpu_blocks_accepted_total" {
            if let Some(metric) = family.get_metric().first() {
                internal_cpu_blocks_accepted = Some(metric.get_counter().get_value().max(0.0) as u64);
            }
        }
    }

    {
        let blocks_submitted = internal_cpu_blocks_submitted.unwrap_or(0);
        let blocks_accepted = internal_cpu_blocks_accepted.unwrap_or(0);
        let hashes_tried = internal_cpu_hashes_tried.unwrap_or(0);
        let hashrate_ghs = internal_cpu_hashrate_ghs.unwrap_or(0.0);

        // Only surface internal CPU miner data when it is actually enabled/active.
        // Otherwise a build that includes the feature would always show an "InternalCPU" row with zeros.
        #[cfg(feature = "rkstratum_cpu_miner")]
        let wallet = INTERNAL_CPU_MINING_ADDRESS.get().cloned().unwrap_or_default();
        #[cfg(not(feature = "rkstratum_cpu_miner"))]
        let wallet = String::new();

        let should_show_internal_cpu =
            !wallet.is_empty() || blocks_submitted > 0 || blocks_accepted > 0 || hashes_tried > 0 || hashrate_ghs > 0.0;

        if should_show_internal_cpu {
            let stale = blocks_submitted.saturating_sub(blocks_accepted);
            stats.internalCpu = Some(InternalCpuStats {
                hashrateGhs: hashrate_ghs,
                hashesTried: hashes_tried,
                blocksSubmitted: blocks_submitted,
                blocksAccepted: blocks_accepted,
                // Expose these so the UI can fill Shares/Stale/Invalid columns for InternalCPU.
                // Internal CPU mining doesn't produce Stratum shares; blocks are the closest analogue.
                shares: blocks_accepted,
                stale,
                invalid: 0,
                wallet,
            });
        }
    }

    for family in families_for_workers_and_blocks {
        let name = family.get_name();

        // Parse block gauge
        if name == "ks_mined_blocks_gauge" {
            for metric in family.get_metric() {
                if metric.get_gauge().get_value() > 0.0 {
                    let labels = metric.get_label();
                    let mut instance = String::new();
                    let mut worker = String::new();
                    let mut wallet = String::new();
                    let mut timestamp = String::new();
                    let mut hash = String::new();
                    let mut nonce = String::new();
                    let mut bluescore = String::new();

                    for label in labels {
                        match label.get_name() {
                            "instance" => instance = label.get_value().to_string(),
                            "worker" => worker = label.get_value().to_string(),
                            "wallet" => wallet = label.get_value().to_string(),
                            "timestamp" => timestamp = label.get_value().to_string(),
                            "hash" => hash = label.get_value().to_string(),
                            "nonce" => nonce = label.get_value().to_string(),
                            "bluescore" => bluescore = label.get_value().to_string(),
                            _ => {}
                        }
                    }

                    if !hash.is_empty() && !block_set.contains(&hash) {
                        block_set.insert(hash.clone());
                        stats.blocks.push(BlockInfo {
                            instance,
                            worker: worker.clone(),
                            wallet: wallet.clone(),
                            timestamp,
                            hash,
                            nonce,
                            bluescore,
                        });
                        stats.totalBlocks += 1;
                    }
                }
            }
        }

        // Parse block counter
        if name == "ks_blocks_mined" {
            for metric in family.get_metric() {
                let (instance, worker_key, wallet) = parse_worker_labels(metric.get_label());

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let entry = worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));
                    // Aggregate across multiple time series for the same (instance,worker,wallet)
                    entry.blocks = entry.blocks.saturating_add(count);
                }
            }
        }

        // Parse share diff counter (for hashrate calculation)
        if name == "ks_valid_share_diff_counter" {
            for metric in family.get_metric() {
                let (instance, worker_key, wallet) = parse_worker_labels(metric.get_label());

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let total_hash_value = metric.get_counter().get_value();
                    // Store hash value for hashrate calculation (aggregate across label variants)
                    *worker_hash_values.entry(key.clone()).or_insert(0.0) += total_hash_value;
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));
                }
            }
        }

        // Parse share counter
        if name == "ks_valid_share_counter" {
            for metric in family.get_metric() {
                let (instance, worker_key, wallet) = parse_worker_labels(metric.get_label());

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let entry = worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));
                    entry.shares = entry.shares.saturating_add(count);
                    stats.totalShares = stats.totalShares.saturating_add(count);
                }
            }
        }

        // Parse invalid share counter
        if name == "ks_invalid_share_counter" {
            for metric in family.get_metric() {
                let mut share_type = String::new();

                let labels = metric.get_label();
                let (instance, worker_key, wallet) = parse_worker_labels(labels);
                for label in labels {
                    if label.get_name() == "type" {
                        share_type = label.get_value().to_string();
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let worker = worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));

                    if share_type == "stale" {
                        worker.stale = worker.stale.saturating_add(count);
                    } else if share_type == "invalid" {
                        worker.invalid = worker.invalid.saturating_add(count);
                    }
                }
            }
        }

        // Parse worker start time
        if name == "ks_worker_start_time" {
            for metric in family.get_metric() {
                let (instance, worker_key, wallet) = parse_worker_labels(metric.get_label());

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let start_time_secs = metric.get_gauge().get_value();
                    // Use earliest start time across multiple label variants
                    worker_start_times
                        .entry(key.clone())
                        .and_modify(|v| {
                            if start_time_secs > 0.0 && (*v <= 0.0 || start_time_secs < *v) {
                                *v = start_time_secs;
                            }
                        })
                        .or_insert(start_time_secs);
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));
                }
            }
        }

        // Parse worker current difficulty
        if name == "ks_worker_current_difficulty" {
            for metric in family.get_metric() {
                let (instance, worker_key, wallet) = parse_worker_labels(metric.get_label());

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let difficulty = metric.get_gauge().get_value();
                    // Use the most recent difficulty value (if multiple label variants exist)
                    if difficulty > 0.0 {
                        worker_difficulties.insert(key.clone(), difficulty);
                    }
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| new_worker_info(instance, worker_key, wallet));
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

    // Filter out inactive workers (no activity in the last 5 minutes)
    const WORKER_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes
    const WORKER_IDLE_THRESHOLD: Duration = Duration::from_secs(60); // 1 minute for idle status
    let now = Instant::now();
    let activity_map = WORKER_LAST_ACTIVITY.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
    let current_time_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

    // Clean up old entries and filter active workers
    let mut active_workers: Vec<WorkerInfo> = Vec::new();
    {
        let mut activity = activity_map.lock();
        for (key, mut worker) in worker_stats.into_iter() {
            // Populate difficulty and session uptime from collected metrics
            if let Some(&difficulty) = worker_difficulties.get(&key) {
                if difficulty > 0.0 {
                    worker.current_difficulty = Some(difficulty);
                }
            }

            // Calculate session uptime from start time
            if let Some(&start_time_secs) = worker_start_times.get(&key) {
                if start_time_secs > 0.0 {
                    let start_time_u64 = start_time_secs as u64;
                    let session_uptime_secs = current_time_secs.saturating_sub(start_time_u64);
                    worker.session_uptime = Some(session_uptime_secs);
                }
            }

            // Check if worker has been active recently
            if let Some(&last_activity) = activity.get(&key) {
                // Check if duration is valid (handles clock adjustments)
                if let Some(duration) = now.checked_duration_since(last_activity) {
                    if duration <= WORKER_INACTIVITY_TIMEOUT {
                        // Calculate last seen timestamp
                        let last_seen_secs = current_time_secs.saturating_sub(duration.as_secs());
                        worker.last_seen = Some(last_seen_secs);

                        // Determine status based on last activity
                        if duration <= WORKER_IDLE_THRESHOLD {
                            worker.status = Some("online".to_string());
                        } else {
                            worker.status = Some("idle".to_string());
                        }

                        active_workers.push(worker);
                    } else {
                        // Remove stale entries (no activity for > 5 minutes)
                        activity.remove(&key);
                    }
                } else {
                    // Clock went backwards or instant is in the future - treat as active
                    // Update to current time to prevent issues
                    activity.insert(key.clone(), now);
                    worker.last_seen = Some(current_time_secs);
                    worker.status = Some("online".to_string());
                    active_workers.push(worker);
                }
            } else {
                // No activity record exists - this means the worker hasn't submitted any shares
                // since the last stats collection. If they have shares, they might be disconnected.
                // Only include them if they have very recent activity (check worker start time)
                if let Some(&start_time_secs) = worker_start_times.get(&key) {
                    let start_time_secs_u64 = start_time_secs as u64;
                    let elapsed_secs = current_time_secs.saturating_sub(start_time_secs_u64);
                    // If worker started less than 1 minute ago and has shares, they might be active
                    // Otherwise, assume they're disconnected
                    if elapsed_secs < 60 && worker.shares > 0 {
                        // Very new worker - give them a chance
                        activity.insert(key.clone(), now);
                        worker.last_seen = Some(current_time_secs);
                        worker.status = Some("online".to_string());
                        active_workers.push(worker);
                    }
                    // Otherwise, don't include them (they're likely disconnected)
                }
            }
        }
    }

    stats.workers = active_workers;
    // Active workers are the number of Stratum workers, plus the internal CPU miner if present.
    stats.activeWorkers = stats.workers.len() + stats.internalCpu.as_ref().map(|_| 1).unwrap_or(0);

    // Fold internal CPU miner counts into summary totals so the dashboard top-cards reflect
    // internal mining even when no ASICs are connected.
    if let Some(icpu) = stats.internalCpu.as_ref() {
        stats.totalBlocks = stats.totalBlocks.saturating_add(icpu.blocksAccepted);
        // Internal CPU mining doesn't produce shares in the Stratum sense; however, blocks are
        // "shares too" operationally (they represent successful work). Counting accepted blocks
        // here prevents the Total Shares card from staying at 0 for CPU-only runs.
        stats.totalShares = stats.totalShares.saturating_add(icpu.blocksAccepted);
    }

    // Add internal CPU recent blocks into the unified blocks list so the donut chart and
    // recent blocks table populate even in CPU-only runs.
    #[cfg(feature = "rkstratum_cpu_miner")]
    if let Some(icpu) = stats.internalCpu.as_ref() {
        if let Some(q) = INTERNAL_CPU_RECENT_BLOCKS.get() {
            let wallet = icpu.wallet.clone();
            let guard = q.lock();
            for b in guard.iter() {
                let hash = b.hash.clone();
                if hash.is_empty() || block_set.contains(&hash) {
                    continue;
                }
                block_set.insert(hash.clone());
                stats.blocks.push(BlockInfo {
                    instance: "-".to_string(),
                    worker: "InternalCPU".to_string(),
                    wallet: wallet.clone(),
                    timestamp: b.timestamp_unix.to_string(),
                    hash,
                    nonce: b.nonce.to_string(),
                    bluescore: b.bluescore.to_string(),
                });
            }
        }
    }

    // Sort blocks by bluescore (newest first)
    stats.blocks.sort_by(|a, b| {
        let a_score: u64 = a.bluescore.parse().unwrap_or(0);
        let b_score: u64 = b.bluescore.parse().unwrap_or(0);
        b_score.cmp(&a_score)
    });

    // Sort workers by blocks (most blocks first)
    stats.workers.sort_by(|a, b| b.blocks.cmp(&a.blocks));

    // Calculate bridge uptime
    if let Some(&start_time) = BRIDGE_START_TIME.get() {
        let uptime_secs = now.duration_since(start_time).as_secs();
        stats.bridgeUptime = Some(uptime_secs);
    }

    stats
}

async fn get_stats_json(instance_id: &str) -> StatsResponse {
    get_stats_json_filtered(Some(instance_id)).await
}

async fn get_stats_json_all() -> StatsResponse {
    get_stats_json_filtered(None).await
}

/// Get current config as JSON
async fn get_config_json() -> String {
    use std::fs;

    let config_path = get_web_config_path();
    if let Ok(content) = fs::read_to_string(&config_path) {
        if let Ok(config) = BridgeConfig::from_yaml(&content) {
            // Convert BridgeConfig to JSON for web UI
            // For backward compatibility with single-instance mode UI, show first instance fields
            let first_instance = config.instances.first();

            let json_value = serde_json::json!({
                // Global fields
                "kaspad_address": config.global.kaspad_address,
                "block_wait_time": config.global.block_wait_time.as_millis() as u64,
                "print_stats": config.global.print_stats,
                "log_to_file": config.global.log_to_file,
                "health_check_port": config.global.health_check_port,
                "web_dashboard_port": config.global.web_dashboard_port,
                "var_diff": config.global.var_diff,
                "shares_per_min": config.global.shares_per_min,
                "var_diff_stats": config.global.var_diff_stats,
                "extranonce_size": config.global.extranonce_size,
                "pow2_clamp": config.global.pow2_clamp,
                "coinbase_tag_suffix": config.global.coinbase_tag_suffix,
                // Instance fields (from first instance for backward compatibility)
                "stratum_port": first_instance.map(|i| &i.stratum_port),
                "min_share_diff": first_instance.map(|i| i.min_share_diff),
                "prom_port": first_instance.and_then(|i| i.prom_port.as_ref()),
            });

            return serde_json::to_string(&json_value).unwrap_or_else(|_| "{}".to_string());
        }
    }
    "{}".to_string()
}

/// Update config from JSON
async fn update_config_from_json(json_body: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::fs;
    use std::time::Duration;

    let updates: serde_json::Value = serde_json::from_str(json_body)?;
    let config_path = get_web_config_path();
    let _guard = WEB_CONFIG_WRITE_LOCK.get_or_init(|| parking_lot::Mutex::new(())).lock();

    // Read existing config
    let content = fs::read_to_string(&config_path).unwrap_or_else(|_| String::new());
    let mut config = if content.is_empty() {
        BridgeConfig::default()
    } else {
        BridgeConfig::from_yaml(&content).unwrap_or_else(|_| BridgeConfig::default())
    };

    // Update global fields if provided
    if let Some(addr) = updates.get("kaspad_address").and_then(|v| v.as_str()) {
        config.global.kaspad_address = addr.to_string();
    }
    if let Some(bwt) = updates.get("block_wait_time").and_then(|v| v.as_u64()) {
        config.global.block_wait_time = Duration::from_millis(bwt);
    }
    if let Some(stats) = updates.get("print_stats").and_then(|v| v.as_bool()) {
        config.global.print_stats = stats;
    }
    if let Some(log) = updates.get("log_to_file").and_then(|v| v.as_bool()) {
        config.global.log_to_file = log;
    }
    if let Some(port) = updates.get("health_check_port").and_then(|v| v.as_str()) {
        config.global.health_check_port = port.to_string();
    }
    if let Some(port) = updates.get("web_dashboard_port").and_then(|v| v.as_str()) {
        config.global.web_dashboard_port = crate::net_utils::normalize_port(port);
    }
    if let Some(vd) = updates.get("var_diff").and_then(|v| v.as_bool()) {
        config.global.var_diff = vd;
    }
    if let Some(spm) = updates.get("shares_per_min").and_then(|v| v.as_u64()) {
        config.global.shares_per_min = spm as u32;
    }
    if let Some(vds) = updates.get("var_diff_stats").and_then(|v| v.as_bool()) {
        config.global.var_diff_stats = vds;
    }
    if let Some(ens) = updates.get("extranonce_size").and_then(|v| v.as_u64()) {
        config.global.extranonce_size = ens as u8;
    }
    if let Some(clamp) = updates.get("pow2_clamp").and_then(|v| v.as_bool()) {
        config.global.pow2_clamp = clamp;
    }
    if let Some(suffix) = updates.get("coinbase_tag_suffix") {
        if suffix.is_null() {
            config.global.coinbase_tag_suffix = None;
        } else if let Some(s) = suffix.as_str() {
            let trimmed = s.trim();
            config.global.coinbase_tag_suffix = if trimmed.is_empty() { None } else { Some(trimmed.to_string()) };
        }
    }

    // Update first instance fields if provided (for single-instance mode compatibility)
    if config.instances.is_empty() {
        config.instances.push(Default::default());
    }
    let instance = &mut config.instances[0];

    if let Some(port) = updates.get("stratum_port").and_then(|v| v.as_str()) {
        instance.stratum_port = crate::net_utils::normalize_port(port);
    }
    if let Some(diff) = updates.get("min_share_diff").and_then(|v| v.as_u64()) {
        instance.min_share_diff = diff as u32;
    }
    if let Some(port) = updates.get("prom_port").and_then(|v| v.as_str()) {
        let normalized = crate::net_utils::normalize_port(port);
        if normalized.is_empty() {
            instance.prom_port = None;
        } else {
            instance.prom_port = Some(normalized);
        }
    } else if updates.get("prom_port").map(|v| v.is_null()).unwrap_or(false) {
        instance.prom_port = None;
    }

    // Convert back to YAML with flattened global fields
    let yaml_content = config.to_yaml().map_err(|e| format!("Failed to serialize config to YAML: {}", e))?;

    // Write to file
    fs::write(config_path, yaml_content)?;

    Ok(())
}

/// Start Prometheus metrics server
pub async fn start_prom_server(port: &str, instance_id: &str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::net::SocketAddr;
    use tokio::net::TcpListener;

    init_metrics();

    let instance_id = instance_id.to_string();

    let addr_str = bind_addr_from_port(port);

    let addr: SocketAddr = addr_str.parse()?;
    let listener = TcpListener::bind(addr).await?;

    tracing::debug!("Hosting prom stats on {}/metrics", addr);
    serve_http_loop(listener, HttpMode::Instance { instance_id, web_bind: addr_str }).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::io::AsyncReadExt;

    async fn send_request(mode: HttpMode, request: &str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let request = request.to_string();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            handle_http_request(stream, &request, &mode).await.unwrap();
        });

        let mut client = tokio::net::TcpStream::connect(addr).await.unwrap();
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await.unwrap();
        server.await.unwrap();
        String::from_utf8_lossy(&buf).to_string()
    }

    fn temp_config_path() -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
        std::env::temp_dir().join(format!("rkstratum_config_test_{}_{}.yaml", std::process::id(), nanos))
    }

    #[tokio::test]
    async fn test_http_routing_and_config_write() {
        let config_path = temp_config_path();
        set_web_config_path(config_path.clone());
        std::fs::write(
            &config_path,
            r#"
kaspad_address: "127.0.0.1:16110"
stratum_port: ":5555"
min_share_diff: 8192
"#,
        )
        .unwrap();

        set_web_status_config("127.0.0.1:16110".to_string(), 2);

        let mode = HttpMode::Instance { instance_id: "0".to_string(), web_bind: "127.0.0.1:0".to_string() };

        let status_resp = send_request(mode.clone(), "GET /api/status HTTP/1.1\r\n\r\n").await;
        assert!(status_resp.contains("200 OK"));
        assert!(status_resp.contains("\"kaspad_address\""));
        assert!(status_resp.contains("\"instances\":2"));

        let stats_resp = send_request(mode.clone(), "GET /api/stats HTTP/1.1\r\n\r\n").await;
        assert!(stats_resp.contains("200 OK"));
        assert!(stats_resp.contains("application/json"));

        let config_resp = send_request(mode.clone(), "GET /api/config HTTP/1.1\r\n\r\n").await;
        assert!(config_resp.contains("200 OK"));
        assert!(config_resp.contains("\"kaspad_address\""));

        // SAFETY: test-only env change scoped to this process; no concurrent mutation expected.
        unsafe {
            std::env::set_var("RKSTRATUM_ALLOW_CONFIG_WRITE", "1");
        }
        let json_body = r#"{"kaspad_address":"127.0.0.2:16110","stratum_port":":5556","min_share_diff":4096}"#;
        let post_req = format!("POST /api/config HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}", json_body.len(), json_body);
        let post_resp = send_request(mode, &post_req).await;
        assert!(post_resp.contains("\"success\": true"));

        let saved = std::fs::read_to_string(&config_path).unwrap();
        assert!(!saved.contains("global:"));
        assert!(saved.contains("instances:"));
    }
}
