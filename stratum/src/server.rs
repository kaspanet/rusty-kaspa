//! Stratum server implementation

use crate::client::StratumClient;
use crate::error::StratumError;
use crate::protocol::{create_notification, StratumNotification};
use hex;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::block::BlockTemplate;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::header::Header;
use kaspa_consensusmanager::ConsensusManager;
use kaspa_hashes::{Hash, HasherBase};
use kaspa_math::Uint256;
use kaspa_mining::manager::MiningManagerProxy;
use kaspa_pow::State as PowState;
use kaspa_txscript::pay_to_address_script;
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// Vardiff configuration
#[derive(Debug, Clone)]
pub struct VardiffConfig {
    pub enabled: bool,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    pub target_time: f64,      // Target time between shares in seconds
    pub variance_percent: f64, // Variance percentage (e.g., 30.0 for 30%)
    pub max_change: f64,       // Maximum change multiplier (e.g., 2.0 for 2x)
    pub change_interval: u64,  // Minimum seconds between difficulty changes
    pub clamp_pow2: bool,      // Clamp difficulty to powers of 2 (required for IceRiver/Bitmain ASICs)
}

impl Default for VardiffConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_difficulty: 1.0,
            max_difficulty: 1000000.0,
            target_time: 30.0,      // 30 seconds between shares
            variance_percent: 30.0, // 30% variance
            max_change: 2.0,        // Max 2x change
            change_interval: 60,    // 60 seconds minimum between changes
            clamp_pow2: true,       // Default to true for IceRiver/Bitmain ASIC compatibility
        }
    }
}

/// Stratum server configuration
#[derive(Debug, Clone)]
pub struct StratumConfig {
    pub listen_address: String,
    pub listen_port: u16,
    pub default_difficulty: f64,
    pub enabled: bool,
    pub vardiff: VardiffConfig,
}

impl Default for StratumConfig {
    fn default() -> Self {
        Self {
            listen_address: "0.0.0.0".to_string(),
            listen_port: 3333,
            default_difficulty: 1.0, // Start at difficulty 1, let vardiff increase if needed
            enabled: false,
            vardiff: VardiffConfig::default(),
        }
    }
}

/// Active mining job
#[derive(Debug, Clone)]
pub struct MiningJob {
    pub id: String,
    pub template_hash: Hash,
    pub header_hash: String,
    pub timestamp: u64,
    pub difficulty: f64,
    pub template: BlockTemplate, // Store full template for block reconstruction
}

/// Block submission request from client
#[derive(Debug)]
pub struct BlockSubmission {
    pub job_id: String,
    pub nonce: u64,
    pub client_addr: SocketAddr,
    pub response_tx: Option<mpsc::UnboundedSender<bool>>, // Send true if accepted, false if rejected
    pub request_id: Option<u64>,                          // Store request ID to send proper response
}

/// Vardiff state for a client (stored in server)
#[derive(Debug, Clone)]
struct ClientVardiffState {
    last_share: u64,             // Timestamp in milliseconds
    last_difficulty_change: u64, // Timestamp in milliseconds
    current_difficulty: f64,
    share_count: u64,
    #[allow(dead_code)]
    connected_at: u64, // Timestamp in milliseconds (stored for future use)
    rejected_share_count: u64, // Count of consecutive rejected shares
    last_rejected_share: u64,  // Timestamp of last rejected share
}

/// Stratum server
pub struct StratumServer {
    config: StratumConfig,
    consensus_manager: Arc<ConsensusManager>,
    mining_manager: MiningManagerProxy,
    clients: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
    jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
    current_job: Arc<RwLock<Option<MiningJob>>>,
    job_counter: Arc<RwLock<u64>>,
    submission_tx: mpsc::UnboundedSender<BlockSubmission>,
    submission_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<BlockSubmission>>>>,
    vardiff_states: Arc<RwLock<HashMap<SocketAddr, ClientVardiffState>>>, // Vardiff tracking per client
    miner_addresses: Arc<RwLock<HashMap<SocketAddr, Address>>>,           // Miner addresses (from mining.authorize)
    seen_nonces: Arc<RwLock<HashSet<(String, u64)>>>,                     // Track (job_id, nonce) pairs to detect duplicates
}

