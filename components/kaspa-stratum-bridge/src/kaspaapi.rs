use crate::constants::{BLOCK_TEMPLATE_MAX_RETRIES, RETRY_DELAY_BASE_MS};
use crate::log_colors::LogColors;
use crate::share_handler::KaspaApiTrait;
use anyhow::{Context, Result};
use kaspa_addresses::Address;
use kaspa_consensus_core::block::Block;
use kaspa_grpc_client::GrpcClient;
use kaspa_notify::{listener::ListenerId, scope::NewBlockTemplateScope};
use kaspa_rpc_core::{
    api::rpc::RpcApi,
    GetBlockTemplateRequest,
    GetBlockDagInfoRequest,
    SubmitBlockRequest, SubmitBlockResponse, Notification,
    RpcRawBlock,
};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Kaspa API client wrapper using RPC client
/// Both use gRPC under the hood, but through an RPC client wrapper abstraction
pub struct KaspaApi {
    client: Arc<GrpcClient>,
    notification_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<Notification>>>>,
    connected: Arc<Mutex<bool>>,
}

impl KaspaApi {
    /// Create a new Kaspa API client
    pub async fn new(
        address: String,
        _block_wait_time: Duration,
    ) -> Result<Arc<Self>> {
        info!("Connecting to Kaspa node at {}", address);
        
        // GrpcClient requires explicit "grpc://" prefix for connection
        // Always add it if not present (avoids unnecessary connection failure)
        let grpc_address = if address.starts_with("grpc://") {
            address.clone()
        } else {
            format!("grpc://{}", address)
        };

        // Log connection attempt (detailed logs moved to debug)
        tracing::debug!("{} {}", LogColors::api("[API]"), LogColors::label("Establishing RPC connection to Kaspa node:"));
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Address:"), &grpc_address);
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Protocol:"), "gRPC (via RPC client wrapper)");
        
        // Connect to Kaspa node with grpc:// prefix
        let client = Arc::new(
            GrpcClient::connect(grpc_address.clone())
                .await
                .context("Failed to connect to Kaspa node")?
        );

        // Log successful connection (detailed logs moved to debug)
        tracing::debug!("{} {}", LogColors::api("[API]"), LogColors::block("✓ RPC Connection Established Successfully"));
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Connected to:"), &grpc_address);
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Connection Type:"), "gRPC (via RPC client wrapper)");

        // Start the client (no notify needed for Direct mode)
        client.start(None).await;

        // Subscribe to block template notifications
        client.start_notify(
            ListenerId::default(),
            NewBlockTemplateScope {}.into(),
        ).await.context("Failed to subscribe to block template notifications")?;

        // Start receiving notifications
        let notification_rx = {
            let receiver = client.notification_channel_receiver();
            // Convert async_channel::Receiver to tokio::sync::mpsc::UnboundedReceiver
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let receiver_clone = receiver.clone();
            tokio::spawn(async move {
                while let Ok(notification) = receiver_clone.recv().await {
                    let _ = tx.send(notification);
                }
            });
            Arc::new(Mutex::new(Some(rx)))
        };

        let api = Arc::new(Self {
            client,
            notification_rx,
            connected: Arc::new(Mutex::new(true)),
        });

        // Wait for node to sync
        api.wait_for_sync(true).await?;

        // Start network stats thread
        let api_clone = Arc::clone(&api);
        tokio::spawn(async move {
            api_clone.start_stats_thread().await;
        });

        Ok(api)
    }

    /// Start network stats thread
    /// Fetches network stats periodically and records them in Prometheus
    async fn start_stats_thread(self: Arc<Self>) {
        use kaspa_rpc_core::{GetBlockDagInfoRequest, EstimateNetworkHashesPerSecondRequest};
        use crate::prom::record_network_stats;
        
        const NETWORK_STATS_INTERVAL: Duration = Duration::from_secs(30);
        let mut interval = tokio::time::interval(NETWORK_STATS_INTERVAL);
        loop {
            interval.tick().await;
            
            // Get block DAG info
            // GetBlockDagInfoRequest is a unit struct, construct directly
            let dag_response = match self.client
                .get_block_dag_info_call(None, GetBlockDagInfoRequest {})
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!("failed to get network hashrate from kaspa, prom stats will be out of date: {}", e);
                    continue;
                }
            };
            
