use crate::{
    constants::*,
    hasher::{calculate_target, serialize_block_header},
    job_formatter::format_job_params,
    jsonrpc_event::JsonRpcEvent,
    miner_detection::{is_bitmain, is_iceriver},
    mining_state::{GetMiningState, Job, MiningState},
    notification_sender::send_mining_notification,
    prom::*,
    share_handler::{KaspaApiTrait, ShareHandler},
    stratum_context::StratumContext,
};
use num_bigint::BigUint;
use num_traits::Zero;
use parking_lot::Mutex;
use regex::Regex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, warn};

static BIG_JOB_REGEX: once_cell::sync::Lazy<Regex> =
    once_cell::sync::Lazy::new(|| Regex::new(r".*(BzMiner|IceRiverMiner).*").unwrap());

pub struct ClientHandler {
    clients: Arc<Mutex<HashMap<i32, Arc<StratumContext>>>>,
    client_counter: AtomicI32,
    min_share_diff: f64,
    _extranonce_size: i8,       // Kept for backward compatibility, but now auto-detected per client (unused)
    _max_extranonce: i32,       // Kept for backward compatibility (unused)
    next_extranonce: AtomicI32, // Used for extranonce_size=2 (IceRiver/BzMiner/Goldshell)
    last_template_time: Arc<Mutex<Instant>>,
    last_balance_check: Arc<Mutex<Instant>>,
    share_handler: Arc<ShareHandler>,
    instance_id: String, // Instance identifier for logging
}