impl StratumServer {
    /// Create a new Stratum server
    pub fn new(config: StratumConfig, consensus_manager: Arc<ConsensusManager>, mining_manager: MiningManagerProxy) -> Self {
        let (submission_tx, submission_rx) = mpsc::unbounded_channel();
        Self {
            config,
            consensus_manager,
            mining_manager,
            clients: Arc::new(RwLock::new(HashMap::new())),
            jobs: Arc::new(RwLock::new(HashMap::new())),
            current_job: Arc::new(RwLock::new(None)),
            job_counter: Arc::new(RwLock::new(0)),
            submission_tx,
            submission_rx: Arc::new(Mutex::new(Some(submission_rx))),
            vardiff_states: Arc::new(RwLock::new(HashMap::new())),
            miner_addresses: Arc::new(RwLock::new(HashMap::new())),
            seen_nonces: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Get the submission sender for clients to use
    pub fn get_submission_sender(&self) -> mpsc::UnboundedSender<BlockSubmission> {
        self.submission_tx.clone()
    }

    /// Start the Stratum server
    pub async fn start(&self) -> Result<(), StratumError> {
        if !self.config.enabled {
            log::info!("Stratum server is disabled");
            return Ok(());
        }

        let addr = format!("{}:{}", self.config.listen_address, self.config.listen_port);
        let listener = TcpListener::bind(&addr).await.map_err(StratumError::Io)?;

        log::info!("Stratum server listening on {}", addr);

        // Clone what we need from &self
        let consensus_manager_clone = self.consensus_manager.clone();
        let mining_manager_clone = self.mining_manager.clone();
        let jobs_clone = self.jobs.clone();
        let current_job_clone = self.current_job.clone();
        let job_counter_clone = self.job_counter.clone();
        let clients_for_loop = self.clients.clone();
        let clients_for_distribution = self.clients.clone();
        let default_difficulty = self.config.default_difficulty;
        let submission_tx = self.submission_tx.clone();
        // Take the receiver from the Mutex so we can move it into the task
        let submission_rx =
            self.submission_rx.lock().take().ok_or_else(|| StratumError::Unknown("Submission receiver already taken".to_string()))?;
        let consensus_submit = self.consensus_manager.clone();
        let jobs_submit = self.jobs.clone();
        let vardiff_states_for_loop = self.vardiff_states.clone();
        let current_job_for_loop = self.current_job.clone();
        let vardiff_config = self.config.vardiff.clone();

        // Start job distribution task
        let vardiff_states_dist = vardiff_states_for_loop.clone();
        let vardiff_config_dist = vardiff_config.clone();
        let miner_addresses_dist = self.miner_addresses.clone();
        tokio::spawn(async move {
            Self::job_distribution_loop(
                consensus_manager_clone,
                mining_manager_clone,
                jobs_clone,
                current_job_clone,
                job_counter_clone,
                clients_for_distribution,
                default_difficulty,
                vardiff_states_dist,
                vardiff_config_dist,
                miner_addresses_dist,
            )
            .await;
        });

        // Start block submission handler
        let vardiff_states_submit = vardiff_states_for_loop.clone();
        let vardiff_config_submit = vardiff_config.clone();
        let clients_submit = clients_for_loop.clone();
        let seen_nonces_submit = self.seen_nonces.clone();
        tokio::spawn(async move {
            Self::block_submission_loop(
                consensus_submit,
                jobs_submit,
                submission_rx,
                vardiff_states_submit,
                vardiff_config_submit,
                clients_submit,
                seen_nonces_submit,
            )
            .await;
        });

        // Start vardiff monitoring if enabled
        if vardiff_config.enabled {
            let vardiff_states_monitor = vardiff_states_for_loop.clone();
            let vardiff_config_monitor = vardiff_config.clone();
            let clients_monitor = clients_for_loop.clone();
            tokio::spawn(async move {
                Self::vardiff_monitoring_loop(vardiff_states_monitor, vardiff_config_monitor, clients_monitor).await;
            });
        }

        // Accept connections with graceful shutdown support
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            log::info!("New Stratum connection from {} (remote: {:?})", addr, stream.peer_addr());
                            let client = StratumClient::new(stream, addr);
                            let notification_tx = client.get_notification_sender();

                            // Store client
                            {
                                let mut clients = clients_for_loop.write();
                                clients.insert(addr, notification_tx.clone());
                            }

                            // Initialize vardiff state if enabled
                            if vardiff_config.enabled {
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis() as u64;
                                let mut vardiff_states = vardiff_states_for_loop.write();
                                vardiff_states.insert(addr, ClientVardiffState {
                                    last_share: now_ms,
                                    last_difficulty_change: now_ms,
                                    current_difficulty: default_difficulty,
                                    share_count: 0,
                                    connected_at: now_ms,
                                    rejected_share_count: 0,
                                    last_rejected_share: 0,
                                });
                                log::info!("Initialized vardiff for client {} with initial difficulty {}", addr, default_difficulty);
                            }

                            // Spawn client handler
                            let clients_clone = clients_for_loop.clone();
                            let submission_tx_clone = submission_tx.clone();
                            let vardiff_states_disconnect = vardiff_states_for_loop.clone();
                            let vardiff_config_disconnect = vardiff_config.clone();
                            let default_difficulty_for_client = default_difficulty;
                            let current_job_for_client = current_job_for_loop.clone();
                            let notification_tx_for_client = notification_tx.clone();
                            let miner_addresses_for_client = self.miner_addresses.clone();
                            let miner_addresses_for_disconnect = self.miner_addresses.clone();
                            tokio::spawn(async move {
                                if let Err(e) = client.run(
                                    submission_tx_clone,
                                    addr,
                                    default_difficulty_for_client,
                                    current_job_for_client,
                                    notification_tx_for_client,
                                    miner_addresses_for_client,
                                ).await {
                                    log::error!("Client handler error: {}", e);
                                }
                                // Remove client on disconnect
                                clients_clone.write().remove(&addr);
                                // Remove vardiff state on disconnect
                                if vardiff_config_disconnect.enabled {
                                    vardiff_states_disconnect.write().remove(&addr);
                                }
                                // Remove miner address on disconnect
                                miner_addresses_for_disconnect.write().remove(&addr);
                            });
                        }
                        Err(e) => {
                            log::error!("Error accepting connection: {}", e);
                        }
                    }
                }
                _ = tokio::signal::ctrl_c() => {
                    log::info!("Received shutdown signal (Ctrl+C), shutting down Stratum server gracefully...");
                    // Close all client connections
                    let clients = clients_for_loop.write();
                    log::info!("Closing {} active client connection(s)", clients.len());
                    drop(clients);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Job distribution loop - monitors for new block templates and distributes to miners
    async fn job_distribution_loop(
        consensus_manager: Arc<ConsensusManager>,
        mining_manager: MiningManagerProxy,
        jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
        current_job: Arc<RwLock<Option<MiningJob>>>,
        job_counter: Arc<RwLock<u64>>,
        clients: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
        default_difficulty: f64,
        vardiff_states: Arc<RwLock<HashMap<SocketAddr, ClientVardiffState>>>,
        vardiff_config: VardiffConfig,
        miner_addresses: Arc<RwLock<HashMap<SocketAddr, Address>>>,
    ) {
        // TODO: Subscribe to new block template notifications
        // For now, poll periodically
        // Use longer interval during IBD (60s) vs normal operation (10s)
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
        let mut is_ibd = false; // Track if we're in IBD based on error patterns
        let mut last_template_time = tokio::time::Instant::now(); // Track last template distribution time for rate limiting

        loop {
            interval.tick().await;

            // Check if consensus is in transitional IBD state before attempting template build
            // Get a fresh session each time to avoid Send issues
            let in_transitional_ibd = {
                let consensus_instance = consensus_manager.consensus();
                let consensus_session = consensus_instance.unguarded_session();
                consensus_session.async_is_consensus_in_transitional_ibd_state().await
            };

            // Get current block template
            // Use the first authorized miner's address, or fall back to placeholder if none
            // Create address and miner_data in a separate scope to ensure Send safety
            // Extract the address string to avoid holding Address across await points
            let miner_data = {
                let miner_address = {
                    let miner_addresses_read = miner_addresses.read();
                    miner_addresses_read
                        .values()
                        .next()
                        .cloned()
                        .unwrap_or_else(|| Address::new(Prefix::Mainnet, Version::PubKey, &[0u8; 32]))
                };
                
                // Log which address we're using (do this while we have the address)
                let has_miners = !miner_addresses.read().is_empty();
                if has_miners {
                    log::debug!("Using miner address for block template: {}", miner_address);
                } else {
                    log::debug!(
                        "No authorized miners yet - using placeholder address for block template (will update once miners authorize)"
                    );
                }
                
                // Create miner_data immediately and drop the address
                let script_pub_key = pay_to_address_script(&miner_address);
                MinerData::new(script_pub_key, vec![])
            };

            // Skip template building if in transitional IBD state
            // CRITICAL: During IBD, the virtual state and pruning points are changing,
            // which makes templates invalid. We must NOT serve templates during IBD
            // as blocks mined on them would be rejected.
            if in_transitional_ibd {
                if !is_ibd {
                    log::info!("Node is in IBD - clearing templates and pausing template distribution (templates invalid during IBD)");
                    is_ibd = true;
                    interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

                    // Clear all jobs since they're invalid during IBD
                    // Virtual state changes during IBD make templates stale
                    let job_count = {
                        let jobs_read = jobs.read();
                        let count = jobs_read.len();
                        drop(jobs_read);
                        let mut jobs_write = jobs.write();
                        jobs_write.clear();
                        *current_job.write() = None;
                        count
                    };
                    if job_count > 0 {
                        log::info!("Cleared {} cached job(s) - templates invalid during IBD", job_count);
                    }
                }
                log::debug!("IBD in progress - template building paused until IBD completes");
                continue;
            }

            let mining_manager_clone = mining_manager.clone();
            // Use the async get_block_template method on MiningManagerProxy
            // Get a fresh session for template building
            let template_result = {
                let consensus_instance = consensus_manager.consensus();
                let consensus_session = consensus_instance.unguarded_session();
                mining_manager_clone.get_block_template(&consensus_session, miner_data.clone()).await
            };
            match template_result {
                Ok(template) => {
                    // CRITICAL: Template distribution rate limiting for Bitmain/IceRiver ASICs
                    // These ASICs can have issues if they receive new jobs too frequently.
                    // Add 250ms minimum delay between template distributions (matches bridge behavior).
                    let elapsed_since_last_template = last_template_time.elapsed();
                    const MIN_TEMPLATE_INTERVAL: tokio::time::Duration = tokio::time::Duration::from_millis(250);
                    if elapsed_since_last_template < MIN_TEMPLATE_INTERVAL {
                        let wait_time = MIN_TEMPLATE_INTERVAL - elapsed_since_last_template;
                        log::debug!("Rate limiting template distribution - waiting {}ms (min interval: 250ms)", wait_time.as_millis());
                        tokio::time::sleep(wait_time).await;
                    }
                    last_template_time = tokio::time::Instant::now();

                    // Successfully got template - reset IBD flag and interval if needed
                    if is_ibd {
                        log::info!("Block template building resumed after IBD - returning to normal polling interval");
                        is_ibd = false;
                        interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
                    }

                    // Create new job from template
                    let job = Self::create_job_from_template(&template, &job_counter, default_difficulty);

                    // Store job
                    {
                        let mut jobs_write = jobs.write();
                        jobs_write.insert(job.id.clone(), job.clone());
                        *current_job.write() = Some(job.clone());
                    }

                    // Notify all connected clients
                    // CRITICAL: Add 500 microsecond delay between client notifications to prevent
                    // network congestion and ASIC overload (matches bridge behavior)
                    // Collect clients into a Vec first to avoid holding the lock across await points
                    let clients_to_notify: Vec<(SocketAddr, mpsc::UnboundedSender<StratumNotification>)> = {
                        let clients_read = clients.read();
                        clients_read.iter().map(|(addr, tx)| (*addr, tx.clone())).collect()
                    };
                    let client_count = clients_to_notify.len();
                    let mut client_index = 0;
                    for (addr, tx) in clients_to_notify.iter() {
                        // Add spacing delay between clients (except for the first one)
                        if client_index > 0 {
                            tokio::time::sleep(tokio::time::Duration::from_micros(500)).await;
                        }
                        client_index += 1;

                        // Use vardiff difficulty if enabled, otherwise use default
                        let difficulty_to_use = if vardiff_config.enabled {
                            let vardiff_states_read = vardiff_states.read();
                            vardiff_states_read.get(addr).map(|s| s.current_difficulty).unwrap_or(default_difficulty)
                        } else {
                            default_difficulty
                        };

                        // Create job with client-specific difficulty
                        let mut client_job = job.clone();
                        client_job.difficulty = difficulty_to_use;

                        // Check if client is still connected before sending
                        // If the channel is closed, the client has disconnected
                        match Self::send_job_notification(tx, &client_job, difficulty_to_use) {
                            Ok(_) => {}
                            Err(e) => {
                                // Client may have disconnected - this is normal, log at debug level
                                log::debug!("Failed to send job to {} (may have disconnected): {}", addr, e);
                            }
                        }
                    }
                    if client_count > 0 {
                        log::info!("Distributed new job {} to {} client(s)", job.id, client_count);
                    }
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    // Check if this is an IBD-related error (missing reward data)
                    if error_msg.contains("missing reward data") || error_msg.contains("bad coinbase payload") {
                        // This is expected during IBD - clear templates and pause distribution
                        if !is_ibd {
                            log::info!(
                                "Node appears to be in IBD (missing reward data) - clearing templates and pausing distribution"
                            );
                            is_ibd = true;
                            interval = tokio::time::interval(tokio::time::Duration::from_secs(60));

                            // Clear all jobs since they're invalid during IBD
                            {
                                let mut jobs_write = jobs.write();
                                jobs_write.clear();
                                *current_job.write() = None;
                            }
                        }
                        log::debug!("IBD in progress (missing reward data) - template building paused until IBD completes");
                    } else {
                        // Other errors - log at warn level and keep normal interval
                        log::warn!("Failed to get block template: {}", e);
                    }
                }
            }
        }
    }

    /// Calculate target from difficulty
    /// Target = max_target / difficulty
    /// Based on WASM calculateTarget function from kaspa-miner
    /// Formula: target = (2^256) / difficulty
    /// For Kaspa, we use Uint256 for full precision
    ///
    /// The WASM implementation uses: target = (2^256) / difficulty
    /// Since Uint256 can represent values up to 2^256 - 1, we use:
    /// target = (2^256 - 1) / difficulty
    ///
    /// For powers of 2, we can use bit shifting for exact division
    /// For other values, we use f64 conversion and back (less precise but works)
    fn calculate_target_from_difficulty(difficulty: f64) -> Uint256 {
        if difficulty <= 0.0 {
            return Uint256::MAX; // Invalid difficulty, return max target
        }

        // Special case: difficulty 1.0 means target = 2^256 - 1 (Uint256::MAX)
        if (difficulty - 1.0).abs() < f64::EPSILON {
            return Uint256::MAX;
        }

        // Check if difficulty is a power of 2 (for exact bit-shift division)
        let log2_diff = difficulty.log2();
        let log2_rounded = log2_diff.round();
        let is_power_of_2 = (log2_diff - log2_rounded).abs() < 1e-10;

        if is_power_of_2 {
            // Exact division using bit shifting: Uint256::MAX >> log2_rounded
            let shift_bits = log2_rounded as u32;
            if shift_bits >= 256 {
                return Uint256::from(1u64); // Shift would result in 0 or 1
            }
            // Right shift Uint256::MAX by shift_bits
            // This is equivalent to dividing by 2^shift_bits = difficulty
            return Uint256::MAX >> shift_bits;
        }

        // For non-power-of-2 difficulties, use f64 conversion
        // This is less precise but necessary for arbitrary difficulties
        let max_target_f64 = Uint256::MAX.as_f64();
        let target_f64 = max_target_f64 / difficulty;

        // Convert back to Uint256 (this may lose precision for very large values)
        // But for reasonable difficulties (1 to millions), this should be fine
        if target_f64 >= max_target_f64 {
            return Uint256::MAX;
        }

        // Try to convert f64 to Uint256
        // Uint256 doesn't have from_f64, so we need to use a different approach
        // For now, use the approximation method but with better precision
        let log2_diff_floor = log2_diff.floor() as i32;
        let exponent = 256i32 - log2_diff_floor;

        if exponent <= 0 {
            return Uint256::from(1u64);
        }

        if exponent >= 256 {
            return Uint256::MAX;
        }

        // Calculate 2^exponent
        let mut result = Uint256::from(1u64);
        let exp_u32 = exponent as u32;
        let chunk_size = 64u32;
        let chunk_value = Uint256::from(1u64) << chunk_size;

        let mut remaining = exp_u32;
        while remaining >= chunk_size {
            result = result * chunk_value;
            remaining -= chunk_size;
        }

        if remaining > 0 {
            let remaining_value = Uint256::from(1u64) << remaining;
            result = result * remaining_value;
        }

        // Adjust for non-power-of-2 difficulty by dividing result by (difficulty / 2^log2_floor)
        // This gives us a better approximation
        let remainder_factor = difficulty / (2.0_f64.powf(log2_diff_floor as f64));
        if remainder_factor > 1.0 {
            // We need to divide further - approximate by right-shifting
            // This is still an approximation but better than before
            let additional_shift = (remainder_factor.log2().floor() as u32).min(255);
            if additional_shift > 0 {
                result = result >> additional_shift;
            }
        }

        result.max(Uint256::from(1u64)) // Ensure minimum of 1
    }

    /// Clamp difficulty to the nearest power of 2
    /// Required for IceRiver/Bitmain ASICs which only accept powers of 2
    /// Based on kaspa-stratum-bridge ClampPow2 feature
    /// When decreasing, rounds DOWN to prevent getting stuck at high difficulty
    fn clamp_to_power_of_2(difficulty: f64) -> f64 {
        if difficulty <= 0.0 {
            return 1.0;
        }

        // Calculate log2 of difficulty
        let log2_diff = difficulty.log2();

        // Round DOWN (floor) to ensure we don't round up when decreasing difficulty
        // This prevents getting stuck at high difficulty values
        let rounded_log2 = log2_diff.floor();

        // Calculate 2^rounded_log2
        let clamped = 2.0_f64.powf(rounded_log2);

        // Ensure minimum of 1.0
        clamped.max(1.0)
    }

    /// Calculate difficulty from block header bits
    /// Uses the same method as Kaspa RPC: Uint256::from_compact_target_bits() then divide by max_difficulty_target
    /// This matches the implementation in rpc/service/src/converter/consensus.rs
    /// The max_difficulty_target is the maximum possible target (2^256 - 1) as f64
    fn calculate_difficulty_from_bits(bits: u32) -> f64 {
        // Use the proper Kaspa method to convert compact bits to target
        let target = Uint256::from_compact_target_bits(bits);

        // The max_difficulty_target is the maximum possible target (2^256 - 1) as f64
        // This matches what's stored in the consensus config
        let max_difficulty_target_f64 = Uint256::MAX.as_f64();

        // Calculate difficulty: max_difficulty_target / target
        // This matches the RPC converter implementation exactly
        let target_f64 = target.as_f64();
        if target_f64 <= 0.0 {
            return 1.0; // Avoid division by zero, use minimum difficulty
        }

        let difficulty = max_difficulty_target_f64 / target_f64;

        // Ensure minimum difficulty of 1.0
        difficulty.max(1.0)
    }

    /// Compute the pre-PoW hash (hash WITHOUT timestamp and nonce)
    /// This matches the WASM PoW.prePoWHash property
    /// Based on kaspa_consensus_core::hashing::header::hash_override_nonce_time
    /// but excludes timestamp and nonce from the hash
    fn compute_pre_pow_hash(header: &Header) -> Hash {
        // We need to manually implement the hashing logic since HasherExtensions is not public
        // This matches the logic in consensus/core/src/hashing/header.rs but excludes timestamp and nonce
        let mut hasher = kaspa_hashes::BlockHash::new();

        // Write version
        hasher.update(header.version.to_le_bytes());

        // Write number of parent levels
        let expanded_len = header.parents_by_level.expanded_len();
        hasher.update((expanded_len as u64).to_le_bytes());

        // Write parents at each level
        for level in header.parents_by_level.expanded_iter() {
            // Write array length
            hasher.update((level.len() as u64).to_le_bytes());
            // Write each parent hash
            for parent in level {
                hasher.update(parent);
            }
        }

        // Write all header fields EXCEPT timestamp and nonce
        hasher
            .update(header.hash_merkle_root)
            .update(header.accepted_id_merkle_root)
            .update(header.utxo_commitment)
            // SKIP timestamp
            .update(header.bits.to_le_bytes())
            // SKIP nonce
            .update(header.daa_score.to_le_bytes())
            .update(header.blue_score.to_le_bytes());

        // Write blue_work (big endian bytes without leading zeros)
        let be_bytes = header.blue_work.to_be_bytes();
        let start = be_bytes.iter().copied().position(|byte| byte != 0).unwrap_or(be_bytes.len());
        let blue_work_bytes = &be_bytes[start..];
        hasher.update((blue_work_bytes.len() as u64).to_le_bytes());
        hasher.update(blue_work_bytes);

        // Write pruning_point
        hasher.update(header.pruning_point);

        hasher.finalize()
    }

    /// Create a mining job from a block template
    fn create_job_from_template(template: &BlockTemplate, job_counter: &Arc<RwLock<u64>>, difficulty: f64) -> MiningJob {
        let mut counter = job_counter.write();
        *counter += 1;
        let job_id = format!("{:x}", *counter);
        drop(counter);

        // Compute the pre-PoW hash (hash WITHOUT timestamp and nonce)
        // This matches what the pool uses: proofOfWork.prePoWHash
        let pre_pow_hash = Self::compute_pre_pow_hash(&template.block.header);
        let hash_hex = pre_pow_hash.to_string();

        // Calculate difficulty from block header bits using proper Kaspa method
        let calculated_difficulty = Self::calculate_difficulty_from_bits(template.block.header.bits);
        // Use the calculated difficulty, but allow override from config if needed
        let job_difficulty = if difficulty > 0.0 { difficulty } else { calculated_difficulty };

        log::info!(
            "Template pre-PoW hash: {} (length: {}), nonce: {}, timestamp: {}, bits: {}, calculated difficulty: {}",
            hash_hex,
            hash_hex.len(),
            template.block.header.nonce,
            template.block.header.timestamp,
            template.block.header.bits,
            calculated_difficulty
        );

        // Convert timestamp to little-endian bytes (8 bytes = 16 hex chars)
        let timestamp = template.block.header.timestamp;
        let timestamp_bytes = timestamp.to_le_bytes();
        let timestamp_hex = hex::encode(timestamp_bytes);

        // Concatenate prePoWHash + timestamp (little-endian hex) - this is the format ASICs expect
        // The pool sends: prePoWHash (64 hex chars) + timestamp (16 hex chars) = 80 hex chars total
        let header_hash = format!("{}{}", hash_hex, timestamp_hex);
        log::info!(
            "Job {} - prePoWHash+timestamp: {} (length: {}, expected: 80), difficulty: {}",
            job_id,
            header_hash,
            header_hash.len(),
            job_difficulty
        );

        MiningJob {
            id: job_id,
            template_hash: template.block.header.hash,
            header_hash,
            timestamp: template.block.header.timestamp, // Store for reference, but use template's timestamp when reconstructing
            difficulty: job_difficulty,
            template: template.clone(), // Store full template - it already has the correct timestamp
        }
    }

    /// Handle block submissions from miners
    async fn block_submission_loop(
        consensus_manager: Arc<ConsensusManager>,
        jobs: Arc<RwLock<HashMap<String, MiningJob>>>,
        mut submission_rx: mpsc::UnboundedReceiver<BlockSubmission>,
        vardiff_states: Arc<RwLock<HashMap<SocketAddr, ClientVardiffState>>>,
        vardiff_config: VardiffConfig,
        clients: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
        seen_nonces: Arc<RwLock<HashSet<(String, u64)>>>,
    ) {
        // Track last cleanup time for seen_nonces (cleanup every 5 minutes)
        let mut last_cleanup = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        const CLEANUP_INTERVAL: u64 = 300; // 5 minutes

        while let Some(submission) = submission_rx.recv().await {
            // Log at debug level to reduce verbosity, but log first submission to confirm processing
            static FIRST_SUBMISSION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
            if FIRST_SUBMISSION.swap(false, std::sync::atomic::Ordering::Relaxed) {
                log::info!(
                    "Processing block submissions - first submission received from {} (Job: {}, Nonce: 0x{:x})",
                    submission.client_addr,
                    submission.job_id,
                    submission.nonce
                );
            } else {
                log::debug!(
                    "Received block submission from {} - Job: {}, Nonce: 0x{:x}",
                    submission.client_addr,
                    submission.job_id,
                    submission.nonce
                );
            }

            // Check for duplicate nonce (before PoW validation, like JS pool)
            // The JS pool checks: if (this.contributions.has(nonce)) throw 'duplicate-share'
            // We track (job_id, nonce) pairs to detect duplicate submissions
            // Note: We use the original job_id for duplicate checking, not the corrected one
            let nonce_key = (submission.job_id.clone(), submission.nonce);
            {
                let mut seen = seen_nonces.write();
                if seen.contains(&nonce_key) {
                    // Send rejection response for duplicate (using current bool format)
                    if let Some(ref response_tx) = submission.response_tx {
                        let _ = response_tx.send(false);
                    }
                    log::debug!("[REJECTED] Duplicate share - job {} nonce 0x{:x} already seen", submission.job_id, submission.nonce);
                    continue;
                }
                // Add to seen set before processing
                seen.insert(nonce_key.clone());
            }

            let jobs_read = jobs.read();
            let job = match jobs_read.get(&submission.job_id) {
                Some(j) => j.clone(),
                None => {
                    // Remove from seen_nonces since job doesn't exist (cleanup)
                    seen_nonces.write().remove(&nonce_key);
                    // Send rejection response for stale job
                    if let Some(ref response_tx) = submission.response_tx {
                        let _ = response_tx.send(false);
                    }
                    // Reduce verbosity for expected errors (stale jobs are normal)
                    log::debug!(
                        "[REJECTED] Stale job - received submission for unknown job: {} from {} (job may have expired)",
                        submission.job_id,
                        submission.client_addr
                    );
                    drop(jobs_read);
                    continue;
                }
            };
            drop(jobs_read);

            // Periodic cleanup of seen_nonces (remove entries for expired jobs)
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            if now - last_cleanup > CLEANUP_INTERVAL {
                let jobs_snapshot: Vec<String> = {
                    let jobs_read = jobs.read();
                    jobs_read.keys().cloned().collect()
                };
                let mut seen = seen_nonces.write();
                let before = seen.len();
                seen.retain(|(job_id, _)| jobs_snapshot.contains(job_id));
                let after = seen.len();
                if before > after {
                    log::debug!("Cleaned up {} duplicate nonce entries (removed entries for expired jobs)", before - after);
                }
                last_cleanup = now;
            }

            // CRITICAL: Job ID workaround for Bitmain/IceRiver ASICs
            // These ASICs sometimes submit shares with incorrect job IDs. When a share is rejected
            // due to low difficulty, we need to loop through previous job IDs to find the correct one.
            // This is a workaround for a known bug in these ASIC firmware versions.
            // We need to do this BEFORE checking stale templates, as the correct job might be older.
            let mut current_job_id = submission.job_id.clone();
            let mut current_job_to_check = job.clone();
            let mut invalid_share = false;
            const MAX_JOBS: u64 = 300; // Maximum number of jobs to check (matches bridge)

            // CRITICAL: Validate share difficulty using PoW hash (not header hash)
            // The pool code shows: state.checkWork(nonce) returns [isBlock, target]
            // where target is the PoW hash value, which is compared against pool difficulty
            // We must use the same PoW validation logic for both pool and network difficulty

            // Create PoW state and check PoW hash
            // CRITICAL: Create PoW state from header BEFORE setting nonce (like JS pool does)
            // The JS pool does: state = getPoW(hash) then state.checkWork(nonce)
            // So we need to create PoW state from the original template header, not the reconstructed one
            let original_header = &current_job_to_check.template.block.header;
            let pow_state = PowState::new(original_header);
            let (mut pow_passed, mut pow_value) = pow_state.check_pow(submission.nonce);

            // Calculate target from pool difficulty (job.difficulty)
            // The pool uses calculateTarget(difficulty) which converts difficulty to a target
            let mut pool_target = Self::calculate_target_from_difficulty(current_job_to_check.difficulty);

            // Compare PoW hash value against pool difficulty target
            // In the pool: if (target > calculateTarget(socket.data.difficulty)) throw 'low-difficulty-share'
            // So we need: pow_value <= pool_target (lower PoW value = higher difficulty = better)
            let meets_pool_difficulty = pow_value <= pool_target;

            if !meets_pool_difficulty {
                // Share doesn't meet difficulty - check if it's due to incorrect job ID
                // Loop through previous job IDs (like the bridge does)
                invalid_share = true;
                let job_id_num: Option<u64> = current_job_id.parse().ok();
                
                // Try previous job IDs if job_id is numeric
                if let Some(mut job_id_num_val) = job_id_num {
                    let mut found_valid_job = false;
                    let mut attempts = 0;
                    const MAX_ATTEMPTS: u64 = 10; // Limit attempts to prevent infinite loop

                    while attempts < MAX_ATTEMPTS && job_id_num_val > 1 {
                        attempts += 1;
                        job_id_num_val -= 1;
                        let prev_job_id = format!("{:x}", job_id_num_val);

                        // Check if this job ID exists
                        let prev_job_opt = {
                            let jobs_read = jobs.read();
                            jobs_read.get(&prev_job_id).cloned()
                        };
                        if let Some(prev_job) = prev_job_opt {
                            // Found a previous job - check if nonce meets difficulty for this job
                            let prev_pow_state = PowState::new(&prev_job.template.block.header);
                            let (prev_pow_passed, prev_pow_value) = prev_pow_state.check_pow(submission.nonce);
                            let prev_pool_target = Self::calculate_target_from_difficulty(prev_job.difficulty);
                            let prev_meets_difficulty = prev_pow_value <= prev_pool_target;

                            if prev_meets_difficulty {
                                // Found the correct job! Use this job instead
                                current_job_id = prev_job_id.clone();
                                current_job_to_check = prev_job;
                                invalid_share = false;
                                found_valid_job = true;
                                // Update pow values for the correct job
                                pow_passed = prev_pow_passed;
                                pow_value = prev_pow_value;
                                pool_target = prev_pool_target;
                                log::debug!(
                                    "[Job ID Workaround] Found correct job ID: {} (was submitted as {}) for nonce 0x{:x}",
                                    current_job_id,
                                    submission.job_id,
                                    submission.nonce
                                );
                                break;
                            }
                        }

                        // Check if we've exhausted all previous blocks
                        if job_id_num_val == 1 || (job_id_num_val % MAX_JOBS) == ((submission.job_id.parse::<u64>().unwrap_or(0) % MAX_JOBS) + 1) {
                            break;
                        }
                    }

                    if !found_valid_job {
                        invalid_share = true;
                    }
                } else {
                    // Job ID is not numeric (hex format) - can't do workaround, just reject
                    invalid_share = true;
                }
            }

            // Use the correct job (may have been changed by job ID workaround)
            // Clone it so we can still use current_job_to_check for vardiff state
            let job = current_job_to_check.clone();
            let submission_job_id = current_job_id.clone();

            // Check if template is stale (older than 10 seconds)
            // Stale templates can cause InvalidPoW errors
            // NOTE: We check this AFTER the job ID workaround, as the correct job might be older
            let current_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
            let template_timestamp = job.template.block.header.timestamp as u64;

            if current_time > template_timestamp {
                let template_age = current_time - template_timestamp;
                if template_age > 10 {
                    // Reduce verbosity for expected errors (stale templates are normal when blocks arrive quickly)
                    log::debug!(
                        "[REJECTED] Stale template - job {} is {} seconds old (max 10s) - skipping submission",
                        submission_job_id,
                        template_age
                    );
                    // Remove from seen_nonces since we're rejecting
                    seen_nonces.write().remove(&nonce_key);
                    // Send rejection response
                    if let Some(ref response_tx) = submission.response_tx {
                        let _ = response_tx.send(false);
                    }
                    continue;
                }
            }

            // Log PoW validation details only for blocks that pass network difficulty or on errors
            if pow_passed {
                log::info!(
                    "[BLOCK FOUND] Job {} - Nonce: 0x{:x}, PoW value: {}, Network target passed! Submitting to consensus...",
                    submission_job_id,
                    submission.nonce,
                    pow_value
                );
            } else {
                log::debug!(
                    "[PoW] Job {} - Nonce: 0x{:x}, PoW value: {}, Pool target: {}, Network passed: {}",
                    submission_job_id,
                    submission.nonce,
                    pow_value,
                    pool_target,
                    pow_passed
                );
            }

            // Reconstruct block with nonce (only if share is valid)
            // CRITICAL: Only set the nonce, do NOT modify the timestamp
            // The pool code shows: template.header.nonce = nonce (no timestamp modification)
            // Modifying the timestamp causes InvalidPoW errors because the block hash changes
            log::debug!("[BLOCK] Reconstructing block for job {} with nonce 0x{:x}", submission_job_id, submission.nonce);

            let mut mutable_block = job.template.block.clone();
            mutable_block.header.nonce = submission.nonce;
            // DO NOT modify timestamp - use the template's original timestamp
            // The ASIC mines on the prePoWHash + timestamp we sent, but the node expects
            // the original template timestamp when validating the block
            mutable_block.header.finalize();

            let header_hash = mutable_block.header.hash;
            log::debug!("[BLOCK] Reconstructed block hash: {} (job: {})", header_hash, submission_job_id);

            if invalid_share {
                // Send rejection response to client
                if let Some(ref response_tx) = submission.response_tx {
                    let _ = response_tx.send(false);
                }

                // Track rejected shares for vardiff adjustment (even if vardiff is disabled, we still need to reduce difficulty if too many rejects)
                let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;

                let should_reduce_difficulty = {
                    let mut vardiff_states_write = vardiff_states.write();

                    // Initialize vardiff state if it doesn't exist (even for rejected shares)
                    // Use current_job_to_check.difficulty as the starting point since that's what the miner is actually mining at
                    let job_difficulty = current_job_to_check.difficulty;
                    vardiff_states_write.entry(submission.client_addr).or_insert_with(|| {
                        log::debug!(
                            "Initialized vardiff state for {} with difficulty {} (from job)",
                            submission.client_addr,
                            job_difficulty
                        );
                        ClientVardiffState {
                            last_share: 0,
                            last_difficulty_change: now_ms,
                            current_difficulty: job_difficulty, // Use job difficulty - this is what the miner is mining at
                            share_count: 0,
                            connected_at: now_ms,
                            rejected_share_count: 0,
                            last_rejected_share: 0,
                        }
                    });

                    if let Some(vardiff_state) = vardiff_states_write.get_mut(&submission.client_addr) {
                        // If the job difficulty is higher than current difficulty, update it
                        // This handles the case where difficulty was increased but vardiff state wasn't updated
                        if job_difficulty > vardiff_state.current_difficulty {
                            log::debug!(
                                "Updating vardiff state difficulty from {} to {} (job difficulty is higher)",
                                vardiff_state.current_difficulty,
                                job_difficulty
                            );
                            vardiff_state.current_difficulty = job_difficulty;
                        }

                        // Track time since last share (accepted or rejected) for vardiff adjustment
                        // This allows vardiff to work even when most shares are rejected
                        let old_last_share = vardiff_state.last_share;
                        vardiff_state.last_share = now_ms; // Update last_share even for rejected shares
                        let time_since_last_share = now_ms.saturating_sub(old_last_share);

                        vardiff_state.rejected_share_count += 1;
                        vardiff_state.last_rejected_share = now_ms;

                        // If we have 50+ consecutive rejected shares, aggressively reduce difficulty
                        // This prevents the miner from being stuck at too-high difficulty
                        // Lowered threshold from 100 to 50 for faster response
                        if vardiff_state.rejected_share_count >= 50 {
                            let old_diff = vardiff_state.current_difficulty;
                            // Reduce difficulty by 75% (or to min_difficulty, whichever is higher)
                            // More aggressive reduction to get miner unstuck faster
                            let new_diff = (old_diff * 0.25).max(vardiff_config.min_difficulty);

                            // Apply Pow2 clamping if enabled
                            let final_diff = if vardiff_config.clamp_pow2 { Self::clamp_to_power_of_2(new_diff) } else { new_diff };

                            vardiff_state.current_difficulty = final_diff;
                            vardiff_state.last_difficulty_change = now_ms;
                            vardiff_state.rejected_share_count = 0; // Reset counter

                            drop(vardiff_states_write);

                            // Send difficulty update (always send, even if vardiff is disabled)
                            Self::send_difficulty_update(&submission.client_addr, final_diff, &clients);

                            log::warn!(
                                "[Vardiff] Too many rejected shares (50+) for {} - reducing difficulty: {:.0} -> {:.0}",
                                submission.client_addr,
                                old_diff,
                                final_diff
                            );

                            true // Signal that we reduced difficulty
                        } else {
                            // Even if we don't hit the 50+ threshold, try to adjust based on time between shares
                            // This allows vardiff to work even when most shares are rejected
                            drop(vardiff_states_write);

                            if time_since_last_share > 0 && vardiff_config.enabled {
                                Self::adjust_difficulty(
                                    &submission.client_addr,
                                    time_since_last_share,
                                    &vardiff_states,
                                    &vardiff_config,
                                    &clients,
                                );
                            }

                            false
                        }
                    } else {
                        false
                    }
                };

                if should_reduce_difficulty {
                    // Don't log the rejection again since we already logged the difficulty reduction
                    continue;
                }

                // Reduce verbosity for expected errors (low difficulty shares are normal)
                log::debug!(
                    "[REJECTED] Share from {} does not meet pool difficulty (job: {}, difficulty: {}, PoW value: {}, target: {})",
                    submission.client_addr,
                    submission_job_id,
                    job.difficulty,
                    pow_value,
                    pool_target
                );
                continue;
            }

            // Share meets pool difficulty - send acceptance response
            if let Some(ref response_tx) = submission.response_tx {
                let _ = response_tx.send(true);
            }

            // Update vardiff state if enabled and log share acceptance periodically
            if vardiff_config.enabled {
                let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;

                let (time_since_last_share, share_count) = {
                    let mut vardiff_states_write = vardiff_states.write();
                    if let Some(vardiff_state) = vardiff_states_write.get_mut(&submission.client_addr) {
                        let old_last_share = vardiff_state.last_share;
                        vardiff_state.last_share = now_ms;
                        vardiff_state.share_count += 1;
                        let count = vardiff_state.share_count;
                        (now_ms.saturating_sub(old_last_share), count)
                    } else {
                        // No vardiff state - initialize it
                        vardiff_states_write.insert(
                            submission.client_addr,
                            ClientVardiffState {
                                last_share: now_ms,
                                last_difficulty_change: now_ms,
                                current_difficulty: job.difficulty,
                                share_count: 1,
                                connected_at: now_ms,
                                rejected_share_count: 0,
                                last_rejected_share: 0,
                            },
                        );
                        (0, 1)
                    }
                };

                // Reset rejected share counter when a share is accepted
                if let Some(vardiff_state) = vardiff_states.write().get_mut(&submission.client_addr) {
                    vardiff_state.rejected_share_count = 0;
                }

                // Log share acceptance periodically (every 100 shares) or on first share
                if share_count == 1 || share_count % 100 == 0 {
                    log::info!(
                        "Share accepted from {} (job: {}, difficulty: {}, PoW: {}, total shares: {})",
                        submission.client_addr,
                        submission_job_id,
                        job.difficulty,
                        pow_value,
                        share_count
                    );
                }

                // Adjust difficulty if enough time has passed
                if time_since_last_share > 0 {
                    Self::adjust_difficulty(
                        &submission.client_addr,
                        time_since_last_share,
                        &vardiff_states,
                        &vardiff_config,
                        &clients,
                    );
                }
            } else {
                // Vardiff disabled - log periodically to show activity (every 100 shares)
                // Use a simple counter stored in a thread-local or static to track shares
                static SHARE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                let share_count = SHARE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

                if share_count == 1 || share_count % 100 == 0 {
                    log::info!(
                        "Share accepted from {} (job: {}, difficulty: {}, PoW: {}, total shares: {})",
                        submission.client_addr,
                        submission_job_id,
                        job.difficulty,
                        pow_value,
                        share_count
                    );
                }
            }

            // Check if share also meets network difficulty (from block header bits)
            // Network difficulty is much higher than pool difficulty
            // Only submit blocks that meet network difficulty to consensus
            // The pow_passed result from check_pow already validates against network target (from header.bits)
            if !pow_passed {
                log::debug!(
                    "Share from {} meets pool difficulty but not network difficulty (PoW value: {}) - not submitting to consensus",
                    submission.client_addr,
                    pow_value
                );
                continue;
            }

            // Block meets network PoW difficulty - submit to consensus
            log::info!(
                "Block from {} meets network PoW difficulty (PoW value: {}) - submitting to consensus",
                submission.client_addr,
                pow_value
            );

            // Convert mutable block to immutable block and submit via consensus API
            let block = mutable_block.to_immutable();
            let consensus_instance = consensus_manager.consensus();
            let consensus_session = consensus_instance.unguarded_session();
            let block_validation = consensus_session.validate_and_insert_block(block);

            // Spawn a task to handle the validation asynchronously
            let client_addr = submission.client_addr;
            let block_hash_str = header_hash.to_string();
            let block_hash_short = if block_hash_str.len() >= 16 { block_hash_str[..16].to_string() } else { block_hash_str.clone() };
            let block_hash_full = block_hash_str.clone();
            tokio::spawn(async move {
                // Wait for both validation tasks to complete
                let block_result = block_validation.block_task.await;
                let virtual_state_result = block_validation.virtual_state_task.await;

                match (block_result, virtual_state_result) {
                    (Ok(_), Ok(_)) => {
                        // Both tasks succeeded - block was successfully accepted!
                        log::info!("🎉 BLOCK FOUND! Hash: {}... - Mined by client {}", block_hash_short, client_addr);
                        log::info!(
                            "✓ Block {} successfully validated and inserted into chain by client {}",
                            block_hash_full,
                            client_addr
                        );
                    }
                    (Ok(_), Err(e)) => {
                        log::warn!("Block validation succeeded but virtual state update failed for client {}: {:?}", client_addr, e);
                        log::warn!("Block {} may not be fully processed", block_hash_full);
                    }
                    (Err(e), _) => {
                        log::warn!("Block validation failed for client {}: {:?}", client_addr, e);
                        log::warn!("Block {} was rejected (may be invalid, orphaned, or duplicate)", block_hash_full);
                    }
                }
            });
        }
    }

    /// Adjust difficulty based on share submission frequency (vardiff)
    fn adjust_difficulty(
        client_addr: &SocketAddr,
        time_since_last_share_ms: u64,
        vardiff_states: &Arc<RwLock<HashMap<SocketAddr, ClientVardiffState>>>,
        config: &VardiffConfig,
        clients: &Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
    ) {
        let mut vardiff_states_write = vardiff_states.write();
        let vardiff_state = match vardiff_states_write.get_mut(client_addr) {
            Some(s) => s,
            None => return, // No vardiff state for this client
        };

        let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;

        // Don't adjust too frequently (throttle changes)
        if now_ms.saturating_sub(vardiff_state.last_difficulty_change) < (config.change_interval * 1000) {
            return;
        }

        // Need at least 2 shares (accepted or rejected) to make meaningful adjustments
        // Count both accepted shares and rejected shares for this check
        let total_shares = vardiff_state.share_count + vardiff_state.rejected_share_count;
        if total_shares < 2 {
            return;
        }

        let time_since_last_share_seconds = time_since_last_share_ms as f64 / 1000.0;
        let target_time = config.target_time;
        let variance = (config.variance_percent / 100.0) * target_time;
        let min_target = target_time - variance;
        let max_target = target_time + variance;

        // CRITICAL SAFEGUARD: If no shares for too long, aggressively reduce difficulty
        const NO_SHARE_TIMEOUT_SECONDS: f64 = 3.0 * 60.0; // 3 minutes
        const CRITICAL_TIMEOUT_SECONDS: f64 = 5.0 * 60.0; // 5 minutes
        const EMERGENCY_RESET_MULTIPLIER: f64 = 0.5; // Drop to 50%

        if time_since_last_share_seconds >= CRITICAL_TIMEOUT_SECONDS {
            // Emergency: No shares for 5+ minutes - reset difficulty
            let emergency_diff = vardiff_state.current_difficulty * EMERGENCY_RESET_MULTIPLIER;
            let mut new_difficulty = emergency_diff.max(config.min_difficulty);

            // Apply Pow2 clamping if enabled (for IceRiver/Bitmain ASICs)
            if config.clamp_pow2 {
                new_difficulty = Self::clamp_to_power_of_2(new_difficulty);
            }

            let old_diff = vardiff_state.current_difficulty;
            vardiff_state.current_difficulty = new_difficulty;
            vardiff_state.last_difficulty_change = now_ms;

            drop(vardiff_states_write);
            Self::send_difficulty_update(client_addr, new_difficulty, clients);

            log::warn!(
                "[Vardiff] EMERGENCY RESET: No shares for {:.1}min - reduced difficulty for {}: {:.0} -> {:.0}",
                time_since_last_share_seconds / 60.0,
                client_addr,
                old_diff,
                new_difficulty
            );
            return;
        }

        let mut new_difficulty = vardiff_state.current_difficulty;
        let mut should_change = false;

        // Adjust based on share frequency
        if time_since_last_share_seconds < min_target {
            // Miner submitting too fast - increase difficulty
            let ratio = target_time / time_since_last_share_seconds;

            // SAFEGUARD: Cap difficulty increases if already high
            let current_percent_of_max = vardiff_state.current_difficulty / config.max_difficulty;
            let effective_max_change = if current_percent_of_max > 0.5 {
                1.5 // Cap at 1.5x if already >50% of max
            } else {
                config.max_change
            };

            let change_multiplier = ratio.min(effective_max_change);
            new_difficulty = vardiff_state.current_difficulty * change_multiplier;
            should_change = true;
        } else if time_since_last_share_seconds > max_target {
            // Miner submitting too slow - decrease difficulty
            let change_multiplier = if time_since_last_share_seconds >= NO_SHARE_TIMEOUT_SECONDS {
                // No shares for 3+ minutes - reduce more aggressively
                let timeout_ratio = time_since_last_share_seconds / NO_SHARE_TIMEOUT_SECONDS;
                (1.0 / timeout_ratio).min(0.7) // Reduce to at most 70% of current
            } else {
                // Smooth scaling
                let max_elapsed_ms = 5.0 * 60.0 * 1000.0;
                let time_ms_f64 = time_since_last_share_ms as f64;
                let capped_time = time_ms_f64.min(max_elapsed_ms);
                let time_weight = capped_time / max_elapsed_ms;
                let scaled_ratio = time_since_last_share_seconds / target_time;
                (1.0 / scaled_ratio).max(1.0 / config.max_change) * time_weight
            };

            new_difficulty = vardiff_state.current_difficulty * change_multiplier;
            should_change = true;
        }

        // Clamp to min/max difficulty
        new_difficulty = new_difficulty.max(config.min_difficulty).min(config.max_difficulty);

        // SAFEGUARD: Never exceed 90% of maxDifficulty
        let safe_max_diff = config.max_difficulty * 0.9;
        if new_difficulty > safe_max_diff {
            new_difficulty = safe_max_diff;
        }

        // Pow2 clamping for IceRiver/Bitmain ASICs (required for compatibility)
        // These ASICs only accept difficulties that are powers of 2
        if config.clamp_pow2 {
            new_difficulty = Self::clamp_to_power_of_2(new_difficulty);
        }

        // Only change if difference is significant (at least 5% change)
        let diff_percent = ((new_difficulty / vardiff_state.current_difficulty) - 1.0).abs() * 100.0;
        if should_change && diff_percent >= 5.0 {
            let old_diff = vardiff_state.current_difficulty;
            vardiff_state.current_difficulty = new_difficulty;
            vardiff_state.last_difficulty_change = now_ms;

            drop(vardiff_states_write);
            Self::send_difficulty_update(client_addr, new_difficulty, clients);

            // Only log significant changes (>20% change)
            let change_percent = ((new_difficulty - old_diff) / old_diff).abs() * 100.0;
            if change_percent >= 20.0 {
                log::info!(
                    "[Vardiff] Adjusted difficulty for {}: {:.0} -> {:.0} ({:.0}% change, interval: {:.1}s)",
                    client_addr,
                    old_diff,
                    new_difficulty,
                    change_percent,
                    time_since_last_share_seconds
                );
            }
        }
    }

    /// Send difficulty update notification to a client
    fn send_difficulty_update(
        client_addr: &SocketAddr,
        difficulty: f64,
        clients: &Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
    ) {
        let clients_read = clients.read();
        if let Some(notification_tx) = clients_read.get(client_addr) {
            let difficulty_notification = create_notification(
                "mining.set_difficulty".to_string(),
                vec![Value::Number(serde_json::Number::from_f64(difficulty).unwrap_or_else(|| serde_json::Number::from(1)))],
            );
            if notification_tx.send(difficulty_notification).is_err() {
                // Client disconnected - this is normal, don't log as warning
                log::debug!("Failed to send difficulty update to {} (client may have disconnected)", client_addr);
            } else {
                log::debug!("Sent difficulty update to {}: {}", client_addr, difficulty);
            }
        }
    }

    /// Periodic monitoring loop for stuck miners (vardiff)
    async fn vardiff_monitoring_loop(
        vardiff_states: Arc<RwLock<HashMap<SocketAddr, ClientVardiffState>>>,
        config: VardiffConfig,
        clients: Arc<RwLock<HashMap<SocketAddr, mpsc::UnboundedSender<StratumNotification>>>>,
    ) {
        const MONITOR_INTERVAL_SECONDS: u64 = 60; // Check every 60 seconds
        const STUCK_MINER_TIMEOUT_MS: u64 = 3 * 60 * 1000; // 3 minutes
        const CRITICAL_STUCK_TIMEOUT_MS: u64 = 5 * 60 * 1000; // 5 minutes

        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(MONITOR_INTERVAL_SECONDS));
        interval.tick().await; // Skip first tick

        loop {
            interval.tick().await;

            if !config.enabled {
                continue;
            }

            let duration = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
            let now_ms_u128 = duration.as_millis();
            let now_ms = now_ms_u128.min(u64::MAX as u128) as u64;

            let stuck_clients: Vec<(SocketAddr, u64)> = {
                let vardiff_states_read = vardiff_states.read();
                vardiff_states_read
                    .iter()
                    .filter_map(|(addr, state)| {
                        let time_since_last_share = now_ms.saturating_sub(state.last_share);
                        if time_since_last_share >= STUCK_MINER_TIMEOUT_MS {
                            Some((*addr, time_since_last_share))
                        } else {
                            None
                        }
                    })
                    .collect()
            };

            for (client_addr, time_since_last_share_ms) in stuck_clients {
                let time_since_last_share_seconds = time_since_last_share_ms as f64 / 1000.0;

                if time_since_last_share_ms >= CRITICAL_STUCK_TIMEOUT_MS {
                    log::warn!(
                        "[Vardiff Monitor] CRITICAL: Stuck miner detected {} (no shares for {:.1}min) - forcing emergency difficulty reset",
                        client_addr,
                        time_since_last_share_seconds / 60.0
                    );
                } else {
                    log::info!(
                        "[Vardiff Monitor] Stuck miner detected {} (no shares for {:.1}min) - reducing difficulty",
                        client_addr,
                        time_since_last_share_seconds / 60.0
                    );
                }

                // Trigger difficulty adjustment
                Self::adjust_difficulty(&client_addr, time_since_last_share_ms, &vardiff_states, &config, &clients);
            }
        }
    }

