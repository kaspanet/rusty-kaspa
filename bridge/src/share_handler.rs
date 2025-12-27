use crate::{
    errors::*,
    jsonrpc_event::{JsonRpcEvent, JsonRpcResponse},
    kaspaapi::NODE_STATUS,
    log_colors::LogColors,
    mining_state::GetMiningState,
    prom::*,
    stratum_context::StratumContext,
};
use kaspa_consensus_core::block::Block;
// kaspa_pow used inline for PoW validation
use num_bigint::BigUint;
use num_traits::{ToPrimitive, Zero};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info, warn};

#[allow(dead_code)]
const VAR_DIFF_THREAD_SLEEP: u64 = 10;
#[allow(dead_code)]
const WORK_WINDOW: u64 = 80;
const STATS_PRUNE_INTERVAL: Duration = Duration::from_secs(60);
const STATS_PRINT_INTERVAL: Duration = Duration::from_secs(10);

// VarDiff tunables
const VARDIFF_MIN_ELAPSED_SECS: f64 = 30.0;
const VARDIFF_MAX_ELAPSED_SECS_NO_SHARES: f64 = 90.0;
const VARDIFF_MIN_SHARES: f64 = 3.0;
const VARDIFF_LOWER_RATIO: f64 = 0.75; // below this => decrease diff
const VARDIFF_UPPER_RATIO: f64 = 1.25; // above this => increase diff
const VARDIFF_MAX_STEP_UP: f64 = 2.0; // max 2x per adjustment tick
const VARDIFF_MAX_STEP_DOWN: f64 = 0.5; // max -50% per adjustment tick

fn vardiff_pow2_clamp_towards(current: f64, next: f64) -> f64 {
    if !next.is_finite() || next <= 0.0 {
        return 1.0;
    }

    let exp = if next >= current { next.log2().ceil() } else { next.log2().floor() };
    let clamped = 2_f64.powi(exp as i32);
    if clamped < 1.0 {
        1.0
    } else {
        clamped
    }
}

fn vardiff_compute_next_diff(current: f64, shares: f64, elapsed_secs: f64, expected_spm: f64, clamp_pow2: bool) -> Option<f64> {
    if !current.is_finite() || current <= 0.0 {
        return None;
    }
    if !elapsed_secs.is_finite() || elapsed_secs <= 0.0 {
        return None;
    }

    if shares == 0.0 && elapsed_secs >= VARDIFF_MAX_ELAPSED_SECS_NO_SHARES {
        let mut next = current * VARDIFF_MAX_STEP_DOWN;
        if next < 1.0 {
            next = 1.0;
        }
        if clamp_pow2 {
            next = vardiff_pow2_clamp_towards(current, next);
        }
        return if (next - current).abs() > f64::EPSILON { Some(next) } else { None };
    }

    if elapsed_secs < VARDIFF_MIN_ELAPSED_SECS || shares < VARDIFF_MIN_SHARES {
        return None;
    }

    let observed_spm = (shares / elapsed_secs) * 60.0;
    let ratio = observed_spm / expected_spm.max(1.0);
    if !ratio.is_finite() || ratio <= 0.0 {
        return None;
    }
    if ratio > VARDIFF_LOWER_RATIO && ratio < VARDIFF_UPPER_RATIO {
        return None;
    }

    let step = ratio.sqrt().clamp(VARDIFF_MAX_STEP_DOWN, VARDIFF_MAX_STEP_UP);
    let mut next = current * step;
    if next < 1.0 {
        next = 1.0;
    }
    if clamp_pow2 {
        next = vardiff_pow2_clamp_towards(current, next);
    }

    let rel_change = (next - current).abs() / current.max(1.0);
    if rel_change < 0.10 {
        return None;
    }
    if (next - current).abs() > f64::EPSILON {
        Some(next)
    } else {
        None
    }
}

struct StatsPrinterEntry {
    instance_id: String,
    inst_short: String,
    target_spm: f64,
    start: Instant,
    stats: Arc<Mutex<HashMap<String, WorkStats>>>,
    overall: Arc<WorkStats>,
}

static STATS_PRINTER_REGISTRY: Lazy<Mutex<Vec<StatsPrinterEntry>>> = Lazy::new(|| Mutex::new(Vec::new()));
static STATS_PRINTER_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone)]
pub struct WorkStats {
    pub blocks_found: Arc<Mutex<i64>>,
    pub shares_found: Arc<Mutex<i64>>,
    pub shares_diff: Arc<Mutex<f64>>,
    pub stale_shares: Arc<Mutex<i64>>,
    pub invalid_shares: Arc<Mutex<i64>>,
    pub worker_name: Arc<Mutex<String>>,
    pub start_time: Instant,
    pub last_share: Arc<Mutex<Instant>>,
    pub var_diff_start_time: Arc<Mutex<Option<Instant>>>,
    pub var_diff_shares_found: Arc<Mutex<i64>>,
    pub var_diff_window: Arc<Mutex<usize>>,
    pub min_diff: Arc<Mutex<f64>>,
}

impl WorkStats {
    pub fn new(worker_name: String) -> Self {
        Self {
            blocks_found: Arc::new(Mutex::new(0)),
            shares_found: Arc::new(Mutex::new(0)),
            shares_diff: Arc::new(Mutex::new(0.0)),
            stale_shares: Arc::new(Mutex::new(0)),
            invalid_shares: Arc::new(Mutex::new(0)),
            worker_name: Arc::new(Mutex::new(worker_name)),
            start_time: Instant::now(),
            last_share: Arc::new(Mutex::new(Instant::now())),
            var_diff_start_time: Arc::new(Mutex::new(None)),
            var_diff_shares_found: Arc::new(Mutex::new(0)),
            var_diff_window: Arc::new(Mutex::new(0)),
            min_diff: Arc::new(Mutex::new(0.0)),
        }
    }
}

pub struct ShareHandler {
    #[allow(dead_code)]
    tip_blue_score: Arc<Mutex<u64>>,
    stats: Arc<Mutex<HashMap<String, WorkStats>>>,
    overall: Arc<WorkStats>,
    instance_id: String, // Instance identifier for logging
}

