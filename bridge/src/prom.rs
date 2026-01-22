use prometheus::proto::MetricFamily;
#[cfg(feature = "rkstratum_cpu_miner")]
use prometheus::{Counter, register_counter};
use prometheus::{CounterVec, Gauge, GaugeVec, register_counter_vec, register_gauge, register_gauge_vec};
use serde::{Deserialize, Serialize};
#[cfg(feature = "rkstratum_cpu_miner")]
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

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

    let mut q = INTERNAL_CPU_RECENT_BLOCKS.get_or_init(|| parking_lot::Mutex::new(VecDeque::with_capacity(256))).lock();

    // De-dupe by hash
    if q.iter().any(|b| b.hash == hash) {
        return;
    }

    q.push_front(InternalCpuBlock { timestamp_unix: ts, bluescore, nonce, hash });
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
    };

    let mut worker_stats: HashMap<String, WorkerInfo> = HashMap::new();
    let mut worker_hash_values: HashMap<String, f64> = HashMap::new(); // Store hash values for hashrate calculation
    let mut worker_start_times: HashMap<String, f64> = HashMap::new(); // Store start times for hashrate calculation
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
                let labels = metric.get_label();
                let mut instance = String::new();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "instance" => instance = label.get_value().to_string(),
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let entry = worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        instance,
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });
                    // Aggregate across multiple time series for the same (instance,worker,wallet)
                    entry.blocks = entry.blocks.saturating_add(count);
                }
            }
        }

        // Parse share diff counter (for hashrate calculation)
        if name == "ks_valid_share_diff_counter" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut instance = String::new();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "instance" => instance = label.get_value().to_string(),
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let total_hash_value = metric.get_counter().get_value();
                    // Store hash value for hashrate calculation (aggregate across label variants)
                    *worker_hash_values.entry(key.clone()).or_insert(0.0) += total_hash_value;
                    // Ensure worker exists in stats
                    worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        instance,
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
                let mut instance = String::new();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "instance" => instance = label.get_value().to_string(),
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let entry = worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        instance,
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });
                    entry.shares = entry.shares.saturating_add(count);
                    stats.totalShares = stats.totalShares.saturating_add(count);
                }
            }
        }

        // Parse invalid share counter
        if name == "ks_invalid_share_counter" {
            for metric in family.get_metric() {
                let labels = metric.get_label();
                let mut instance = String::new();
                let mut worker_key = String::new();
                let mut wallet = String::new();
                let mut share_type = String::new();

                for label in labels {
                    match label.get_name() {
                        "instance" => instance = label.get_value().to_string(),
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        "type" => share_type = label.get_value().to_string(),
                        _ => {}
                    }
                }

                if !worker_key.is_empty() {
                    let key = format!("{}:{}:{}", instance, worker_key, wallet);
                    let count = metric.get_counter().get_value() as u64;
                    let worker = worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        instance,
                        worker: worker_key,
                        wallet,
                        hashrate: 0.0,
                        shares: 0,
                        stale: 0,
                        invalid: 0,
                        blocks: 0,
                    });

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
                let labels = metric.get_label();
                let mut instance = String::new();
                let mut worker_key = String::new();
                let mut wallet = String::new();

                for label in labels {
                    match label.get_name() {
                        "instance" => instance = label.get_value().to_string(),
                        "worker" => worker_key = label.get_value().to_string(),
                        "wallet" => wallet = label.get_value().to_string(),
                        _ => {}
                    }
                }

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
                    worker_stats.entry(key.clone()).or_insert_with(|| WorkerInfo {
                        instance,
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
    use yaml_rust::YamlLoader;

    let config_path = get_web_config_path();
    if let Ok(content) = fs::read_to_string(&config_path) {
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
    let config_path = get_web_config_path();
    let _guard = WEB_CONFIG_WRITE_LOCK.get_or_init(|| parking_lot::Mutex::new(())).lock();

    // Build YAML content directly from JSON values
    let mut out_str = String::new();
    out_str.push_str("# RKStratum Configuration\n");
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
