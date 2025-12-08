//! Stratum client connection handler

use crate::error::StratumError;
use crate::protocol::{
    create_error_response, create_notification, create_success_response, parse_message, MiningAuthorizeParams, MiningSubmitParams,
    MiningSubscribeParams, StratumNotification, StratumRequest, StratumResponse,
};
use crate::BlockSubmission;
use kaspa_addresses::Address;
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// Miner encoding type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    BigHeader, // Standard EthereumStratum format
    Bitmain,   // Bitmain/GodMiner format
}

/// Vardiff tracking state
#[derive(Debug, Clone)]
pub struct VardiffState {
    pub last_share: u64,             // Timestamp of last share (milliseconds)
    pub last_difficulty_change: u64, // Timestamp of last difficulty change (milliseconds)
    pub current_difficulty: f64,     // Current vardiff difficulty
    pub initialized: bool,           // Whether vardiff has been initialized
    pub share_count: u64,            // Total shares submitted
}

impl Default for VardiffState {
    fn default() -> Self {
        Self { last_share: 0, last_difficulty_change: 0, current_difficulty: 1.0, initialized: false, share_count: 0 }
    }
}

/// Miner connection state
#[derive(Debug, Clone)]
pub struct MinerState {
    pub agent: String,
    pub difficulty: f64,
    pub workers: HashSet<(String, String)>, // (address, worker_name)
    pub extra_nonce: String,
    pub encoding: Encoding,
    pub subscribed: bool,
    pub authorized: bool,
    pub connected_at: u64,
    pub message_count: u64,
    pub vardiff: Option<VardiffState>, // Vardiff tracking (only used when enabled)
}

impl Default for MinerState {
    fn default() -> Self {
        Self {
            agent: "Unknown".to_string(),
            difficulty: 1.0,
            workers: HashSet::new(),
            extra_nonce: generate_extra_nonce(),
            encoding: Encoding::BigHeader,
            subscribed: false,
            authorized: false,
            connected_at: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
            message_count: 0,
            vardiff: None,
        }
    }
}

fn generate_extra_nonce() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!("{:04x}", rng.gen::<u16>())
}

// MiningJob is now defined in server.rs

/// Client handler for a single miner connection
pub struct StratumClient {
    stream: TcpStream,
    state: Arc<RwLock<MinerState>>,
    jobs: Arc<RwLock<HashMap<String, String>>>, // job_id -> header_hash mapping
    current_job: Arc<RwLock<Option<String>>>,   // current job_id
    tx: mpsc::UnboundedSender<StratumNotification>,
    rx: mpsc::UnboundedReceiver<StratumNotification>,
    submission_tx: Option<mpsc::UnboundedSender<BlockSubmission>>,
    client_addr: SocketAddr,
}