impl ShareHandler {
    pub fn new(instance_id: String) -> Self {
        Self {
            tip_blue_score: Arc::new(Mutex::new(0)),
            stats: Arc::new(Mutex::new(HashMap::new())),
            overall: Arc::new(WorkStats::new("overall".to_string())),
            instance_id,
        }
    }

    fn log_prefix(&self) -> String {
        format!("[{}]", self.instance_id)
    }

    pub fn get_create_stats(&self, ctx: &StratumContext) -> WorkStats {
        let mut stats_map = self.stats.lock();

        let worker_id = {
            let worker_name = ctx.worker_name.lock();
            if !worker_name.is_empty() {
                worker_name.clone()
            } else {
                format!("{}:{}", ctx.remote_addr(), ctx.remote_port())
            }
        };

        if let Some(stats) = stats_map.get(&worker_id) {
            return stats.clone();
        }

        let stats = WorkStats::new(worker_id.clone());
        stats_map.insert(worker_id.clone(), stats.clone());
        drop(stats_map);

        // Initialize worker counters
        let wallet_addr = ctx.wallet_addr.lock().clone();
        let worker_name = stats.worker_name.lock().clone();
        init_worker_counters(&crate::prom::WorkerContext {
            worker_name: worker_name.clone(),
            miner: String::new(),
            wallet: wallet_addr.clone(),
            ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
        });

        stats
    }