impl ClientHandler {
    pub fn new(share_handler: Arc<ShareHandler>, min_share_diff: f64, extranonce_size: i8, instance_id: String) -> Self {
        let max_extranonce = if extranonce_size > 0 { (2_f64.powi(8 * extranonce_size.min(3) as i32) - 1.0) as i32 } else { 0 };

        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            client_counter: AtomicI32::new(0),
            min_share_diff,
            _extranonce_size: extranonce_size,
            _max_extranonce: max_extranonce,
            next_extranonce: AtomicI32::new(0),
            last_template_time: Arc::new(Mutex::new(Instant::now())),
            last_balance_check: Arc::new(Mutex::new(Instant::now())),
            share_handler,
            instance_id,
        }
    }

    pub fn on_connect(&self, ctx: Arc<StratumContext>) {
        let idx = self.client_counter.fetch_add(1, Ordering::Relaxed);

        // Don't assign extranonce here - will be assigned in handle_subscribe based on detected miner type
        // Leave extranonce empty initially
        *ctx.extranonce.lock() = String::new();

        ctx.set_id(idx);
        self.clients.lock().insert(idx, Arc::clone(&ctx));

        tracing::debug!(
            "{} [CONNECTION] Client {} connected (ID: {}), extranonce will be assigned after miner type detection",
            self.instance_id,
            ctx.remote_addr,
            idx
        );

        // Create stats after delay (give time for authorize)
        let share_handler = Arc::clone(&self.share_handler);
        let ctx_clone = Arc::clone(&ctx);
        tokio::spawn(async move {
            tokio::time::sleep(STATS_CREATION_DELAY).await;
            share_handler.get_create_stats(&ctx_clone);
        });
    }

    /// Assign extranonce to a client based on detected miner type
    /// Called from handle_subscribe after miner type is detected
    pub fn assign_extranonce_for_miner(&self, ctx: &StratumContext, remote_app: &str) {
        use std::sync::atomic::Ordering;

        // Detect miner type and determine required extranonce size
        // Bitmain (GodMiner) requires extranonce_size = 0 (no extranonce)
        // IceRiver, BzMiner, Goldshell require extranonce_size = 2
        let is_bitmain_flag = is_bitmain(remote_app);

        let required_extranonce_size = if is_bitmain_flag { EXTRANONCE_SIZE_BITMAIN } else { EXTRANONCE_SIZE_NON_BITMAIN };

        let extranonce = if required_extranonce_size > 0 {
            // Calculate max extranonce for size 2
            let max_extranonce = MAX_EXTRANONCE_VALUE; // 2 bytes = 16 bits = 65535

            let next = self.next_extranonce.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |val| {
                if val < max_extranonce {
                    Some(val + 1)
                } else {
                    Some(0)
                }
            });

            if next.is_err() || next.unwrap() >= max_extranonce {
                warn!("wrapped extranonce! new clients may be duplicating work...");
            }

            let extranonce_val = next.unwrap_or(0);
            let extranonce_str = format!("{:0width$x}", extranonce_val, width = (required_extranonce_size * 2) as usize);
            tracing::debug!(
                "[AUTO-EXTRANONCE] Assigned extranonce '{}' (value: {}, size: {} bytes) to {} miner '{}'",
                extranonce_str,
                extranonce_val,
                required_extranonce_size,
                if is_bitmain_flag { "Bitmain" } else { "IceRiver/BzMiner/Goldshell" },
                remote_app
            );
            extranonce_str
        } else {
            tracing::debug!("[AUTO-EXTRANONCE] Assigned empty extranonce (size: 0 bytes) to Bitmain miner '{}'", remote_app);
            String::new()
        };

        *ctx.extranonce.lock() = extranonce.clone();

        tracing::debug!(
            "[AUTO-EXTRANONCE] Client {} extranonce set to '{}' (detected miner: '{}', type: {})",
            ctx.remote_addr,
            extranonce,
            remote_app,
            if is_bitmain_flag { "Bitmain" } else { "IceRiver/BzMiner/Goldshell" }
        );
    }

    pub fn on_disconnect(&self, ctx: &StratumContext) {
        ctx.disconnect();
        let mut clients = self.clients.lock();
        if let Some(id) = ctx.id() {
            tracing::debug!("removing client {}", id);
            clients.remove(&id);
            tracing::debug!("removed client {}", id);
        }
        let wallet_addr = ctx.wallet_addr.lock().clone();
        let worker_name = ctx.worker_name.lock().clone();
        record_disconnect(&crate::prom::WorkerContext {
            worker_name: worker_name.clone(),
            miner: String::new(),
            wallet: wallet_addr.clone(),
            ip: format!("{}:{}", ctx.remote_addr(), ctx.remote_port()),
        });
    }

    /// Send an immediate job to a specific client (for use after authorization)
    /// This ensures IceRiver and other ASICs get a job immediately, not waiting for polling
    pub async fn send_immediate_job_to_client<T: KaspaApiTrait + Send + Sync + ?Sized + 'static>(
        &self,
        client: Arc<StratumContext>,
        kaspa_api: Arc<T>,
    ) {
        // Check if client has wallet address
        let _wallet_addr_str = {
            let wallet_addr = client.wallet_addr.lock();
            if wallet_addr.is_empty() {
                tracing::debug!("send_immediate_job: client {} has no wallet address yet, skipping", client.remote_addr);
                return;
            }
            wallet_addr.clone()
        };

        if !client.connected() {
            tracing::debug!("send_immediate_job: client {} not connected, skipping", client.remote_addr);
            return;
        }

        let client_clone = Arc::clone(&client);
        let kaspa_api_clone = Arc::clone(&kaspa_api);
        let share_handler = Arc::clone(&self.share_handler);
        let min_diff = self.min_share_diff;

        tokio::spawn(async move {
            // Get per-client mining state from context
            let state = GetMiningState(&client_clone);

            // Get client info
            let (wallet_addr, remote_app, canxium_addr) = {
                let wallet = client_clone.wallet_addr.lock().clone();
                let app = client_clone.remote_app.lock().clone();
                let canx = client_clone.canxium_addr.lock().clone();
                (wallet, app, canx)
            };

            tracing::debug!(
                "send_immediate_job: fetching block template for client {} (wallet: {})",
                client_clone.remote_addr,
                wallet_addr
            );

            // Get block template
            let template_result = kaspa_api_clone.get_block_template(&wallet_addr, &remote_app, &canxium_addr).await;

            let block = match template_result {
                Ok(block) => {
                    tracing::debug!("send_immediate_job: successfully fetched block template for client {}", client_clone.remote_addr);

                    // === LOG NEW BLOCK TEMPLATE HEADER === (moved to debug level)
                    tracing::debug!("=== NEW BLOCK TEMPLATE RECEIVED ===");
                    tracing::debug!("  blue_score: {}", block.header.blue_score);
                    tracing::debug!("  bits: {} (0x{:08x})", block.header.bits, block.header.bits);
                    tracing::debug!("  timestamp: {}", block.header.timestamp);
                    tracing::debug!("  version: {}", block.header.version);
                    tracing::debug!("  daa_score: {}", block.header.daa_score);

                    // Track and log what changed from previous header
                    if let Some(old_header) = state.get_last_header() {
                        tracing::debug!("=== HEADER CHANGES ===");
                        tracing::debug!("  blue_score_changed: {}", old_header.blue_score != block.header.blue_score);
                        tracing::debug!("    old: {}, new: {}", old_header.blue_score, block.header.blue_score);
                        tracing::debug!("  bits_changed: {}", old_header.bits != block.header.bits);
                        tracing::debug!("    old: 0x{:08x}, new: 0x{:08x}", old_header.bits, block.header.bits);
                        tracing::debug!("  timestamp_changed: {}", old_header.timestamp != block.header.timestamp);
                        tracing::debug!("    delta: {} ms", block.header.timestamp - old_header.timestamp);
                        tracing::debug!("  daa_score_changed: {}", old_header.daa_score != block.header.daa_score);
                        tracing::debug!("  version_changed: {}", old_header.version != block.header.version);
                    } else {
                        tracing::debug!("=== FIRST HEADER === (no previous header to compare)");
                    }

                    // Store this header for next comparison
                    state.set_last_header((*block.header).clone());

                    block
                }
                Err(e) => {
                    if e.to_string().contains("Could not decode address") {
                        record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::InvalidAddressFmt.as_str());
                        error!("send_immediate_job: failed fetching block template, malformed address: {}", e);
                        client_clone.disconnect();
                    } else {
                        record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::FailedBlockFetch.as_str());
                        error!("send_immediate_job: failed fetching block template: {}", e);
                    }
                    return;
                }
            };

            // Calculate target
            let big_diff = calculate_target(block.header.bits as u64);
            state.set_big_diff(big_diff);

            // Serialize header - now returns Hash type directly
            // The "Odd number of digits" error typically indicates a malformed hex string
            // in one of the hash fields. This can happen if the block data from the node
            // contains an invalid hash representation.
            let pre_pow_hash = match serialize_block_header(&block) {
                Ok(h) => h,
                Err(e) => {
                    let error_msg = e.to_string();
                    record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::BadDataFromMiner.as_str());
                    error!("send_immediate_job: failed to serialize block header: {}", error_msg);

                    // Log block header details for debugging
                    tracing::debug!("Block header version: {}", block.header.version);
                    tracing::debug!("Block header timestamp: {}", block.header.timestamp);
                    tracing::debug!("Block header bits: {}", block.header.bits);

                    // Skip this block and continue - the next block template should work
                    return;
                }
            };

            // Create Job struct with both block and pre_pow_hash
            let job = Job { block: block.clone(), pre_pow_hash };

            // Add job
            let job_id = state.add_job(job);
            let counter_after = state.current_job_counter();
            let stored_ids = state.get_stored_job_ids();
            tracing::debug!(
                "[JOB CREATION] send_immediate_job: created job ID {} for client {} (counter: {}, stored IDs: {:?})",
                job_id,
                client_clone.remote_addr,
                counter_after,
                stored_ids
            );

            // Initialize state if first time
            if !state.is_initialized() {
                state.set_initialized(true);
                let use_big_job = BIG_JOB_REGEX.is_match(&remote_app);
                state.set_use_big_job(use_big_job);

                // Initialize stratum diff
                use crate::hasher::KaspaDiff;
                let mut stratum_diff = KaspaDiff::new();
                let remote_app_clone = remote_app.clone();
                stratum_diff.set_diff_value_for_miner(min_diff, &remote_app_clone);
                state.set_stratum_diff(stratum_diff);
                let target = state.stratum_diff().map(|d| d.target_value.clone()).unwrap_or_else(BigUint::zero);
                let target_bytes = target.to_bytes_be();
                tracing::debug!(
                    "send_immediate_job: Initialized MiningState with difficulty: {}, target: {:x} ({} bytes, {} bits)",
                    min_diff,
                    target,
                    target_bytes.len(),
                    target_bytes.len() * 8
                );
            }

            // CRITICAL: Always send difficulty to each client (IceRiver expects this on every connection)
            // Even if state is already initialized, we need to send difficulty to this specific client
            tracing::debug!("[DIFFICULTY] ===== SENDING DIFFICULTY TO {} =====", client_clone.remote_addr);
            tracing::debug!("[DIFFICULTY] Difficulty value: {}", min_diff);
            send_client_diff(&client_clone, &state, min_diff);
            share_handler.set_client_vardiff(&client_clone, min_diff);
            tracing::debug!("[DIFFICULTY] ===== DIFFICULTY SENT TO {} =====", client_clone.remote_addr);

            // Small delay to ensure difficulty is sent before job
            tokio::time::sleep(IMMEDIATE_JOB_DELAY).await;

            // Build job params - check if this is an IceRiver or Bitmain miner
            let is_iceriver_flag = is_iceriver(&remote_app);
            let is_bitmain_flag = is_bitmain(&remote_app);

            tracing::debug!("[JOB] ===== BUILDING JOB FOR {} =====", client_clone.remote_addr);
            tracing::debug!("[JOB] Job ID: {}", job_id);
            tracing::debug!("[JOB] Remote app: '{}'", remote_app);
            tracing::debug!(
                "[JOB] Is IceRiver: {}, Is Bitmain: {}, use_big_job: {}",
                is_iceriver_flag,
                is_bitmain_flag,
                state.use_big_job()
            );
            tracing::debug!("[JOB] Pre-PoW hash: {}", pre_pow_hash);
            tracing::debug!("[JOB] Block timestamp: {}", block.header.timestamp);

            // Format job params using helper function (preserves exact formatting logic)
            let job_params = format_job_params(job_id, &pre_pow_hash, block.header.timestamp, &remote_app, state.use_big_job());
            tracing::debug!("[JOB] Job params initialized with job_id: {}", job_id);
            tracing::debug!("[JOB] Job params count: {}", job_params.len());

            tracing::debug!("[JOB] ===== SENDING MINING.NOTIFY TO {} =====", client_clone.remote_addr);
            tracing::debug!("[JOB] Method: mining.notify");

            // Also log the raw job data for verification (for string formats)
            if let Some(serde_json::Value::String(job_data)) = job_params.get(1) {
                tracing::debug!("[JOB] Job data string length: {} chars", job_data.len());
                if job_data.len() == 80 {
                    let hash_part = &job_data[..64];
                    let timestamp_part = &job_data[64..];
                    tracing::debug!("[JOB] Hash part (64 hex): {}", hash_part);
                    tracing::debug!("[JOB] Timestamp part (16 hex): {}", timestamp_part);
                    tracing::debug!("[JOB] Full job_data: {}", job_data);
                } else {
                    let expected_for = if is_iceriver_flag {
                        "IceRiver"
                    } else if is_bitmain_flag {
                        "Bitmain"
                    } else {
                        "standard"
                    };
                    tracing::warn!("[JOB] WARNING - job_data length is {} (expected 80 for {})", job_data.len(), expected_for);
                }
            }

            let format_name = if is_iceriver_flag {
                "IceRiver"
            } else if state.use_big_job() {
                "BzMiner"
            } else {
                "Legacy"
            };
            tracing::debug!(
                "[JOB] Sending job ID {} to {} (format: {}, params: {})",
                job_id,
                client_clone.remote_addr,
                format_name,
                job_params.len()
            );

            // Send notification with appropriate format (minimal for IceRiver, standard for others)
            let send_result = send_mining_notification(&client_clone, "mining.notify", job_params.clone(), job_id, &remote_app).await;

            if let Err(e) = send_result {
                if e.to_string().contains("disconnected") {
                    record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::Disconnected.as_str());
                    tracing::error!("[JOB] ERROR: Failed to send job {} - client disconnected", job_id);
                } else {
                    record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::FailedSendWork.as_str());
                    error!("[JOB] ERROR: Failed sending work packet {}: {}", job_id, e);
                }
                tracing::debug!("[JOB] ===== JOB SEND FAILED FOR {} =====", client_clone.remote_addr);
            } else {
                let wallet_addr_str = wallet_addr.clone();
                let worker_name = client_clone.worker_name.lock().clone();
                record_new_job(&crate::prom::WorkerContext {
                    worker_name: worker_name.clone(),
                    miner: String::new(),
                    wallet: wallet_addr_str.clone(),
                    ip: format!("{}:{}", client_clone.remote_addr(), client_clone.remote_port()),
                });
                tracing::debug!("[JOB] Successfully sent job ID {} to client {}", job_id, client_clone.remote_addr);
                tracing::debug!("[JOB] ===== JOB SENT SUCCESSFULLY TO {} =====", client_clone.remote_addr);
            }
        });
    }

    pub async fn new_block_available<T: KaspaApiTrait + Send + Sync + 'static>(&self, kaspa_api: Arc<T>) {
        // Rate limit templates (minimum time between sends)
        {
            let mut last_time = self.last_template_time.lock();
            if last_time.elapsed() < BLOCK_TEMPLATE_RATE_LIMIT {
                return;
            }
            *last_time = Instant::now();
        }

        let clients = {
            let clients_guard = self.clients.lock();
            clients_guard.values().cloned().collect::<Vec<_>>()
        };

        // Collect addresses for balance checking
        let mut addresses: Vec<String> = Vec::new();
        let mut client_count = 0;

        for client in clients {
            if !client.connected() {
                continue;
            }

            if client_count > 0 {
                // Small delay between sending jobs to different clients
                tokio::time::sleep(Duration::from_micros(500)).await;
            }
            client_count += 1;

            // Collect wallet address for balance checking
            {
                let wallet_addr = client.wallet_addr.lock();
                if !wallet_addr.is_empty() {
                    addresses.push(wallet_addr.clone());
                }
            }

            let client_clone = Arc::clone(&client);
            let kaspa_api_clone = Arc::clone(&kaspa_api);
            let share_handler = Arc::clone(&self.share_handler);
            let min_diff = self.min_share_diff;

            tokio::spawn(async move {
                // Get per-client mining state from context
                let state = GetMiningState(&client_clone);

                // Check if client has wallet address
                let wallet_addr_str = {
                    let wallet_addr = client_clone.wallet_addr.lock();
                    if wallet_addr.is_empty() {
                        let connect_time = state.connect_time();
                        if let Ok(elapsed) = connect_time.elapsed() {
                            if elapsed > CLIENT_TIMEOUT {
                                warn!("client misconfigured, no miner address specified - disconnecting");
                                let wallet_str = wallet_addr.clone();
                                record_worker_error(&wallet_str, crate::errors::ErrorShortCode::NoMinerAddress.as_str());
                                drop(wallet_addr); // Drop before disconnect
                                client_clone.disconnect();
                            }
                        }
                        tracing::debug!(
                            "new_block_available: client {} has no wallet address yet, skipping",
                            client_clone.remote_addr
                        );
                        return;
                    }
                    wallet_addr.clone()
                };

                tracing::debug!(
                    "new_block_available: fetching block template for client {} (wallet: {})",
                    client_clone.remote_addr,
                    wallet_addr_str
                );

                // Get block template
                let (wallet_addr, remote_app, canxium_addr) = {
                    let wallet = client_clone.wallet_addr.lock().clone();
                    let app = client_clone.remote_app.lock().clone();
                    let canx = client_clone.canxium_addr.lock().clone();
                    (wallet, app, canx)
                };

                let template_result = kaspa_api_clone.get_block_template(&wallet_addr, &remote_app, &canxium_addr).await;

                let block = match template_result {
                    Ok(block) => {
                        tracing::debug!(
                            "new_block_available: successfully fetched block template for client {}",
                            client_clone.remote_addr
                        );
                        block
                    }
                    Err(e) => {
                        if e.to_string().contains("Could not decode address") {
                            record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::InvalidAddressFmt.as_str());
                            error!("failed fetching new block template from kaspa, malformed address: {}", e);
                            client_clone.disconnect();
                        } else {
                            record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::FailedBlockFetch.as_str());
                            error!("failed fetching new block template from kaspa: {}", e);
                        }
                        return;
                    }
                };

                // Calculate target
                let big_diff = calculate_target(block.header.bits as u64);
                state.set_big_diff(big_diff);

                // Serialize header - now returns Hash type directly
                // The "Odd number of digits" error typically indicates a malformed hex string
                // in one of the hash fields. This can happen if the block data from the node
                // contains an invalid hash representation.
                let pre_pow_hash = match serialize_block_header(&block) {
                    Ok(h) => h,
                    Err(e) => {
                        let error_msg = e.to_string();
                        record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::BadDataFromMiner.as_str());
                        error!("failed to serialize block header: {}", error_msg);

                        // Log block header details for debugging
                        tracing::debug!("Block header version: {}", block.header.version);
                        tracing::debug!("Block header timestamp: {}", block.header.timestamp);
                        tracing::debug!("Block header bits: {}", block.header.bits);
                        tracing::debug!("Block header daa_score: {}", block.header.daa_score);
                        tracing::debug!("Block header blue_score: {}", block.header.blue_score);
                        tracing::debug!(
                            "Block header parents_by_level expanded_len: {}",
                            block.header.parents_by_level.expanded_len()
                        );

                        // Skip this block and continue - the next block template should work
                        return;
                    }
                };

                // Create Job struct with both block and pre_pow_hash
                let job = Job { block: block.clone(), pre_pow_hash };

                // Add job
                let job_id = state.add_job(job);
                let counter_after = state.current_job_counter();
                let stored_ids = state.get_stored_job_ids();
                tracing::debug!(
                    "[JOB CREATION] new_block_available: created job ID {} for client {} (counter: {}, stored IDs: {:?})",
                    job_id,
                    client_clone.remote_addr,
                    counter_after,
                    stored_ids
                );

                // Initialize state if first time (per-client state initialization)
                if !state.is_initialized() {
                    state.set_initialized(true);
                    let use_big_job = BIG_JOB_REGEX.is_match(&remote_app);
                    state.set_use_big_job(use_big_job);

                    // Send initial difficulty
                    use crate::hasher::KaspaDiff;
                    let mut stratum_diff = KaspaDiff::new();
                    // Use miner-specific calculation (IceRiver uses different formula)
                    let remote_app = client_clone.remote_app.lock().clone();
                    stratum_diff.set_diff_value_for_miner(min_diff, &remote_app);
                    state.set_stratum_diff(stratum_diff);
                    let target = state.stratum_diff().map(|d| d.target_value.clone()).unwrap_or_else(BigUint::zero);
                    let target_bytes = target.to_bytes_be();
                    tracing::debug!(
                        "Initialized per-client MiningState with difficulty: {}, target: {:x} ({} bytes, {} bits)",
                        min_diff,
                        target,
                        target_bytes.len(),
                        target_bytes.len() * 8
                    );
                    send_client_diff(&client_clone, &state, min_diff);
                    share_handler.set_client_vardiff(&client_clone, min_diff);
                } else {
                    // Check for vardiff update
                    let var_diff = share_handler.get_client_vardiff(&client_clone);
                    if let Some(mut stratum_diff) = state.stratum_diff() {
                        let current_diff = stratum_diff.diff_value;
                        if var_diff != current_diff && var_diff != 0.0 {
                            tracing::debug!("changing diff from {} to {}", current_diff, var_diff);
                            // Use miner-specific calculation (IceRiver uses different formula)
                            let remote_app = client_clone.remote_app.lock().clone();
                            stratum_diff.set_diff_value_for_miner(var_diff, &remote_app);
                            state.set_stratum_diff(stratum_diff);
                            send_client_diff(&client_clone, &state, var_diff);
                            // Reset vardiff window once the new difficulty has been applied/sent
                            share_handler.set_client_vardiff(&client_clone, var_diff);
                        }
                    }
                }

                // Build job params
                // Check if this is an IceRiver or Bitmain miner - they need single hex string format
                let remote_app = client_clone.remote_app.lock().clone();
                let is_iceriver_flag = is_iceriver(&remote_app);
                let is_bitmain_flag = is_bitmain(&remote_app);

                tracing::debug!(
                    "[JOB] new_block_available: client {}, is_iceriver: {}, is_bitmain: {}, use_big_job: {}",
                    client_clone.remote_addr,
                    is_iceriver_flag,
                    is_bitmain_flag,
                    state.use_big_job()
                );

                // Format job params using helper function (preserves exact formatting logic)
                let job_params = format_job_params(job_id, &pre_pow_hash, block.header.timestamp, &remote_app, state.use_big_job());

                // IceRiver expects minimal notification format (method + params only, no id or jsonrpc)
                // This matches StratumNotification format used by the stratum crate
                // NOTE: We reuse the flags already computed above for consistency
                tracing::debug!(
                    "new_block_available: sending job ID {} to client {} (params count: {}, is_iceriver: {}, is_bitmain: {})",
                    job_id,
                    client_clone.remote_addr,
                    job_params.len(),
                    is_iceriver_flag,
                    is_bitmain_flag
                );

                // Send notification with appropriate format (minimal for IceRiver, standard for others)
                let send_result =
                    send_mining_notification(&client_clone, "mining.notify", job_params.clone(), job_id, &remote_app).await;

                if let Err(e) = send_result {
                    if e.to_string().contains("disconnected") {
                        record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::Disconnected.as_str());
                        tracing::warn!("new_block_available: failed to send job {} - client disconnected", job_id);
                    } else {
                        record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::FailedSendWork.as_str());
                        error!("failed sending work packet {}: {}", job_id, e);
                        tracing::error!(
                            "new_block_available: failed to send job {} to client {}: {}",
                            job_id,
                            client_clone.remote_addr,
                            e
                        );
                    }
                } else {
                    let wallet_addr_str = wallet_addr.clone();
                    let worker_name = client_clone.worker_name.lock().clone();
                    record_new_job(&crate::prom::WorkerContext {
                        worker_name: worker_name.clone(),
                        miner: String::new(),
                        wallet: wallet_addr_str.clone(),
                        ip: format!("{}:{}", client_clone.remote_addr(), client_clone.remote_port()),
                    });
                    tracing::debug!("new_block_available: successfully sent job ID {} to client {}", job_id, client_clone.remote_addr);
                }
            });
        }

        // Check balances periodically
        {
            let mut last_check = self.last_balance_check.lock();
            if last_check.elapsed() > BALANCE_DELAY && !addresses.is_empty() {
                *last_check = Instant::now();
                drop(last_check);

                // Fetch balances via kaspa_api
                let addresses_clone = addresses.clone();
                let kaspa_api_clone = Arc::clone(&kaspa_api);
                tokio::spawn(async move {
                    match kaspa_api_clone.get_balances_by_addresses(&addresses_clone).await {
                        Ok(balances) => {
                            // Record balances
                            crate::prom::record_balances(&balances);
                        }
                        Err(e) => {
                            warn!("failed to get balances from kaspa, prom stats will be out of date: {}", e);
                        }
                    }
                });
            }
        }
    }
}