    /// Send job notification to a client
    /// NOTE: Does NOT send mining.set_difficulty - that should be sent separately:
    ///     - Once after subscribe (initial difficulty)
    ///     - When vardiff adjusts difficulty
    /// This matches the JS pool behavior where difficulty is not sent before every job
    fn send_job_notification(
        tx: &mpsc::UnboundedSender<StratumNotification>,
        job: &MiningJob,
        _difficulty: f64,
    ) -> Result<(), StratumError> {
        // Send job notification only (no difficulty update)
        // Format: [job_id, header_hash+timestamp] - hash and timestamp are concatenated
        // The header_hash already contains hash + timestamp (little-endian hex)
        let job_notification = create_notification(
            "mining.notify".to_string(),
            vec![Value::String(job.id.clone()), Value::String(job.header_hash.clone())],
        );
        log::info!(
            "Sending mining.notify for job {} - hash+timestamp: {} (length: {}, template_hash: {}, difficulty: {})",
            job.id,
            job.header_hash,
            job.header_hash.len(),
            job.template_hash,
            job.difficulty
        );
        tx.send(job_notification.clone()).map_err(|_| StratumError::Protocol("Failed to send job".to_string()))?;
        log::debug!("Sent mining.notify notification: {:?}", job_notification);

        Ok(())
    }
}