    pub async fn handle_submit(
        &self,
        ctx: Arc<StratumContext>,
        event: JsonRpcEvent,
        kaspa_api: Arc<dyn KaspaApiTrait + Send + Sync>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let prefix = self.log_prefix();
        tracing::debug!("{} [SUBMIT] ===== SHARE SUBMISSION FROM {} =====", prefix, ctx.remote_addr);
        tracing::debug!("{} [SUBMIT] Event ID: {:?}", prefix, event.id);
        tracing::debug!("{} [SUBMIT] Params count: {}", prefix, event.params.len());
        tracing::debug!("{} [SUBMIT] Full params: {:?}", prefix, event.params);

        // Get per-client mining state from context
        let state = GetMiningState(&ctx);
        let _max_jobs = state.max_jobs() as u64;
        let current_counter = state.current_job_counter();
        let stored_ids = state.get_stored_job_ids();
        tracing::debug!("{} [SUBMIT] Retrieved MiningState - counter: {}, stored IDs: {:?}", prefix, current_counter, stored_ids);

        // Validate submit
        // According to stratum protocol: params[0] = address.name, params[1] = jobid, params[2] = nonce
        // We get the address from authorize, but we can optionally validate params[0] if present
        if event.params.len() < 3 {
            tracing::error!("{} [SUBMIT] ERROR: Expected at least 3 params, got {}", prefix, event.params.len());
            let wallet_addr = ctx.wallet_addr.lock().clone();
            record_worker_error(&wallet_addr, ErrorShortCode::BadDataFromMiner.as_str());
            return Err("malformed event, expected at least 3 params".into());
        }

        let prefix = self.log_prefix();
        tracing::debug!("{} [SUBMIT] Params[0] (address/identity): {:?}", prefix, event.params.first());
        tracing::debug!("{} [SUBMIT] Params[1] (job_id): {:?}", prefix, event.params.get(1));
        tracing::debug!("{} [SUBMIT] Params[2] (nonce): {:?}", prefix, event.params.get(2));

        // Optionally validate params[0] (address.name) if present
        // Some miners send it, others don't - we get address from authorize anyway
        if let Some(Value::String(submitted_identity)) = event.params.first() {
            let wallet_addr = ctx.wallet_addr.lock().clone();
            let _worker_name = ctx.worker_name.lock().clone();

            // Extract address from submitted identity (format: "address.worker")
            let parts: Vec<&str> = submitted_identity.split('.').collect();
            let submitted_address = parts[0];

            // Check if submitted address matches authorized address (case-insensitive, ignore prefix)
            let submitted_clean = submitted_address.trim_start_matches("kaspa:").trim_start_matches("kaspatest:");
            let authorized_clean = wallet_addr.trim_start_matches("kaspa:").trim_start_matches("kaspatest:");

            if submitted_clean.to_lowercase() != authorized_clean.to_lowercase() {
                tracing::debug!(
                    "Submit params[0] address mismatch: submitted '{}' vs authorized '{}' (using authorized)",
                    submitted_identity,
                    wallet_addr
                );
            } else {
                tracing::debug!("Submit params[0] matches authorized address: {}", submitted_identity);
            }
        }

        // Parse job ID - can be either string or number
        let job_id = match &event.params[1] {
            serde_json::Value::String(s) => {
                tracing::debug!("[SUBMIT] Job ID is string: '{}'", s);
                s.parse::<u64>().map_err(|e| format!("job id is not parsable as a number: {}", e))?
            }
            serde_json::Value::Number(n) => {
                tracing::debug!("[SUBMIT] Job ID is number: {}", n);
                n.as_u64().ok_or("job id number is out of range")?
            }
            _ => {
                tracing::error!("[SUBMIT] ERROR: Job ID must be string or number, got: {:?}", event.params[1]);
                return Err("job id must be a string or number".into());
            }
        };

        tracing::debug!("[SUBMIT] Parsed job_id: {}", job_id);

        // Get current job counter for debugging
        let current_job_counter = state.current_job_counter();
        tracing::debug!(
            "[SUBMIT] Current job counter: {}, submitted job_id: {} (diff: {})",
            current_job_counter,
            job_id,
            if job_id > current_job_counter {
                format!("+{}", job_id - current_job_counter)
            } else {
                format!("-{}", current_job_counter - job_id)
            }
        );

        // Fail immediately if job doesn't exist
        //          if !exists { return nil, fmt.Errorf("job does not exist. stale?") }
        // GetJob returns job at slot (id % maxJobs) without verifying ID matches
        let job = state.get_job(job_id);
        let current_counter = state.current_job_counter();
        let prefix = self.log_prefix();
        let job = match job {
            Some(j) => {
                tracing::debug!("{} [SUBMIT] Found job ID {} (current counter: {})", prefix, job_id, current_counter);
                j
            }
            None => {
                // Job doesn't exist at slot - log debug info
                let stored_job_ids = state.get_stored_job_ids();
                tracing::warn!(
                    "[SUBMIT] Job ID {} not found at slot {} (current counter: {}, stored IDs: {:?})",
                    job_id,
                    job_id % 300,
                    current_counter,
                    stored_job_ids
                );
                // Job doesn't exist - fail immediately
                let wallet_addr = ctx.wallet_addr.lock().clone();
                record_worker_error(&wallet_addr, ErrorShortCode::MissingJob.as_str());
                return Err("job does not exist. stale?".into());
            }
        };

        let nonce_str = event.params[2].as_str().ok_or("nonce must be a string")?;
        tracing::debug!("[SUBMIT] Raw nonce string: '{}'", nonce_str);

        let nonce_str = nonce_str.replace("0x", "");
        tracing::debug!("[SUBMIT] Nonce after removing 0x: '{}' (length: {} hex chars)", nonce_str, nonce_str.len());

        // Add extranonce if enabled
        let mut final_nonce_str = nonce_str.clone();
        {
            let extranonce = ctx.extranonce.lock();
            if !extranonce.is_empty() {
                let extranonce_val = extranonce.clone();
                let extranonce2_len = 16 - extranonce_val.len();

                // Only prepend extranonce if nonce is shorter than expected
                if nonce_str.len() <= extranonce2_len {
                    // Format with zero-padding on the right
                    final_nonce_str = format!("{}{:0>width$}", extranonce_val, nonce_str, width = extranonce2_len);
                    tracing::debug!(
                        "[SUBMIT] Extranonce prepended: '{}' = '{}' + '{:0>width$}'",
                        final_nonce_str,
                        extranonce_val,
                        nonce_str,
                        width = extranonce2_len
                    );
                }
            }
        } // extranonce guard is dropped here

        tracing::debug!("[SUBMIT] Final nonce string: '{}'", final_nonce_str);
        let nonce_val = {
            let prefix = self.log_prefix();
            u64::from_str_radix(&final_nonce_str, 16).map_err(|e| {
                tracing::error!("{} [SUBMIT] ERROR: Failed to parse nonce '{}' as hex: {}", prefix, final_nonce_str, e);
                format!("failed parsing noncestr: {}", e)
            })?
        };

        tracing::debug!("[SUBMIT] Parsed nonce value (u64): {}", nonce_val);
        tracing::debug!("[SUBMIT] Nonce hex: {:016x}", nonce_val);

        // PoW validation with job ID workaround
        // Go validates the submitted job first, then tries previous jobs if share doesn't meet pool difficulty
        // This workaround handles IceRiver/Bitmain ASICs that submit jobs with incorrect IDs
        let mut current_job_id = job_id;
        let mut current_job = job;
        let mut invalid_share = false;
        let mut pow_passed;
        let mut pow_value;
        let max_jobs = state.max_jobs() as u64;

        tracing::debug!("[SUBMIT] Starting PoW validation for job_id: {} (max_jobs: {})", current_job_id, max_jobs);

        loop {
            // DIAGNOSTIC: Run full diagnostic on first share
            static DIAGNOSTIC_RUN: std::sync::Once = std::sync::Once::new();
            let header = &current_job.block.header;
            let mut header_clone = (**header).clone();

            DIAGNOSTIC_RUN.call_once(|| {
                tracing::debug!("{}", LogColors::block("===== RUNNING POW DIAGNOSTIC ====="));
                crate::pow_diagnostic::diagnose_pow_issue(&header_clone, nonce_val);
                tracing::debug!("{}", LogColors::block("===== DIAGNOSTIC COMPLETE ====="));
            });

            // DEBUG: Compare what we sent to ASIC vs what we're validating (moved to debug level)
            tracing::debug!("{} {}", LogColors::validation("[DEBUG]"), LogColors::label("===== VALIDATION DEBUG ====="));
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[DEBUG]"),
                LogColors::label("Job we sent to ASIC:"),
                format!("job_id={}, timestamp={}", current_job_id, current_job.block.header.timestamp)
            );
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[DEBUG]"),
                LogColors::label("ASIC submitted:"),
                format!("job_id={}, nonce=0x{:x}", current_job_id, nonce_val)
            );
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[DEBUG]"),
                LogColors::label("Header we're validating:"),
                format!("timestamp={}, nonce={}, bits=0x{:08x}", header_clone.timestamp, header_clone.nonce, header_clone.bits)
            );

            // Set the nonce in the header
            header_clone.nonce = nonce_val;

            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[DEBUG]"),
                LogColors::label("After setting nonce:"),
                format!("timestamp={}, nonce=0x{:x}, bits=0x{:08x}", header_clone.timestamp, header_clone.nonce, header_clone.bits)
            );

            // Use kaspa_pow::State for proper PoW validation
            use kaspa_pow::State as PowState;
            let pow_state = PowState::new(&header_clone);
            let (check_passed, pow_value_uint256) = pow_state.check_pow(nonce_val);

            // Convert Uint256 to BigUint for comparison
            pow_value = num_bigint::BigUint::from_bytes_be(&pow_value_uint256.to_be_bytes());

            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[DEBUG]"),
                LogColors::label("PowState result:"),
                format!("check_passed={}, pow_value={:x}", check_passed, pow_value)
            );

            // Calculate network target from header.bits
            use crate::hasher::calculate_target;
            let network_target = calculate_target(header_clone.bits as u64);

            // Check if pow_value meets network target (lower hash is better)
            let meets_network_target = pow_value <= network_target;
            pow_passed = meets_network_target;

            let pow_value_bytes = pow_value.to_bytes_be();
            let network_target_bytes = network_target.to_bytes_be();

            tracing::debug!("[SUBMIT] Target comparison:");
            tracing::debug!("[SUBMIT]   - pow_value: {:x} ({} bytes)", pow_value, pow_value_bytes.len());
            tracing::debug!("[SUBMIT]   - network_target: {:x} ({} bytes)", network_target, network_target_bytes.len());
            tracing::debug!("[SUBMIT]   - meets_network_target: {}", meets_network_target);

            tracing::debug!(
                "[SUBMIT] PoW check result: passed={}, pow_value={:x}, network_target={:x}, header.bits={}",
                pow_passed,
                pow_value,
                network_target,
                header_clone.bits
            );

            // Log detailed validation information with colors (moved to debug level)
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[VALIDATION]"),
                LogColors::label("PoW Validation -"),
                format!(
                    "Nonce: {:x}, Pow Value: {:x} ({} bytes), Network Target: {:x} ({} bytes)",
                    nonce_val,
                    pow_value,
                    pow_value_bytes.len(),
                    network_target,
                    network_target_bytes.len()
                )
            );
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[VALIDATION]"),
                LogColors::label("Comparison:"),
                format!("pow_value <= network_target = {} (lower hash is better)", meets_network_target)
            );
            tracing::debug!(
                "{} {} {}",
                LogColors::validation("[VALIDATION]"),
                LogColors::label("PowState.check_pow() result:"),
                format!("passed={}, Header bits: {}", pow_passed, header_clone.bits)
            );

            // On devnet, network difficulty is very low, so we should see blocks being found
            // Log at debug level (detailed validation logs moved to debug)
            if pow_passed {
                tracing::debug!(
                    "{} {} {}",
                    LogColors::validation("[VALIDATION]"),
                    LogColors::block("*** NETWORK TARGET PASSED ***"),
                    format!("pow_value={:x} <= network_target={:x}", pow_value, network_target)
                );
            } else if !network_target.is_zero() {
                let ratio = if !pow_value.is_zero() {
                    let target_f64 = network_target.to_f64().unwrap_or(0.0);
                    let pow_f64 = pow_value.to_f64().unwrap_or(1.0);
                    if pow_f64 > 0.0 {
                        (target_f64 / pow_f64) * 100.0
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                tracing::debug!(
                    "{} {} {}",
                    LogColors::validation("[VALIDATION]"),
                    LogColors::label("Network target NOT met -"),
                    format!("pow_value={:x} > network_target={:x} ({}% of target)", pow_value, network_target, ratio)
                );
            } else {
                warn!("{} {}", LogColors::validation("[VALIDATION]"), LogColors::error("Network target is ZERO - cannot validate!"));
            }

            // Check network target (block)
            // Use meets_network_target (not pow_passed) for network target validation
            // Go code compares: powValue.Cmp(&powState.Target) <= 0 where Target is network target from header.bits
            // We calculate network_target directly from current job's header.bits (not from stored state)
            // This ensures we use the correct target for each job, as different jobs may have different header.bits
            if meets_network_target {
                let wallet_addr = ctx.wallet_addr.lock().clone();
                let worker_name = ctx.worker_name.lock().clone();
                let prefix = self.log_prefix();

                info!("{} {}", prefix, LogColors::block("===== BLOCK FOUND! ===== PoW passed network target"));
                info!(
                    "{} {} {} {}",
                    prefix,
                    LogColors::block("[BLOCK]"),
                    LogColors::label("ACCEPTANCE REASON:"),
                    format!(
                        "pow_value ({:x}) <= network_target ({:x}) - Block meets network difficulty requirement",
                        pow_value, network_target
                    )
                );
                info!(
                    "{} {} {} {}",
                    prefix,
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Worker:"),
                    format!("{}, Wallet: {}, Nonce: {:x}, Pow Value: {:x}", worker_name, wallet_addr, nonce_val, pow_value)
                );

                // Log block details before creating the block (to avoid borrow issues)
                let header_bits = header_clone.bits;
                let header_version = header_clone.version;
                let original_timestamp = header_clone.timestamp;

                // Block found - submit it
                // Only set the nonce - keep all other header fields from the real block template
                // The header comes directly from the Kaspa node via get_block_template_call()
                // We preserve: version, bits, timestamp, all hash fields, parents, scores, etc.
                header_clone.nonce = nonce_val;

                // Verify timestamp is still valid (not too old)
                // Kaspa typically accepts blocks with timestamps within a reasonable window
                // Block templates are fetched frequently, so the timestamp should be recent
                let current_time_ms =
                    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
                let timestamp_age_ms = current_time_ms.saturating_sub(original_timestamp);
                let timestamp_age_sec = timestamp_age_ms / 1000;

                // Log header verification to confirm we're using real headers (moved to debug level)
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Header Verification:"),
                    "Using REAL header from Kaspa node block template"
                );
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("  - Header Version:"), header_version);
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("  - Header Bits:"),
                    format!("{} (0x{:x})", header_bits, header_bits)
                );
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("  - Timestamp:"),
                    format!("{} (age: {}s, preserved from template)", original_timestamp, timestamp_age_sec)
                );
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("  - Nonce:"),
                    format!("{:x} (set from ASIC submission)", nonce_val)
                );

                // Warn if timestamp is very old (more than 60 seconds)
                // This shouldn't happen with frequent template updates, but log it for debugging
                if timestamp_age_sec > 60 {
                    warn!(
                        "{} {} {}",
                        LogColors::block("[BLOCK]"),
                        LogColors::error("⚠ Timestamp is old:"),
                        format!("{} seconds old - block template may be stale", timestamp_age_sec)
                    );
                }

                // Create new block with updated header
                let transactions_vec = current_job.block.transactions.iter().cloned().collect();
                let block = Block::from_arcs(Arc::new(header_clone), Arc::new(transactions_vec));
                let blue_score = block.header.blue_score;

                // Calculate block hash immediately after block creation
                // Use kaspa_consensus_core::hashing::header::hash() for block hash calculation
                // In Kaspa, the block hash is the header hash (transactions are represented by hash_merkle_root in header)
                use kaspa_consensus_core::hashing::header;
                let block_hash = header::hash(&block.header).to_string();

                // Log prominent "Block Found" message with hash
                info!("{} {}", prefix, LogColors::block(&format!("🎉 BLOCK FOUND! Hash: {}", block_hash)));
                info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Hash:"), block_hash);
                info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
                info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Wallet:"), wallet_addr);
                info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Nonce:"), format!("{:x}", nonce_val));

                // Log block submission details before submission (moved to debug level)
                tracing::debug!("{} {}", LogColors::block("[BLOCK]"), LogColors::block("=== SUBMITTING BLOCK TO NODE ==="));
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Nonce:"),
                    format!("{:x} (0x{:016x})", nonce_val, nonce_val)
                );
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Bits:"),
                    format!("{} (0x{:08x})", header_bits, header_bits)
                );
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Timestamp:"),
                    format!("{}", original_timestamp)
                );
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Blue Score:"), blue_score);
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Pow Value:"), format!("{:x}", pow_value));
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Network Target:"),
                    format!("{:x}", network_target)
                );
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Job ID:"), current_job_id);
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Wallet:"), wallet_addr);
                tracing::debug!(
                    "{} {} {}",
                    LogColors::block("[BLOCK]"),
                    LogColors::label("Client:"),
                    format!("{}:{}", ctx.remote_addr(), ctx.remote_port())
                );
                tracing::debug!("{} {} {}", LogColors::block("[BLOCK]"), LogColors::label("Block Hash:"), block_hash);
                tracing::debug!("{} {}", LogColors::block("[BLOCK]"), "Calling kaspa_api.submit_block()...");

                // Submit block to node
                let block_submit_result = kaspa_api.submit_block(block.clone()).await;

                match block_submit_result {
                    Ok(_response) => {
                        let prefix = self.log_prefix();
                        // Block accepted - log after submit to get it submitted faster
                        info!(
                            "{} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::block(&format!("✓ Block submitted successfully! Hash: {}", block_hash))
                        );
                        info!(
                            "{} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::block(&format!("🎉🎉🎉 BLOCK ACCEPTED BY NODE! 🎉🎉🎉 Hash: {}", block_hash))
                        );
                        info!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("  - Worker:"), worker_name);
                        info!(
                            "{} {} {} {}",
                            prefix,
                            LogColors::block("[BLOCK]"),
                            LogColors::label("  - Nonce:"),
                            format!("{:x}", nonce_val)
                        );

                        // Record block found statistics
                        let stats = self.get_create_stats(&ctx);
                        *stats.blocks_found.lock() += 1;
                        *self.overall.blocks_found.lock() += 1;

                        record_block_found(
                            &crate::prom::WorkerContext {
                                worker_name: worker_name.clone(),
                                miner: String::new(),
                                wallet: wallet_addr.clone(),
                                ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                            },
                            nonce_val,
                            blue_score,
                            block_hash.clone(),
                        );

                        // Return allows HandleSubmit to record share (blocks are shares too!)
                        // After successful block submission, continue to record share at end of function
                        // Don't return early - let the code continue to record the share
                        invalid_share = false;
                        break;
                    }
                    Err(e) => {
                        let prefix = self.log_prefix();
                        // Only check for "ErrDuplicateBlock" (not "duplicate" or "stale")
                        // Block submission failed
                        let error_str = e.to_string();
                        error!("{} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("✗ Block submission FAILED"));
                        error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Worker:"), worker_name);
                        error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::label("Blockhash:"), block_hash);
                        error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Error:"), error_str);

                        if error_str.contains("ErrDuplicateBlock") {
                            // Block rejected, stale
                            warn!("{} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("block rejected, stale"));
                            warn!(
                                "{} {} {} {}",
                                prefix,
                                LogColors::block("[BLOCK]"),
                                LogColors::label("REJECTION REASON:"),
                                "Block was already submitted to the network (stale/duplicate)"
                            );

                            let stats = self.get_create_stats(&ctx);
                            *stats.stale_shares.lock() += 1;
                            *self.overall.stale_shares.lock() += 1;

                            record_stale_share(&crate::prom::WorkerContext {
                                worker_name: worker_name.clone(),
                                miner: String::new(),
                                wallet: wallet_addr.clone(),
                                ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                            });
                            ctx.reply_stale_share(event.id.clone()).await?;
                            return Ok(());
                        } else {
                            // Block rejected, unknown issue (probably bad pow)
                            warn!(
                                "{} {} {}",
                                prefix,
                                LogColors::block("[BLOCK]"),
                                LogColors::error("block rejected, unknown issue (probably bad pow)")
                            );
                            error!(
                                "{} {} {} {}",
                                prefix,
                                LogColors::block("[BLOCK]"),
                                LogColors::label("REJECTION REASON:"),
                                "Block failed node validation (probably bad pow)"
                            );
                            error!("{} {} {} {}", prefix, LogColors::block("[BLOCK]"), LogColors::error("Error:"), error_str);

                            let stats = self.get_create_stats(&ctx);
                            *stats.invalid_shares.lock() += 1;
                            *self.overall.invalid_shares.lock() += 1;

                            record_invalid_share(&crate::prom::WorkerContext {
                                worker_name: worker_name.clone(),
                                miner: String::new(),
                                wallet: wallet_addr.clone(),
                                ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
                            });
                            ctx.reply_bad_share(event.id.clone()).await?;
                            return Ok(());
                        }
                    }
                }
            }

            // Check pool difficulty
            let pool_target = state.stratum_diff().map(|d| d.target_value.clone()).unwrap_or_else(BigUint::zero);

            // Compare FULL pow_value against pool_target (not just lower bits)
            // Compare full 256-bit values
            let pow_bytes = pow_value.to_bytes_be();
            let target_bytes = pool_target.to_bytes_be();

            // Log difficulty check for debugging
            if pool_target.is_zero() {
                tracing::warn!("stratum_diff target is zero! pow_value: {:x}, pool_target: {:x}", pow_value, pool_target);
            } else {
                let pow_len = pow_bytes.len();
                let target_len = target_bytes.len();

                tracing::debug!("difficulty check: nonce: {:x} ({}), pow_value (full): {:x} ({} bytes), pool_target: {:x} ({} bytes), diff_value: {:?}, pow_value <= pool_target = {}", 
                              nonce_val, nonce_val, pow_value, pow_len, pool_target, target_len, state.stratum_diff().map(|d| d.diff_value), pow_value <= pool_target);
                tracing::debug!(
                    "Full comparison - pow_value: {:x} ({} bytes), pool_target: {:x} ({} bytes)",
                    pow_value,
                    pow_len,
                    pool_target,
                    target_len
                );
            }

            // Check pool difficulty (stratum target)
            // If pow_value >= pool_target, share doesn't meet pool difficulty
            // Higher hash value means worse share
            if pow_value >= pool_target {
                // Share doesn't meet pool difficulty - might be wrong job ID (moved to debug to keep terminal clean)
                let worker_name = ctx.worker_name.lock().clone();
                tracing::debug!(
                    "{} {} {}",
                    LogColors::validation("✗ INVALID SHARE (too high)"),
                    LogColors::label("worker:"),
                    format!(
                        "{}, nonce: {:x}, pow_value: {:x}, pool_target: {:x}, pow_ge_pool_target: true",
                        worker_name, nonce_val, pow_value, pool_target
                    )
                );

                if current_job_id == job_id {
                    tracing::debug!("low diff share... checking for bad job ID ({})", current_job_id);
                    invalid_share = true;
                }

                // Job ID workaround for Bitmain/IceRiver ASICs - try previous jobs
                // Validate job ID: jobId == 1 || jobId%maxJobs == submitInfo.jobId%maxJobs+1
                if current_job_id == 1 || (current_job_id % max_jobs == ((job_id % max_jobs) + 1) % max_jobs) {
                    // Exhausted all previous blocks (wrapped around or reached job 1)
                    tracing::debug!(
                        "Job ID loop exhausted: current_job_id={}, job_id={}, max_jobs={}",
                        current_job_id,
                        job_id,
                        max_jobs
                    );
                    break;
                } else {
                    // Try previous job ID
                    let prev_job_id = current_job_id - 1;
                    if let Some(prev_job) = state.get_job(prev_job_id) {
                        current_job_id = prev_job_id;
                        current_job = prev_job;
                        tracing::debug!("Trying previous job ID: {} (submitted as {})", current_job_id, job_id);
                        // Continue loop to validate with previous job
                        continue;
                    } else {
                        // Job doesn't exist, exit loop - bad share will be recorded
                        tracing::debug!("Previous job ID {} doesn't exist, exiting loop", prev_job_id);
                        break;
                    }
                }
            } else {
                // Valid share (pow_value < pool_target) - moved to debug to keep terminal clean
                let worker_name = ctx.worker_name.lock().clone();
                tracing::debug!(
                    "{} {} {}",
                    LogColors::validation("✓ VALID SHARE"),
                    LogColors::label("worker:"),
                    format!(
                        "{}, nonce: {:x}, pow_value: {:x}, pool_target: {:x}, pow_lt_pool_target: true",
                        worker_name, nonce_val, pow_value, pool_target
                    )
                );

                if invalid_share {
                    tracing::debug!("found correct job ID: {} (submitted as {})", current_job_id, job_id);
                }
                invalid_share = false;
                break;
            }
        }

        let stats = self.get_create_stats(&ctx);

        if invalid_share {
            tracing::debug!("low diff share confirmed");
            *stats.invalid_shares.lock() += 1;
            *self.overall.invalid_shares.lock() += 1;

            let wallet_addr = ctx.wallet_addr.lock().clone();
            let worker_name = ctx.worker_name.lock().clone();
            record_weak_share(&crate::prom::WorkerContext {
                worker_name: worker_name.clone(),
                miner: String::new(),
                wallet: wallet_addr.clone(),
                ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
            });

            if let Some(id) = &event.id {
                let _ = ctx.reply_low_diff_share(id).await;
            }
            return Ok(());
        }

        // Record valid share
        //   stats.SharesFound.Add(1)
        //   stats.VarDiffSharesFound.Add(1)
        //   stats.SharesDiff.Add(state.stratumDiff.hashValue)  // Accumulates hashValue, not diffValue!
        //   stats.LastShare = time.Now()
        //   sh.overall.SharesFound.Add(1)
        //   RecordShareFound(ctx, state.stratumDiff.hashValue)
        let stats = self.get_create_stats(&ctx);
        *stats.shares_found.lock() += 1;
        *stats.var_diff_shares_found.lock() += 1;

        // Get hashValue from stratum_diff
        let hash_value = state.stratum_diff().map(|d| d.hash_value).unwrap_or(0.0);

        // Accumulate hashValue for hashrate calculation
        *stats.shares_diff.lock() += hash_value;
        *stats.last_share.lock() = Instant::now();
        *self.overall.shares_found.lock() += 1;

        let wallet_addr = ctx.wallet_addr.lock().clone();
        let worker_name = ctx.worker_name.lock().clone();
        record_share_found(
            &crate::prom::WorkerContext {
                worker_name: worker_name.clone(),
                miner: String::new(),
                wallet: wallet_addr.clone(),
                ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
            },
            hash_value,
        );

        ctx.reply(JsonRpcResponse { id: event.id.clone(), result: Some(serde_json::Value::Bool(true)), error: None })
            .await
            .map_err(|e| format!("failed to reply: {}", e))?;
        Ok(())
    }

    #[allow(dead_code)]
    async fn submit_block(
        &self,
        _ctx: &StratumContext,
        _block: Block,
        _nonce: u64,
        _event_id: &serde_json::Value,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Block submission is handled at the HandleSubmit level
        // This method is kept for compatibility but actual submission
        // happens when PoW passes network target in handle_submit
        Ok(())
    }

    pub fn set_client_vardiff(&self, ctx: &StratumContext, min_diff: f64) -> f64 {
        let stats = self.get_create_stats(ctx);
        let previous = *stats.min_diff.lock();
        *stats.min_diff.lock() = min_diff;
        *stats.var_diff_start_time.lock() = Some(Instant::now());
        *stats.var_diff_shares_found.lock() = 0;
        *stats.var_diff_window.lock() = 0;
        previous
    }

    pub fn get_client_vardiff(&self, ctx: &StratumContext) -> f64 {
        let stats = self.get_create_stats(ctx);
        let min_diff = *stats.min_diff.lock();
        min_diff
    }

    pub fn start_client_vardiff(&self, ctx: &StratumContext) {
        let stats = self.get_create_stats(ctx);
        if stats.var_diff_start_time.lock().is_none() {
            *stats.var_diff_start_time.lock() = Some(Instant::now());
            *stats.var_diff_shares_found.lock() = 0;
        }
    }

    pub fn start_prune_stats_thread(&self) {
        let stats = Arc::clone(&self.stats);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(STATS_PRUNE_INTERVAL);
            loop {
                interval.tick().await;
                let mut stats_map = stats.lock();
                let now = Instant::now();
                stats_map.retain(|_, v| {
                    let last_share = *v.last_share.lock();
                    let shares = *v.shares_found.lock();
                    (shares > 0 || now.duration_since(v.start_time) < Duration::from_secs(180))
                        && now.duration_since(last_share) < Duration::from_secs(600)
                });
                // Note: Pruning is silent, no logs needed
            }
        });
    }

    pub fn start_print_stats_thread(&self, target_spm: u32) {
        let target_spm = if target_spm == 0 { 20.0 } else { target_spm as f64 };
        let instance_id = self.instance_id.clone();
        let inst_short = {
            let digits: String = instance_id.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u32>() {
                format!("Ins{:02}", n)
            } else {
                "Ins??".to_string()
            }
        };

        {
            let mut registry = STATS_PRINTER_REGISTRY.lock();
            if !registry.iter().any(|e| e.instance_id == instance_id) {
                registry.push(StatsPrinterEntry {
                    instance_id,
                    inst_short,
                    target_spm,
                    start: Instant::now(),
                    stats: Arc::clone(&self.stats),
                    overall: Arc::clone(&self.overall),
                });
            }
        }

        if STATS_PRINTER_STARTED.swap(true, Ordering::AcqRel) {
            return;
        }

        tokio::spawn(async move {
            fn trunc<'a>(s: &'a str, max: usize) -> Cow<'a, str> {
                if s.len() <= max {
                    Cow::Borrowed(s)
                } else {
                    Cow::Owned(s.chars().take(max).collect())
                }
            }

            const WORKER_W: usize = 16;
            const INST_W: usize = 5;
            const HASH_W: usize = 11;
            const DIFF_W: usize = 6;
            const SPM_W: usize = 11;
            const TRND_W: usize = 4;
            const ACC_W: usize = 12;
            const BLK_W: usize = 6;
            const TIME_W: usize = 7;

            fn border() -> String {
                format!(
                    "+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+-{}-+",
                    "-".repeat(WORKER_W),
                    "-".repeat(INST_W),
                    "-".repeat(HASH_W),
                    "-".repeat(DIFF_W),
                    "-".repeat(SPM_W),
                    "-".repeat(TRND_W),
                    "-".repeat(ACC_W),
                    "-".repeat(BLK_W),
                    "-".repeat(TIME_W)
                )
            }

            fn header() -> String {
                format!(
                    "| {:<WORKER_W$} | {:<INST_W$} | {:>HASH_W$} | {:>DIFF_W$} | {:>SPM_W$} | {:<TRND_W$} | {:>ACC_W$} | {:>BLK_W$} | {:>TIME_W$} |",
                    "Worker",
                    "Inst",
                    "Hash",
                    "Diff",
                    "SPM/tgt",
                    "Trnd",
                    "Acc/Stl/Inv",
                    "Blocks",
                    "Time",
                )
            }

            let mut interval = tokio::time::interval(STATS_PRINT_INTERVAL);
            loop {
                interval.tick().await;

                let node_status = {
                    let s = NODE_STATUS.lock();
                    s.clone()
                };

                let entries = {
                    let registry = STATS_PRINTER_REGISTRY.lock();
                    registry
                        .iter()
                        .map(|e| (e.inst_short.clone(), e.target_spm, e.start, Arc::clone(&e.stats), Arc::clone(&e.overall)))
                        .collect::<Vec<_>>()
                };

                if entries.is_empty() {
                    continue;
                }

                let mut rows: Vec<(String, String)> = Vec::new();
                let mut total_rate = 0.0;
                let mut total_shares: i64 = 0;
                let mut total_stales: i64 = 0;
                let mut total_invalids: i64 = 0;
                let mut total_blocks: i64 = 0;

                let now = Instant::now();
                let start = entries.iter().map(|(_, _, start, _, _)| *start).max_by_key(|t| t.elapsed()).unwrap_or_else(Instant::now);
                let total_uptime_mins = now.duration_since(start).as_secs_f64() / 60.0;

                let mut total_target: Option<f64> = Some(entries[0].1);
                for (inst_short, target_spm, _, stats, overall) in entries.iter() {
                    if let Some(t) = total_target {
                        if (t - *target_spm).abs() > 0.0001 {
                            total_target = None;
                        }
                    }

                    total_shares += *overall.shares_found.lock();
                    total_stales += *overall.stale_shares.lock();
                    total_invalids += *overall.invalid_shares.lock();
                    total_blocks += *overall.blocks_found.lock();

                    let stats_map = stats.lock();
                    for (_, v) in stats_map.iter() {
                        let elapsed = v.start_time.elapsed().as_secs_f64();
                        let rate = if elapsed > 0.0 {
                            let total_hash_value = *v.shares_diff.lock();
                            total_hash_value / elapsed
                        } else {
                            0.0
                        };
                        total_rate += rate;

                        let shares = *v.shares_found.lock();
                        let stales = *v.stale_shares.lock();
                        let invalids = *v.invalid_shares.lock();
                        let blocks = *v.blocks_found.lock();
                        let min_diff = *v.min_diff.lock();

                        let spm = if elapsed > 0.0 { (shares as f64) / (elapsed / 60.0) } else { 0.0 };
                        let trend = if spm > *target_spm * 1.2 {
                            "up"
                        } else if spm < *target_spm * 0.8 {
                            "down"
                        } else {
                            "flat"
                        };

                        let uptime_mins = v.start_time.elapsed().as_secs_f64() / 60.0;
                        let worker = v.worker_name.lock().clone();

                        let spm_tgt = format!("{:>4.1}/{:<4.1}", spm, *target_spm);

                        let line = format!(
                            "| {:<WORKER_W$} | {:<INST_W$} | {:>HASH_W$} | {:>DIFF_W$} | {:>SPM_W$} | {:<TRND_W$} | {:>ACC_W$} | {:>BLK_W$} | {:>TIME_W$} |",
                            trunc(&worker, WORKER_W),
                            inst_short,
                            format_hashrate(rate),
                            min_diff.round() as u64,
                            spm_tgt,
                            trend,
                            format!("{}/{}/{}", shares, stales, invalids),
                            blocks,
                            format!("{:.1}m", uptime_mins)
                        );
                        let sort_key = format!("{}:{}", inst_short, worker);
                        rows.push((sort_key, line));
                    }
                }

                rows.sort_by(|a, b| a.0.cmp(&b.0));

                let top = border();
                let sep = border();
                let hdr = header();

                let mut out = Vec::new();

                let sync_str = match node_status.is_synced {
                    Some(true) => "synced".to_string(),
                    Some(false) => "syncing".to_string(),
                    None => "unknown".to_string(),
                };
                let conn_str = if node_status.is_connected { "connected" } else { "disconnected" };

                let net = node_status.network_id.as_deref().unwrap_or("-");
                let ver = node_status.server_version.as_deref().unwrap_or("-");
                let peers = node_status.peers.map(|p| p.to_string()).unwrap_or_else(|| "-".to_string());
                let vdaa = node_status.virtual_daa_score.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                let blocks = node_status.block_count.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                let headers = node_status.header_count.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());
                let diff = node_status.difficulty.map(|d| format!("{:.2}", d)).unwrap_or_else(|| "-".to_string());
                let tip = node_status.tip_hash.as_deref().unwrap_or("-");
                let mempool = node_status.mempool_size.map(|v| v.to_string()).unwrap_or_else(|| "-".to_string());

                out.push(format!(
                    "[NODE] {} / {} | net={} | ver={} | peers={} | vdaa={} | blocks={}/{} | diff={} | mempool={} | tip={}",
                    conn_str, sync_str, net, ver, peers, vdaa, blocks, headers, diff, mempool, tip
                ));

                out.push(top.clone());
                out.push(hdr);
                out.push(sep.clone());

                for (_, line) in rows.iter() {
                    out.push(line.clone());
                }

                out.push(sep.clone());

                let overall_spm = if total_uptime_mins > 0.0 { (total_shares as f64) / total_uptime_mins } else { 0.0 };
                let total_spm_tgt = match total_target {
                    Some(t) => format!("{:>4.1}/{:<4.1}", overall_spm, t),
                    None => format!("{:>4.1}/-", overall_spm),
                };

                out.push(format!(
                    "| {:<WORKER_W$} | {:<INST_W$} | {:>HASH_W$} | {:>DIFF_W$} | {:>SPM_W$} | {:<TRND_W$} | {:>ACC_W$} | {:>BLK_W$} | {:>TIME_W$} |",
                    "TOTAL",
                    "ALL",
                    format_hashrate(total_rate),
                    "-",
                    total_spm_tgt,
                    "-",
                    format!("{}/{}/{}", total_shares, total_stales, total_invalids),
                    total_blocks,
                    format!("{:.1}m", total_uptime_mins)
                ));

                out.push(top);
                info!("{}", out.join("\n"));
            }
        });
    }

    pub fn start_vardiff_thread(&self, _expected_share_rate: u32, _log_stats: bool, _clamp: bool) {
        let stats = Arc::clone(&self.stats);
        let prefix = self.log_prefix();
        let expected_share_rate = _expected_share_rate;
        let log_stats = _log_stats;
        let clamp = _clamp;

        tokio::spawn(async move {
            let expected_spm = expected_share_rate.max(1) as f64;
            let mut interval = tokio::time::interval(Duration::from_secs(VAR_DIFF_THREAD_SLEEP));

            if log_stats {
                tracing::info!(
                    "{} VarDiff enabled (target={} shares/min, tick={}s, pow2_clamp={})",
                    prefix,
                    expected_spm,
                    VAR_DIFF_THREAD_SLEEP,
                    clamp
                );
            } else {
                tracing::debug!(
                    "{} VarDiff thread started (target={} shares/min, tick={}s, pow2_clamp={})",
                    prefix,
                    expected_spm,
                    VAR_DIFF_THREAD_SLEEP,
                    clamp
                );
            }

            loop {
                interval.tick().await;

                let mut stats_map = stats.lock();
                let now = Instant::now();

                for (_worker_id, v) in stats_map.iter_mut() {
                    let start_opt = *v.var_diff_start_time.lock();
                    let Some(start) = start_opt else { continue };

                    let elapsed = now.duration_since(start).as_secs_f64().max(0.0);
                    let shares = *v.var_diff_shares_found.lock() as f64;
                    let current = *v.min_diff.lock();
                    let next_opt = vardiff_compute_next_diff(current, shares, elapsed, expected_spm, clamp);
                    let Some(next) = next_opt else { continue };

                    *v.min_diff.lock() = next;
                    *v.var_diff_start_time.lock() = Some(now);
                    *v.var_diff_shares_found.lock() = 0;
                    *v.var_diff_window.lock() = 0;

                    if log_stats {
                        let observed_spm = if elapsed > 0.0 { (shares / elapsed) * 60.0 } else { 0.0 };
                        tracing::info!(
                            "{} VarDiff: {:.1} spm (target {:.1}), shares={}, window={:.0}s, diff {:.0} -> {:.0}",
                            prefix,
                            observed_spm,
                            expected_spm,
                            shares as i64,
                            elapsed,
                            current,
                            next
                        );
                    }
                }
            }
        });
    }
}

fn format_hashrate(ghs: f64) -> String {
    if ghs < 1.0 {
        format!("{:.2}MH/s", ghs * 1000.0)
    } else if ghs < 1000.0 {
        format!("{:.2}GH/s", ghs)
    } else {
        format!("{:.2}TH/s", ghs / 1000.0)
    }
}

// Trait for kaspa API operations
#[async_trait::async_trait]
pub trait KaspaApiTrait: Send + Sync {
    async fn get_block_template(
        &self,
        wallet_addr: &str,
        remote_app: &str,
        canxium_addr: &str,
    ) -> Result<Block, Box<dyn std::error::Error + Send + Sync>>;

    async fn submit_block(
        &self,
        block: Block,
    ) -> Result<kaspa_rpc_core::SubmitBlockResponse, Box<dyn std::error::Error + Send + Sync>>;

    /// Get balances by addresses (for Prometheus metrics)
    /// Get balances for addresses
    async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<(String, u64)>, Box<dyn std::error::Error + Send + Sync>>;
}

pub struct WorkerContext<'a> {
    pub worker_name: &'a str,
    pub wallet_addr: &'a str,
}