impl StratumClient {
    pub fn new(stream: TcpStream, addr: SocketAddr) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            stream,
            state: Arc::new(RwLock::new(MinerState::default())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
            current_job: Arc::new(RwLock::new(None)),
            tx,
            rx,
            submission_tx: None,
            client_addr: addr,
        }
    }

    pub fn set_submission_sender(&mut self, tx: mpsc::UnboundedSender<BlockSubmission>) {
        self.submission_tx = Some(tx);
    }

    pub fn get_state(&self) -> Arc<RwLock<MinerState>> {
        self.state.clone()
    }

    pub fn get_notification_sender(&self) -> mpsc::UnboundedSender<StratumNotification> {
        self.tx.clone()
    }

    async fn handle_subscribe_static(
        state: &Arc<RwLock<MinerState>>,
        request: StratumRequest,
        client_addr: SocketAddr,
        default_difficulty: f64,
    ) -> Result<StratumResponse, StratumError> {
        let params = MiningSubscribeParams::try_from(&request).map_err(StratumError::Protocol)?;

        let mut state_guard = state.write();
        state_guard.subscribed = true;
        state_guard.agent = params.user_agent.unwrap_or_else(|| "Unknown".to_string());

        // Detect encoding type based on user agent
        let agent_lower = state_guard.agent.to_lowercase();
        if agent_lower.contains("bitmain") || agent_lower.contains("godminer") || agent_lower.contains("antminer") {
            state_guard.encoding = Encoding::Bitmain;
        }

        let extra_nonce = state_guard.extra_nonce.clone();
        let encoding = state_guard.encoding;
        let agent = state_guard.agent.clone();
        // Initialize difficulty from default (will be updated by notifications)
        state_guard.difficulty = default_difficulty;
        drop(state_guard);

        log::info!("Stratum client subscribed from {} - Agent: {}, Encoding: {:?}", client_addr, agent, encoding);

        // Build response based on encoding
        let result = if encoding == Encoding::Bitmain {
            // Bitmain format: [null, extranonce, size]
            json!([null, extra_nonce, 8 - (extra_nonce.len() / 2)])
        } else {
            // Standard format: [true, "EthereumStratum/1.0.0"]
            json!([true, "EthereumStratum/1.0.0"])
        };

        Ok(create_success_response(request.id, result))
    }

    async fn handle_authorize_static(
        state: &Arc<RwLock<MinerState>>,
        request: StratumRequest,
        client_addr: SocketAddr,
        miner_addresses: Option<Arc<parking_lot::RwLock<std::collections::HashMap<SocketAddr, kaspa_addresses::Address>>>>,
    ) -> Result<StratumResponse, StratumError> {
        let params = MiningAuthorizeParams::try_from(&request).map_err(StratumError::Protocol)?;

        // Parse address from username (format: address.worker_name or just address)
        let (address, worker_name) = if let Some(dot_pos) = params.username.find('.') {
            let addr = params.username[..dot_pos].to_string();
            let worker = params.username[dot_pos + 1..].to_string();
            (addr, worker)
        } else {
            (params.username.clone(), "default".to_string())
        };

        // Validate address format - Address parser expects the full address string with prefix
        // Ensure address has the prefix for parsing
        let address_with_prefix = if address.starts_with("kaspa:") || address.starts_with("kaspatest:") {
            // Address already has prefix, use as-is
            address.clone()
        } else {
            // Add mainnet prefix if missing
            format!("kaspa:{}", address)
        };

        // Try to parse as Kaspa address (parser requires prefix)
        let kaspa_address = Address::try_from(address_with_prefix.as_str()).map_err(|e| {
            log::error!("Failed to parse address '{}' (original: '{}'): {:?}", address_with_prefix, address, e);
            StratumError::Address(e)
        })?;

        // Register miner address in server (for block template creation)
        if let Some(ref miner_addresses) = miner_addresses {
            miner_addresses.write().insert(client_addr, kaspa_address.clone());
            log::info!("Registered miner address {} for client {}", kaspa_address, client_addr);
        }

        // Store address without prefix for consistency
        let address_clean = address_with_prefix.trim_start_matches("kaspa:").trim_start_matches("kaspatest:").to_string();

        let mut state_guard = state.write();
        state_guard.authorized = true;
        state_guard.workers.insert((address_clean.clone(), worker_name.clone()));
        drop(state_guard);

        log::info!("Stratum client authorized from {} - Address: {}, Worker: {}", client_addr, address_clean, worker_name);

        Ok(create_success_response(request.id, json!(true)))
    }

    async fn handle_submit_static(
        state: &Arc<RwLock<MinerState>>,
        jobs: &Arc<RwLock<HashMap<String, String>>>, // job_id -> header_hash mapping (client-side)
        submission_tx: &Option<mpsc::UnboundedSender<BlockSubmission>>,
        client_addr: SocketAddr,
        request: StratumRequest,
    ) -> Result<StratumResponse, StratumError> {
        let params = MiningSubmitParams::try_from(&request).map_err(StratumError::Protocol)?;

        log::debug!("Received mining.submit from {} - Job: {}, Nonce: {}", client_addr, params.job_id, params.nonce);

        // Check subscription and authorization (drop lock before await)
        let (is_subscribed, is_authorized) = {
            let state_guard = state.read();
            (state_guard.subscribed, state_guard.authorized)
        };

        if !is_subscribed {
            log::warn!("[REJECTED] Submit rejected: client {} not subscribed", client_addr);
            return Ok(create_error_response(request.id, StratumError::NotSubscribed.code(), "Not subscribed".to_string()));
        }
        if !is_authorized {
            log::warn!("[REJECTED] Submit rejected: client {} not authorized", client_addr);
            return Ok(create_error_response(request.id, StratumError::UnauthorizedWorker.code(), "Not authorized".to_string()));
        }

        // Check if job exists (drop lock before await)
        let job_exists = {
            let jobs_read = jobs.read();
            jobs_read.contains_key(&params.job_id)
        };

        if !job_exists {
            log::warn!(
                "[REJECTED] Stale job - Stratum submit from {} - Job not found: {} (job may have expired)",
                client_addr,
                params.job_id
            );
            return Ok(create_error_response(request.id, StratumError::JobNotFound.code(), "Job not found".to_string()));
        }

        // Handle extranonce2 padding like the pool does (before parsing nonce)
        // The pool combines extranonce with work parameter if work is shorter than expected
        // Extract values before await to avoid holding lock across await
        let (extra_nonce, encoding) = {
            let state_guard = state.read();
            (state_guard.extra_nonce.clone(), state_guard.encoding)
        };

        let mut work_to_parse = params.nonce.clone();
        let mut extranonce_used = false;

        if !extra_nonce.is_empty() {
            // extranonce2Len = 16 - extranonce.length (in hex chars)
            // For Kaspa, nonce is 8 bytes = 16 hex chars
            let extra_nonce_len_hex = extra_nonce.len();
            let extranonce2_len = 16usize.saturating_sub(extra_nonce_len_hex);

            // Only combine if work is shorter than expected (like pool does)
            if work_to_parse.len() <= extranonce2_len {
                // Pad work with zeros and prepend extranonce
                let padded_work = format!("{:0>width$}", work_to_parse, width = extranonce2_len);
                work_to_parse = format!("{}{}", extra_nonce, padded_work);
                extranonce_used = true;
                log::debug!("[NONCE] Combined extranonce {} with work {} -> {}", extra_nonce, params.nonce, work_to_parse);
            } else {
                log::debug!("[NONCE] Work length {} > extranonce2_len {}, using work as-is", work_to_parse.len(), extranonce2_len);
            }
        } else {
            log::debug!("[NONCE] No extranonce, using work as-is");
        }

        // Parse nonce based on encoding (like pool does)
        let nonce = match encoding {
            Encoding::Bitmain => {
                // Bitmain sends nonce as decimal string
                let parsed = work_to_parse.parse::<u64>().map_err(|_| {
                    log::warn!("Stratum submit from {} - Invalid Bitmain nonce format: {}", client_addr, work_to_parse);
                    StratumError::Protocol("Invalid Bitmain nonce format".to_string())
                })?;
                log::debug!("[NONCE] Bitmain encoding - parsed nonce: {} (0x{:x})", parsed, parsed);
                parsed
            }
            Encoding::BigHeader => {
                // Standard format: hex string (remove 0x prefix if present)
                let hex_str = work_to_parse.trim_start_matches("0x").trim_start_matches("0X");
                let parsed = u64::from_str_radix(hex_str, 16).map_err(|_| {
                    log::warn!("Stratum submit from {} - Invalid hex nonce format: {}", client_addr, hex_str);
                    StratumError::Protocol("Invalid hex nonce format".to_string())
                })?;
                log::debug!("[NONCE] BigHeader encoding - parsed nonce: 0x{:x} (extranonce_used: {})", parsed, extranonce_used);
                parsed
            }
        };

        log::debug!("[NONCE] Final parsed nonce for job {}: 0x{:x}", params.job_id, nonce);

        // Send submission to server for processing and wait for validation result
        if let Some(ref submission_tx) = submission_tx {
            // Create a channel to receive the validation result
            let (response_tx, mut response_rx) = mpsc::unbounded_channel::<bool>();

            let submission =
                BlockSubmission { job_id: params.job_id, nonce, client_addr, response_tx: Some(response_tx), request_id: request.id };

            if let Err(e) = submission_tx.send(submission) {
                log::error!("Failed to send block submission: {}", e);
                return Ok(create_error_response(
                    request.id,
                    StratumError::BlockSubmissionFailed("Internal error".to_string()).code(),
                    "Failed to submit block".to_string(),
                ));
            }

            // Wait for validation result (with timeout to prevent hanging)
            match tokio::time::timeout(tokio::time::Duration::from_secs(5), response_rx.recv()).await {
                Ok(Some(true)) => {
                    // Share was accepted
                    Ok(create_success_response(request.id, json!(true)))
                }
                Ok(Some(false)) => {
                    // Share was rejected
                    Ok(create_error_response(
                        request.id,
                        StratumError::LowDifficultyShare.code(),
                        "Share does not meet difficulty".to_string(),
                    ))
                }
                Ok(None) => {
                    // Channel closed (shouldn't happen)
                    log::warn!("Response channel closed for submission from {}", client_addr);
                    Ok(create_error_response(
                        request.id,
                        StratumError::BlockSubmissionFailed("Validation channel closed".to_string()).code(),
                        "Validation failed".to_string(),
                    ))
                }
                Err(_) => {
                    // Timeout
                    log::warn!("Timeout waiting for validation result from {}", client_addr);
                    Ok(create_error_response(
                        request.id,
                        StratumError::BlockSubmissionFailed("Validation timeout".to_string()).code(),
                        "Validation timeout".to_string(),
                    ))
                }
            }
        } else {
            Ok(create_error_response(
                request.id,
                StratumError::BlockSubmissionFailed("No submission channel".to_string()).code(),
                "Submission channel not available".to_string(),
            ))
        }
    }

    /// Run the client handler loop
    pub async fn run(
        mut self,
        submission_tx: mpsc::UnboundedSender<BlockSubmission>,
        addr: SocketAddr,
        default_difficulty: f64,
        server_current_job: Arc<RwLock<Option<super::MiningJob>>>,
        _server_notification_tx: mpsc::UnboundedSender<StratumNotification>,
        miner_addresses: Arc<parking_lot::RwLock<std::collections::HashMap<SocketAddr, kaspa_addresses::Address>>>,
    ) -> Result<(), StratumError> {
        self.submission_tx = Some(submission_tx);
        self.client_addr = addr;

        // Extract what we need before moving self
        let state = self.state.clone();
        let jobs = self.jobs.clone();
        let current_job = self.current_job.clone();
        let submission_tx_for_handle = self.submission_tx.clone();
        let client_addr_for_handle = self.client_addr;
        let default_difficulty_for_handle = default_difficulty;
        let miner_addresses_for_handle = miner_addresses.clone();

        let (read, write) = tokio::io::split(self.stream);
        let mut reader = BufReader::new(read);
        let mut line = String::new();

        // Create a channel for sending responses/notifications
        let (response_tx, mut response_rx) = mpsc::unbounded_channel::<String>();

        // Spawn task to handle notifications and responses
        let write_handle = {
            let mut write_clone = write;
            tokio::spawn(async move {
                while let Some(message) = response_rx.recv().await {
                    if let Err(e) = write_clone.write_all(message.as_bytes()).await {
                        // Client disconnected - this is normal, don't log as error
                        // Error 10053 on Windows means "connection aborted by peer"
                        let error_msg = e.to_string();
                        if error_msg.contains("10053") || error_msg.contains("Broken pipe") || error_msg.contains("Connection reset") {
                            log::debug!("Client disconnected while sending message: {}", e);
                        } else {
                            log::warn!("Failed to write message to client: {}", e);
                        }
                        break;
                    }
                    // Flush after each message to ensure it's sent immediately
                    if let Err(e) = write_clone.flush().await {
                        log::debug!("Failed to flush write buffer (client may have disconnected): {}", e);
                        break;
                    }
                }
            })
        };

        // Spawn task to forward notifications to response channel and handle job notifications
        let mut notification_rx = self.rx;
        let response_tx_notify = response_tx.clone();
        let jobs_for_notify = jobs.clone();
        let current_job_for_notify = current_job.clone();
        let state_for_notify = state.clone();
        tokio::spawn(async move {
            while let Some(notification) = notification_rx.recv().await {
                // Handle mining.notify to store job information
                if notification.method == "mining.notify" && !notification.params.is_empty() {
                    log::info!("Received mining.notify notification with {} params", notification.params.len());
                    if let (Some(Value::String(job_id)), Some(Value::String(header_hash))) =
                        (notification.params.first(), notification.params.get(1))
                    {
                        // Store job
                        {
                            let mut jobs_write = jobs_for_notify.write();
                            jobs_write.insert(job_id.clone(), header_hash.clone());
                            *current_job_for_notify.write() = Some(job_id.clone());
                        }
                        log::info!("Stored new job: {} with header_hash: {}", job_id, header_hash);
                    } else {
                        log::warn!("mining.notify params format unexpected: {:?}", notification.params);
                    }
                }

                // Handle mining.set_difficulty to store difficulty
                if notification.method == "mining.set_difficulty" && !notification.params.is_empty() {
                    if let Some(Value::Number(diff)) = notification.params.first() {
                        if let Some(diff_f64) = diff.as_f64() {
                            {
                                let mut state_write = state_for_notify.write();
                                state_write.difficulty = diff_f64;
                            }
                            log::info!("Updated difficulty to {}", diff_f64);
                        }
                    }
                }

                // Forward notification to client
                let json = match serde_json::to_string(&notification) {
                    Ok(j) => j,
                    Err(e) => {
                        log::error!("Failed to serialize notification: {}", e);
                        continue;
                    }
                };
                let line = format!("{}\n", json);
                if response_tx_notify.send(line).is_err() {
                    break;
                }
            }
        });

        // Handle incoming requests
        // Rate limiting: track message count and elapsed time (like JS pool: MAX_MESSAGES_PER_SECOND = 100)
        const MAX_MESSAGES_PER_SECOND: u64 = 100;
        const RATE_LIMIT_THRESHOLD_MULTIPLIER: u64 = 10; // Disconnect if 10x threshold exceeded

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    // Rate limiting check (like JS pool)
                    {
                        let mut state_guard = state.write();
                        state_guard.message_count += 1;
                        let elapsed_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
                            as u64
                            - state_guard.connected_at * 1000;

                        if elapsed_ms > 1000 {
                            let messages_per_second = (state_guard.message_count * 1000) / elapsed_ms;
                            if messages_per_second > MAX_MESSAGES_PER_SECOND * RATE_LIMIT_THRESHOLD_MULTIPLIER {
                                log::warn!(
                                    "Rate limit exceeded for {} ({} messages in {}ms) - disconnecting",
                                    client_addr_for_handle,
                                    state_guard.message_count,
                                    elapsed_ms
                                );
                                drop(state_guard);
                                break; // Exit loop to disconnect
                            }
                        }
                    }

                    // Log all incoming lines at info level to track ASIC communication
                    if !line.is_empty() && line.len() < 500 {
                        log::info!("Received line from {}: {}", client_addr_for_handle, line.trim());
                    }

                    match parse_message(line.as_bytes()) {
                        Err(e) => {
                            log::warn!("Failed to parse message from {}: {} - Line: {}", client_addr_for_handle, e, line);
                            continue;
                        }
                        Ok(request) => {
                            log::debug!("Received request from {}: method={}", client_addr_for_handle, request.method);
                            // Store method before moving request
                            let method = request.method.clone();
                            // Create a temporary client handler for this request
                            let response = match method.as_str() {
                                "mining.subscribe" => {
                                    let response = Self::handle_subscribe_static(
                                        &state,
                                        request.clone(),
                                        client_addr_for_handle,
                                        default_difficulty_for_handle,
                                    )
                                    .await?;

                                    // CRITICAL: Send messages SYNCHRONOUSLY and IMMEDIATELY after subscribe response
                                    // This matches the pool's behavior and is required for KS5 compatibility
                                    // Order: subscribe response → set_extranonce → set_difficulty → mining.notify (job)

                                    let state_read = state.read();
                                    let extra_nonce = state_read.extra_nonce.clone();
                                    let encoding = state_read.encoding;
                                    let difficulty = state_read.difficulty;
                                    drop(state_read);

                                    // 1. Send subscribe response first
                                    let response_json = serde_json::to_string(&response).map_err(StratumError::Json)?;
                                    let response_line = format!("{}\n", response_json);
                                    if response_tx.send(response_line).is_err() {
                                        break;
                                    }

                                    // 2. IMMEDIATELY send set_extranonce notification
                                    let extranonce_notification = if encoding == Encoding::Bitmain {
                                        create_notification(
                                            "set_extranonce".to_string(),
                                            vec![
                                                Value::String(extra_nonce.clone()),
                                                Value::Number(serde_json::Number::from(8 - (extra_nonce.len() / 2))),
                                            ],
                                        )
                                    } else {
                                        create_notification("set_extranonce".to_string(), vec![Value::String(extra_nonce)])
                                    };
                                    let extranonce_json = serde_json::to_string(&extranonce_notification).unwrap();
                                    let extranonce_line = format!("{}\n", extranonce_json);
                                    if response_tx.send(extranonce_line).is_err() {
                                        break;
                                    }

                                    // 3. IMMEDIATELY send set_difficulty notification
                                    let difficulty_notification = create_notification(
                                        "mining.set_difficulty".to_string(),
                                        vec![Value::Number(serde_json::Number::from_f64(difficulty).unwrap())],
                                    );
                                    let difficulty_json = serde_json::to_string(&difficulty_notification).unwrap();
                                    let difficulty_line = format!("{}\n", difficulty_json);
                                    if response_tx.send(difficulty_line).is_err() {
                                        break;
                                    }

                                    // 4. IMMEDIATELY send job (if available) - CRITICAL for KS5
                                    // Get current job from server
                                    let server_job_opt = server_current_job.read().clone();
                                    if let Some(server_job) = server_job_opt {
                                        // Send job notification immediately after difficulty
                                        let job_notification = create_notification(
                                            "mining.notify".to_string(),
                                            vec![Value::String(server_job.id.clone()), Value::String(server_job.header_hash.clone())],
                                        );
                                        let job_json = serde_json::to_string(&job_notification).unwrap();
                                        let job_line = format!("{}\n", job_json);
                                        if response_tx.send(job_line).is_err() {
                                            break;
                                        }

                                        // Also store job in client's job map
                                        {
                                            let mut jobs_write = jobs.write();
                                            jobs_write.insert(server_job.id.clone(), server_job.header_hash.clone());
                                            *current_job.write() = Some(server_job.id.clone());
                                        }

                                        log::info!(
                                            "Sent job {} immediately after subscribe to {}",
                                            server_job.id,
                                            client_addr_for_handle
                                        );
                                    } else {
                                        log::warn!(
                                            "No job available to send immediately after subscribe to {}",
                                            client_addr_for_handle
                                        );
                                    }

                                    Ok(response)
                                }
                                "mining.authorize" => {
                                    Self::handle_authorize_static(
                                        &state,
                                        request,
                                        client_addr_for_handle,
                                        Some(miner_addresses_for_handle.clone()),
                                    )
                                    .await
                                }
                                "mining.submit" => {
                                    Self::handle_submit_static(
                                        &state,
                                        &jobs,
                                        &submission_tx_for_handle,
                                        client_addr_for_handle,
                                        request,
                                    )
                                    .await
                                }
                                "mining.extranonce.subscribe" => {
                                    // This is a Stratum extension - respond with success (false means we don't support it)
                                    log::debug!("mining.extranonce.subscribe from {}", client_addr_for_handle);
                                    Ok(create_success_response(request.id, json!(false)))
                                }
                                _ => {
                                    log::warn!("Unknown method from {}: {}", client_addr_for_handle, request.method);
                                    Ok(create_error_response(
                                        request.id,
                                        StratumError::Unknown(format!("Unknown method: {}", request.method)).code(),
                                        format!("Unknown method: {}", request.method),
                                    ))
                                }
                            };

                            // Only send response if it wasn't already sent (subscribe handles its own response)
                            if method != "mining.subscribe" {
                                match response {
                                    Ok(response) => {
                                        let json = serde_json::to_string(&response).map_err(StratumError::Json)?;
                                        let response_line = format!("{}\n", json);
                                        if response_tx.send(response_line).is_err() {
                                            break;
                                        }
                                    }
                                    Err(e) => {
                                        log::error!("Error handling request: {}", e);
                                        let error_response = create_error_response(None, e.code(), e.to_string());
                                        let json = serde_json::to_string(&error_response).map_err(StratumError::Json)?;
                                        let response_line = format!("{}\n", json);
                                        let _ = response_tx.send(response_line);
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("Error reading from stream: {}", e);
                    break;
                }
            }
        }

        drop(response_tx); // Close the channel to signal the write task to exit
        let _ = write_handle.await;
        Ok(())
    }
}