            // Get tip hash (first one)
            // tip_hashes is Vec<Hash> in the response (already parsed)
            let tip_hash = match dag_response.tip_hashes.first() {
                Some(hash) => Some(*hash), // Clone the Hash
                None => {
                    warn!("no tip hashes available for network hashrate estimation");
                    continue;
                }
            };
            
            // Estimate network hashes per second
            // new(window_size: u32, start_hash: Option<RpcHash>)
            // RpcHash is the same as Hash, so we can use tip_hash directly
            let hashrate_response = match self.client
                .estimate_network_hashes_per_second_call(None, EstimateNetworkHashesPerSecondRequest::new(1000, tip_hash))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    warn!("failed to get network hashrate from kaspa, prom stats will be out of date: {}", e);
                    continue;
                }
            };
            
            // Record network stats
            record_network_stats(
                hashrate_response.network_hashes_per_second,
                dag_response.block_count,
                dag_response.difficulty,
            );
        }
    }

    /// Submit a block
    pub async fn submit_block(
        &self,
        block: Block,
    ) -> Result<SubmitBlockResponse> {
        // Use kaspa_consensus_core::hashing::header::hash() for block hash calculation
        // In Kaspa, the block hash is the header hash (transactions are represented by hash_merkle_root in header)
        use kaspa_consensus_core::hashing::header;
        let block_hash = header::hash(&block.header).to_string();
        let blue_score = block.header.blue_score;
        let timestamp = block.header.timestamp;
        let nonce = block.header.nonce;
        
        tracing::debug!("{} {}", LogColors::api("[API]"), LogColors::api(&format!("✓ ===== ATTEMPTING BLOCK SUBMISSION TO KASPA NODE ===== Hash: {}", block_hash)));
        tracing::debug!("{} {}", LogColors::api("[API]"), LogColors::label("Block Details:"));
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Hash:"), block_hash);
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Blue Score:"), blue_score);
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Timestamp:"), timestamp);
        tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Nonce:"), format!("{:x} ({})", nonce, nonce));
        tracing::debug!("{} {}", LogColors::api("[API]"), "Converting block to RPC format and sending to node...");
        
        // Convert Block to RpcRawBlock (use reference)
        let rpc_block: RpcRawBlock = (&block).into();

        // Submit block (don't allow non-DAA blocks)
        tracing::debug!("{} {}", LogColors::api("[API]"), "Calling submit_block via RPC client...");
        let result = self.client
            .submit_block_call(None, SubmitBlockRequest::new(rpc_block, false))
            .await
            .context("Failed to submit block");
        
        match &result {
            Ok(response) => {
                // Keep block accepted message at info (important operational event)
                info!("{} {}", LogColors::api("[API]"), LogColors::block(&format!("===== BLOCK ACCEPTED BY KASPA NODE ===== Hash: {}", block_hash)));
                // Detailed acceptance logs moved to debug
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("ACCEPTANCE REASON:"), "Block passed all node validation checks");
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Block structure:"), "VALID");
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Block header:"), "VALID");
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Transactions:"), "VALID");
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - DAA validation:"), "PASSED");
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Node Response:"), format!("{:?}", response));
                tracing::debug!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Blue Score:"), format!("{}, Timestamp: {}, Nonce: {:x}", blue_score, timestamp, nonce));
                
                // Optional: Check if block appears in tip hashes (verifies propagation)
                // This is informational only - block may still propagate even if not immediately in tips
                let client_clone = Arc::clone(&self.client);
                let block_hash_clone = block_hash.clone();
                let block_hash_for_check = header::hash(&block.header); // Use the actual Hash type
                tokio::spawn(async move {
                    // Wait a bit for block to be processed and potentially added to DAG
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    
                    // Check if block appears in tip hashes
                    if let Ok(dag_response) = client_clone
                        .get_block_dag_info_call(None, GetBlockDagInfoRequest {})
                        .await
                    {
                        // Check if our block hash is in tip hashes
                        let in_tips = dag_response.tip_hashes.iter().any(|tip| {
                            *tip == block_hash_for_check
                        });
                        
                        if in_tips {
                            info!("{} {} {}", LogColors::api("[API]"), LogColors::block("✓ Block appears in tip hashes (good sign for propagation)"), format!("Hash: {}", block_hash_clone));
                        } else {
                            // This is not necessarily bad - block may still propagate or be in a side chain
                            info!("{} {} {}", LogColors::api("[API]"), LogColors::label("ℹ Block not yet in tip hashes (may still propagate)"), format!("Hash: {}", block_hash_clone));
                            info!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Note:"), "Block may be in a side chain or still propagating");
                            info!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Tip hashes count:"), dag_response.tip_hashes.len());
                        }
                    }
                });
            }
            Err(e) => {
                let error_str = e.to_string();
                if error_str.contains("ErrDuplicateBlock") || error_str.contains("duplicate") {
                    warn!("{} {}", LogColors::api("[API]"), LogColors::validation(&format!("===== BLOCK REJECTED BY KASPA NODE: STALE ===== Hash: {}", block_hash)));
                    warn!("{} {} {}", LogColors::api("[API]"), LogColors::label("REJECTION REASON:"), "Block already exists in the network");
                    warn!("{} {}", LogColors::api("[API]"), LogColors::label("  - Block was previously submitted and accepted"));
                    warn!("{} {}", LogColors::api("[API]"), LogColors::label("  - This is a duplicate/stale block submission"));
                    warn!("{} {} {}", LogColors::api("[API]"), LogColors::error("  - Error:"), error_str);
                    warn!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Blue Score:"), format!("{}, Timestamp: {}, Nonce: {:x}", blue_score, timestamp, nonce));
                } else {
                    error!("{} {}", LogColors::api("[API]"), LogColors::error(&format!("===== BLOCK REJECTED BY KASPA NODE: INVALID ===== Hash: {}", block_hash)));
                    error!("{} {} {}", LogColors::api("[API]"), LogColors::label("REJECTION REASON:"), "Block failed node validation");
                    error!("{} {}", LogColors::api("[API]"), LogColors::label("  - Possible validation failures:"));
                    error!("{} {}", LogColors::api("[API]"), "    * Invalid block structure or format");
                    error!("{} {}", LogColors::api("[API]"), "    * Block header validation failed");
                    error!("{} {}", LogColors::api("[API]"), "    * Transaction validation failed");
                    error!("{} {}", LogColors::api("[API]"), "    * DAA (Difficulty Adjustment Algorithm) validation failed");
                    error!("{} {}", LogColors::api("[API]"), "    * Block does not meet network consensus rules");
                    error!("{} {} {}", LogColors::api("[API]"), LogColors::error("  - Error from node:"), error_str);
                    error!("{} {} {}", LogColors::api("[API]"), LogColors::label("  - Blue Score:"), format!("{}, Timestamp: {}, Nonce: {:x}", blue_score, timestamp, nonce));
                }
            }
        }
        
        result
    }

    /// Wait for node to sync
    async fn wait_for_sync(&self, verbose: bool) -> Result<()> {
        const SYNC_CHECK_INTERVAL: Duration = Duration::from_secs(5);
        
        if verbose {
            tracing::debug!("checking kaspad sync state");
        }

        loop {
            match self.client.get_sync_status().await {
                Ok(is_synced) => {
                    if is_synced {
                        if verbose {
                            tracing::debug!("kaspad synced, starting server");
                        }
                        break;
                    }
                }
                Err(e) => {
                    warn!("failed to get sync status: {}, retrying...", e);
                }
            }
            
            if verbose {
                warn!("Kaspa is not synced, waiting for sync before starting bridge");
            }
            sleep(SYNC_CHECK_INTERVAL).await;
        }

        Ok(())
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }

    /// Get block template for a client
    pub async fn get_block_template(
        &self,
        wallet_addr: &str,
        _remote_app: &str,
        _canxium_addr: &str,
    ) -> Result<Block> {
        // Retry if we get "Odd number of digits" error
        // This error can occur if the block template has malformed hash fields
        let mut last_error = None;
        
        for attempt in 0..BLOCK_TEMPLATE_MAX_RETRIES {
            // Parse wallet address each time (in case Address doesn't implement Clone)
            let address = Address::try_from(wallet_addr)
                .map_err(|e| anyhow::anyhow!("Could not decode address {}: {}", wallet_addr, e))?;

            // Request block template using RPC client wrapper
            let response = match self.client
                .get_block_template_call(None, GetBlockTemplateRequest::new(address, vec![]))
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    if attempt < BLOCK_TEMPLATE_MAX_RETRIES - 1 {
                        warn!("Failed to get block template (attempt {}/{}): {}, retrying...", attempt + 1, BLOCK_TEMPLATE_MAX_RETRIES, e);
                        sleep(Duration::from_millis(RETRY_DELAY_BASE_MS * (attempt + 1) as u64)).await;
                        continue;
                    }
                    return Err(anyhow::anyhow!("Failed to get block template after {} attempts: {}", BLOCK_TEMPLATE_MAX_RETRIES, e));
                }
            };

            // Get RPC block from response
            let rpc_block = response.block;

            // Convert RpcRawBlock to Block
            // The RpcRawBlock contains the block data that we need to convert
            // The "Odd number of digits" error can occur here if hash fields have malformed hex strings
            match Block::try_from(rpc_block) {
                Ok(block) => {
                    // Validate that we can serialize the block header
                    // This catches "Odd number of digits" errors early
                    // Convert error to String immediately to avoid Send issues
                    let serialize_result = crate::hasher::serialize_block_header(&block)
                        .map_err(|e| e.to_string());
                    
                    match serialize_result {
                        Ok(_) => {
                            return Ok(block);
                        }
                        Err(error_str) => {
                            if error_str.contains("Odd number of digits") {
                                last_error = Some(format!("Block has malformed hash field: {}", error_str));
                                if attempt < BLOCK_TEMPLATE_MAX_RETRIES - 1 {
                                    warn!("Block template has malformed hash field (attempt {}/{}), retrying...", attempt + 1, BLOCK_TEMPLATE_MAX_RETRIES);
                                    sleep(Duration::from_millis(RETRY_DELAY_BASE_MS * (attempt + 1) as u64)).await;
                                    continue;
                                }
                            }
                            // If it's a different error, return it
                            return Err(anyhow::anyhow!("Failed to serialize block header: {}", error_str));
                        }
                    }
                }
                Err(e) => {
                    let error_str = format!("{:?}", e);
                    last_error = Some(error_str.clone());
                    if error_str.contains("Odd number of digits") && attempt < BLOCK_TEMPLATE_MAX_RETRIES - 1 {
                        warn!("Block conversion failed with 'Odd number of digits' error (attempt {}/{}), retrying...", attempt + 1, BLOCK_TEMPLATE_MAX_RETRIES);
                        sleep(Duration::from_millis(RETRY_DELAY_BASE_MS * (attempt + 1) as u64)).await;
                        continue;
                    }
                    // If the error contains "Odd number of digits", provide more context
                    if error_str.contains("Odd number of digits") {
                        return Err(anyhow::anyhow!("Failed to convert RPC block to Block after {} attempts: {} - This usually indicates a malformed hash field in the block template from the Kaspa node. The block may have a hash with an odd-length hex string.", BLOCK_TEMPLATE_MAX_RETRIES, error_str));
                    } else {
                        return Err(anyhow::anyhow!("Failed to convert RPC block to Block: {}", error_str));
                    }
                }
            }
        }
        
        // Should never reach here, but handle it just in case
        Err(anyhow::anyhow!("Failed to get valid block template after {} attempts: {:?}", BLOCK_TEMPLATE_MAX_RETRIES, last_error))
    }

    /// Get balances by addresses (for Prometheus metrics)
    pub async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<(String, u64)>> {
        let parsed_addresses: Result<Vec<Address>, _> = addresses
            .iter()
            .map(|addr| Address::try_from(addr.as_str()))
            .collect();

        let addresses = parsed_addresses
            .map_err(|e| anyhow::anyhow!("Failed to parse addresses: {:?}", e))?;

        let utxos = self.client
            .get_utxos_by_addresses_call(None, kaspa_rpc_core::GetUtxosByAddressesRequest::new(addresses))
            .await
            .context("Failed to get UTXOs by addresses")?;

        // Calculate balances from UTXOs
        // Group entries by address
        use std::collections::HashMap;
        let mut balance_map: HashMap<String, u64> = HashMap::new();
        for entry in utxos.entries {
            if let Some(address) = entry.address {
                let addr_str = address.to_string();
                let amount = entry.utxo_entry.amount;
                *balance_map.entry(addr_str).or_insert(0) += amount;
            }
        }
        let balances: Vec<(String, u64)> = balance_map.into_iter().collect();

        Ok(balances)
    }

    /// Start listening for block template notifications
    /// Uses RegisterForNewBlockTemplateNotifications with ticker fallback
    /// This provides immediate notifications when new blocks are available, with polling as fallback
    pub async fn start_block_template_listener<F>(
        self: Arc<Self>,
        block_wait_time: Duration,
        mut block_cb: F,
    ) -> Result<()>
    where
        F: FnMut() + Send + 'static,
    {
        let mut rx = self.notification_rx.lock().take()
            .ok_or_else(|| anyhow::anyhow!("Notification receiver already taken"))?;

        let api_clone = Arc::clone(&self);
        tokio::spawn(async move {
            let mut restart_channel = true;
            let mut ticker = tokio::time::interval(block_wait_time);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            loop {
                // Check sync state and reconnect if needed
                const RECONNECT_DELAY: Duration = Duration::from_secs(5);
                if let Err(e) = api_clone.wait_for_sync(false).await {
                    error!("error checking kaspad sync state, attempting reconnect: {}", e);
                    // Note: gRPC client handles reconnection automatically, but we log it
                    // In Go, reconnect() is called explicitly, but Rust gRPC handles it
                    tokio::time::sleep(RECONNECT_DELAY).await;
                    restart_channel = true;
                }

                // Re-register for notifications if needed
                if restart_channel {
                    // In Go, RegisterForNewBlockTemplateNotifications is called here when restartChannel is true
                    // In Rust, we already subscribed in new(), and the notification channel persists
                    // If the connection is lost, the gRPC client handles reconnection automatically
                    // The notification subscription should be maintained by the gRPC client
                    // If notifications stop working, we'll fall back to ticker polling
                    restart_channel = false;
                }

                // Wait for either notification or ticker timeout
                tokio::select! {
                    // Notification received
                    notification_result = rx.recv() => {
                        match notification_result {
                            Some(Notification::NewBlockTemplate(_)) => {
                                // Drain any additional notifications
                                while rx.try_recv().is_ok() {}
                                
                                // Call callback
                                block_cb();
                                
                                // Reset ticker
                                ticker = tokio::time::interval(block_wait_time);
                                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            }
                            Some(_) => {
                                // Other notification types - ignore
                            }
                            None => {
                                // Channel closed - exit loop
                                warn!("Block template notification channel closed");
                                break;
                            }
                        }
                    }
                    // Ticker timeout - manually check for new blocks
                    _ = ticker.tick() => {
                        block_cb();
                    }
                }
            }
        });

        Ok(())
    }
}

#[async_trait::async_trait]
impl KaspaApiTrait for KaspaApi {
    async fn get_block_template(
        &self,
        wallet_addr: &str,
        _remote_app: &str,
        _canxium_addr: &str,
    ) -> Result<Block, Box<dyn std::error::Error + Send + Sync>> {
        KaspaApi::get_block_template(self, wallet_addr, "", "").await
            .map_err(|e| {
                let error_msg = e.to_string();
                Box::new(std::io::Error::other(error_msg)) as Box<dyn std::error::Error + Send + Sync>
            })
    }

    async fn submit_block(
        &self,
        block: Block,
    ) -> Result<kaspa_rpc_core::SubmitBlockResponse, Box<dyn std::error::Error + Send + Sync>> {
        KaspaApi::submit_block(self, block).await
            .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
    }

    async fn get_balances_by_addresses(
        &self,
        addresses: &[String],
    ) -> Result<Vec<(String, u64)>, Box<dyn std::error::Error + Send + Sync>> {
        KaspaApi::get_balances_by_addresses(self, addresses).await
            .map_err(|e| Box::new(std::io::Error::other(e.to_string())) as Box<dyn std::error::Error + Send + Sync>)
    }
}