// Send difficulty update to client
fn send_client_diff(client: &StratumContext, _state: &MiningState, diff: f64) {
    tracing::debug!("[DIFFICULTY] Building difficulty message for {}", client.remote_addr);

    // Send diffValue directly as a number
    let diff_value =
        serde_json::Value::Number(serde_json::Number::from_f64(diff).unwrap_or_else(|| serde_json::Number::from(diff as u64)));

    let client_clone = client.clone();
    tokio::spawn(async move {
        tracing::debug!("[DIFFICULTY] Sending mining.set_difficulty to {}", client_clone.remote_addr);

        // Always use standard JSON-RPC format
        let diff_event = JsonRpcEvent {
            jsonrpc: "2.0".to_string(),
            method: "mining.set_difficulty".to_string(),
            id: None, // Go doesn't send ID for set_difficulty
            params: vec![diff_value],
        };

        let send_result = client_clone.send(diff_event).await;

        if let Err(e) = send_result {
            let wallet_addr = client_clone.wallet_addr.lock().clone();
            record_worker_error(&wallet_addr, crate::errors::ErrorShortCode::FailedSetDiff.as_str());
            error!("[DIFFICULTY] ERROR: Failed sending difficulty: {}", e);
            return;
        }
        tracing::debug!("[DIFFICULTY] Successfully sent difficulty {} to {}", diff, client_clone.remote_addr);
    });
}
